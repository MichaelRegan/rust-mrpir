//! MQTT client wrapper using rumqttc.

use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::config::MqttConfig;
use crate::error::MqttError;
use crate::mqtt::discovery::HaDiscoveryPayload;

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
    pub async fn new(
        config: &MqttConfig,
        device_name: &str,
        display_name: &str,
        client_id: &str,
    ) -> Result<(Self, mpsc::Receiver<MqttEvent>), MqttError> {
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
            "offline".as_bytes().to_vec(),
            QoS::AtLeastOnce,
            true,
        ));

        let (client, eventloop) = AsyncClient::new(options, 10);

        let (event_tx, event_rx) = mpsc::channel(10);

        // Spawn the event loop handler
        let availability_topic_clone = availability_topic.clone();
        tokio::spawn(async move {
            Self::run_eventloop(eventloop, event_tx, availability_topic_clone).await;
        });

        let mqtt_client = Self {
            client,
            device_name: device_name.to_string(),
            display_name: display_name.to_string(),
            ha_prefix: config.ha_discovery_prefix.clone(),
            ha_discovery: config.ha_discovery,
        };

        Ok((mqtt_client, event_rx))
    }

    /// Run the MQTT event loop.
    async fn run_eventloop(
        mut eventloop: EventLoop,
        event_tx: mpsc::Sender<MqttEvent>,
        availability_topic: String,
    ) {
        let mut connected = false;

        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(ack))) => {
                    if ack.code == rumqttc::ConnectReturnCode::Success {
                        info!("MQTT connected successfully");
                        connected = true;
                        let _ = event_tx.send(MqttEvent::Connected).await;
                    } else {
                        warn!("MQTT connection failed: {:?}", ack.code);
                    }
                }
                Ok(Event::Incoming(Packet::PubAck(_))) => {
                    debug!("MQTT publish acknowledged");
                }
                Ok(Event::Incoming(Packet::PingResp)) => {
                    debug!("MQTT ping response");
                }
                Ok(Event::Outgoing(_)) => {
                    // Ignore outgoing events
                }
                Ok(event) => {
                    debug!("MQTT event: {:?}", event);
                }
                Err(e) => {
                    if connected {
                        warn!("MQTT disconnected: {}", e);
                        connected = false;
                        let _ = event_tx.send(MqttEvent::Disconnected).await;
                    } else {
                        debug!("MQTT connection attempt failed: {}", e);
                    }
                    // rumqttc handles reconnection automatically
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
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

        info!("Publishing HA discovery to {}", topic);
        debug!("Discovery payload: {}", json);

        self.client
            .publish(&topic, QoS::AtLeastOnce, true, json.as_bytes())
            .await?;

        Ok(())
    }

    /// Publish availability status.
    pub async fn publish_availability(&self, online: bool) -> Result<(), MqttError> {
        let topic = format!(
            "{}/binary_sensor/{}/availability",
            self.ha_prefix, self.device_name
        );
        let payload = if online { "online" } else { "offline" };

        debug!("Publishing availability: {}", payload);
        self.client
            .publish(&topic, QoS::AtLeastOnce, true, payload.as_bytes())
            .await?;

        Ok(())
    }

    /// Publish motion state.
    pub async fn publish_motion(&self, motion_detected: bool) -> Result<(), MqttError> {
        let topic = format!(
            "{}/binary_sensor/{}/state",
            self.ha_prefix, self.device_name
        );
        let payload = if motion_detected { "ON" } else { "OFF" };

        info!(
            "Motion {}: publishing to {}",
            if motion_detected {
                "detected"
            } else {
                "cleared"
            },
            topic
        );

        self.client
            .publish(&topic, QoS::AtLeastOnce, false, payload.as_bytes())
            .await?;

        Ok(())
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
