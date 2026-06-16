//! MQTT client wrapper using rumqttc.

use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS};
use std::{
    future::Future,
    pin::Pin,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, watch};
use tracing::{debug, error, info, warn};

use crate::config::MqttConfig;
use crate::error::MqttError;
use crate::mqtt::discovery::HaDiscoveryPayload;

const PUBLISH_TIMEOUT: Duration = Duration::from_secs(5);

/// Boxed future used by the MQTT publisher trait.
pub type MqttPublishFuture<'a> = Pin<Box<dyn Future<Output = Result<(), MqttError>> + Send + 'a>>;

/// MQTT publishing interface used by the application.
pub trait MqttPublisher: Send + Sync {
    /// Publish Home Assistant discovery config.
    fn publish_discovery<'a>(&'a self, client_id: &'a str) -> MqttPublishFuture<'a>;

    /// Publish availability status.
    fn publish_availability(&self, online: bool) -> MqttPublishFuture<'_>;

    /// Publish motion state.
    fn publish_motion(&self, motion_detected: bool) -> MqttPublishFuture<'_>;

    /// Gracefully disconnect from MQTT.
    fn disconnect(&self) -> MqttPublishFuture<'_>;
}

/// MQTT client wrapper for motion sensor publishing.
pub struct MqttClient {
    client: AsyncClient,
    device_name: String,
    display_name: String,
    ha_prefix: String,
    ha_discovery: bool,
}

/// Events from the MQTT event loop.
#[derive(Debug, Clone)]
pub enum MqttEvent {
    Connected,
    Disconnected,
    Error(String),
}

impl MqttClient {
    /// Create a new MQTT client and start the event loop.
    ///
    /// Returns the client and a receiver for connection events.
    pub fn new(
        config: &MqttConfig,
        device_name: &str,
        display_name: &str,
        client_id: &str,
    ) -> Result<(Self, mpsc::Receiver<MqttEvent>, watch::Receiver<Instant>), MqttError> {
        let mut options = MqttOptions::new(client_id, &config.host, config.port);

        options.set_keep_alive(Duration::from_secs(config.keep_alive_secs));

        // Set credentials if provided
        if let (Some(username), Some(password)) = (&config.username, &config.password) {
            options.set_credentials(username, password);
        }

        // Set last will for availability
        let availability_topic = format!(
            "{}/binary_sensor/{}/availability",
            config.ha_discovery_prefix, device_name
        );
        options.set_last_will(rumqttc::LastWill::new(
            &availability_topic,
            b"offline".to_vec(),
            QoS::AtLeastOnce,
            true,
        ));
        info!(topic = %availability_topic, retain = true, "Configured MQTT Last Will");

        let (client, eventloop) = AsyncClient::new(options, 10);

        let (event_tx, event_rx) = mpsc::channel(10);
        let (progress_tx, progress_rx) = watch::channel(Instant::now());

        // Spawn the event loop handler
        tokio::spawn(async move {
            Self::run_eventloop(eventloop, event_tx, progress_tx).await;
        });

        let mqtt_client = Self {
            client,
            device_name: device_name.to_string(),
            display_name: display_name.to_string(),
            ha_prefix: config.ha_discovery_prefix.clone(),
            ha_discovery: config.ha_discovery,
        };

        Ok((mqtt_client, event_rx, progress_rx))
    }

    /// Run the MQTT event loop.
    async fn run_eventloop(
        mut eventloop: EventLoop,
        event_tx: mpsc::Sender<MqttEvent>,
        progress_tx: watch::Sender<Instant>,
    ) {
        let mut connected = false;
        let mut has_connected_once = false;
        let mut reconnect_attempt = 0u64;
        let mut consecutive_errors = 0u64;

        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(ack))) => {
                    let _ = progress_tx.send(Instant::now());
                    consecutive_errors = 0;

                    if ack.code == rumqttc::ConnectReturnCode::Success {
                        let reconnect = has_connected_once && !connected;
                        info!(reconnect, "MQTT connected successfully");
                        connected = true;
                        has_connected_once = true;
                        reconnect_attempt = 0;
                        if let Err(e) = event_tx.send(MqttEvent::Connected).await {
                            error!(error = %e, "MQTT event receiver closed");
                            break;
                        }
                    } else {
                        warn!(return_code = ?ack.code, "MQTT connection failed");
                        if let Err(e) = event_tx
                            .send(MqttEvent::Error(format!(
                                "MQTT connection rejected: {:?}",
                                ack.code
                            )))
                            .await
                        {
                            error!(error = %e, "MQTT event receiver closed");
                            break;
                        }
                    }
                }
                Ok(Event::Incoming(Packet::PubAck(_))) => {
                    let _ = progress_tx.send(Instant::now());
                    debug!("MQTT publish acknowledged");
                }
                Ok(Event::Incoming(Packet::PingResp)) => {
                    let _ = progress_tx.send(Instant::now());
                    debug!("MQTT ping response");
                }
                Ok(Event::Outgoing(_)) => {
                    let _ = progress_tx.send(Instant::now());
                    // Ignore outgoing events
                }
                Ok(event) => {
                    let _ = progress_tx.send(Instant::now());
                    debug!(?event, "MQTT event");
                }
                Err(e) => {
                    let _ = progress_tx.send(Instant::now());
                    consecutive_errors = consecutive_errors.saturating_add(1);

                    if connected {
                        warn!(error = %e, "MQTT disconnected");
                        connected = false;
                        reconnect_attempt = 0;
                        if let Err(send_error) = event_tx.send(MqttEvent::Disconnected).await {
                            error!(error = %send_error, "MQTT event receiver closed");
                            break;
                        }
                    } else {
                        reconnect_attempt = reconnect_attempt.saturating_add(1);
                        if reconnect_attempt == 1 || reconnect_attempt % 30 == 0 {
                            warn!(
                                attempt = reconnect_attempt,
                                error = %e,
                                "MQTT reconnect attempt failed"
                            );
                        } else {
                            debug!(attempt = reconnect_attempt, error = %e, "MQTT reconnect attempt failed");
                        }
                    }

                    if consecutive_errors == 1 || consecutive_errors % 30 == 0 {
                        match event_tx.try_send(MqttEvent::Error(e.to_string())) {
                            Ok(()) => {}
                            Err(mpsc::error::TrySendError::Full(_)) => {
                                debug!("MQTT event channel full; dropping MQTT error event");
                            }
                            Err(mpsc::error::TrySendError::Closed(_)) => {
                                error!("MQTT event receiver closed");
                                break;
                            }
                        }
                    }
                    // rumqttc handles reconnection automatically
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }

    async fn publish_payload(
        &self,
        topic: String,
        payload: &str,
        retain: bool,
        publish_type: &'static str,
    ) -> Result<(), MqttError> {
        info!(
            topic = %topic,
            retain,
            publish_type,
            payload_len = payload.len(),
            "Queueing MQTT publish"
        );
        debug!(topic = %topic, publish_type, payload = %payload, "MQTT publish payload");

        match tokio::time::timeout(
            PUBLISH_TIMEOUT,
            self.client
                .publish(&topic, QoS::AtLeastOnce, retain, payload.as_bytes()),
        )
        .await
        {
            Ok(Ok(())) => {
                info!(topic = %topic, retain, publish_type, "MQTT publish queued");
                Ok(())
            }
            Ok(Err(e)) => {
                warn!(topic = %topic, retain, publish_type, error = %e, "MQTT publish failed");
                Err(MqttError::PublishFailed(e))
            }
            Err(_) => {
                warn!(
                    topic = %topic,
                    retain,
                    publish_type,
                    timeout_secs = PUBLISH_TIMEOUT.as_secs(),
                    "MQTT publish timed out"
                );
                Err(MqttError::PublishTimedOut {
                    topic,
                    timeout_secs: PUBLISH_TIMEOUT.as_secs(),
                })
            }
        }
    }

    fn availability_topic(&self) -> String {
        format!(
            "{}/binary_sensor/{}/availability",
            self.ha_prefix, self.device_name
        )
    }

    fn state_topic(&self) -> String {
        format!(
            "{}/binary_sensor/{}/state",
            self.ha_prefix, self.device_name
        )
    }

    /// Publish Home Assistant discovery config.
    pub async fn publish_discovery(&self, client_id: &str) -> Result<(), MqttError> {
        if !self.ha_discovery {
            debug!("Home Assistant discovery disabled");
            return Ok(());
        }

        let payload = HaDiscoveryPayload::motion_sensor(
            &self.device_name,
            &self.display_name,
            client_id,
            &self.ha_prefix,
        );

        let topic = HaDiscoveryPayload::config_topic(&self.device_name, &self.ha_prefix);
        let json = payload
            .to_json()
            .map_err(|e| MqttError::InvalidConfig(e.to_string()))?;

        info!(topic = %topic, retain = true, "Publishing HA discovery");
        debug!(payload = %json, "Discovery payload");

        self.publish_payload(topic, &json, true, "discovery").await
    }

    /// Publish availability status.
    pub async fn publish_availability(&self, online: bool) -> Result<(), MqttError> {
        let topic = self.availability_topic();
        let payload = if online { "online" } else { "offline" };

        info!(topic = %topic, payload, retain = true, "Publishing availability");
        self.publish_payload(topic, payload, true, "availability")
            .await
    }

    /// Publish motion state.
    pub async fn publish_motion(&self, motion_detected: bool) -> Result<(), MqttError> {
        let topic = self.state_topic();
        let payload = if motion_detected { "ON" } else { "OFF" };

        info!(
            topic = %topic,
            payload,
            retain = true,
            "Publishing motion state"
        );

        self.publish_payload(topic, payload, true, "state").await
    }

    /// Gracefully disconnect from the broker.
    pub async fn disconnect(&self) -> Result<(), MqttError> {
        info!("Disconnecting from MQTT broker");

        // Publish offline status before disconnecting
        if let Err(e) = self.publish_availability(false).await {
            warn!("Failed to publish offline status: {}", e);
        }

        self.client.disconnect().await?;
        Ok(())
    }
}

impl MqttPublisher for MqttClient {
    fn publish_discovery<'a>(&'a self, client_id: &'a str) -> MqttPublishFuture<'a> {
        Box::pin(Self::publish_discovery(self, client_id))
    }

    fn publish_availability(&self, online: bool) -> MqttPublishFuture<'_> {
        Box::pin(Self::publish_availability(self, online))
    }

    fn publish_motion(&self, motion_detected: bool) -> MqttPublishFuture<'_> {
        Box::pin(Self::publish_motion(self, motion_detected))
    }

    fn disconnect(&self) -> MqttPublishFuture<'_> {
        Box::pin(Self::disconnect(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rumqttc::Request;

    fn test_client() -> (MqttClient, flume::Receiver<Request>) {
        let (tx, rx) = flume::bounded(10);
        let client = MqttClient {
            client: AsyncClient::from_senders(tx),
            device_name: "officescreen".to_string(),
            display_name: "Office Screen".to_string(),
            ha_prefix: "homeassistant".to_string(),
            ha_discovery: true,
        };

        (client, rx)
    }

    async fn next_publish(rx: &flume::Receiver<Request>) -> rumqttc::mqttbytes::v4::Publish {
        match rx.recv_async().await.expect("request should be queued") {
            Request::Publish(publish) => publish,
            request => panic!("expected publish request, got {request:?}"),
        }
    }

    #[tokio::test]
    async fn publish_motion_uses_retained_home_assistant_payload() {
        let (client, rx) = test_client();

        client
            .publish_motion(true)
            .await
            .expect("motion publish should queue");

        let publish = next_publish(&rx).await;
        assert_eq!(
            publish.topic,
            "homeassistant/binary_sensor/officescreen/state"
        );
        assert_eq!(&publish.payload[..], b"ON");
        assert!(publish.retain);
    }

    #[tokio::test]
    async fn publish_discovery_and_availability_are_retained() {
        let (client, rx) = test_client();

        client
            .publish_discovery("mrpir-officescreen")
            .await
            .expect("discovery publish should queue");
        client
            .publish_availability(true)
            .await
            .expect("availability publish should queue");

        let discovery = next_publish(&rx).await;
        assert_eq!(
            discovery.topic,
            "homeassistant/binary_sensor/officescreen/config"
        );
        assert!(discovery.retain);

        let availability = next_publish(&rx).await;
        assert_eq!(
            availability.topic,
            "homeassistant/binary_sensor/officescreen/availability"
        );
        assert_eq!(&availability.payload[..], b"online");
        assert!(availability.retain);
    }
}
