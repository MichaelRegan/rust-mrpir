//! Application state and event handling.

use anyhow::Result;
use std::time::Instant;
use tracing::{error, info, warn};

use crate::config::Config;
use crate::error::MqttError;
use crate::mqtt::{MqttEvent, MqttPublisher};
use crate::screen::ScreenManager;
use crate::sensor::MotionEvent;
use crate::time_events::NightModeManager;

/// Application state.
pub struct App {
    config: Config,
    pub mqtt_client: Option<Box<dyn MqttPublisher>>,
    screen_manager: Option<ScreenManager>,
    night_mode: NightModeManager,
    motion_active: bool,
    last_state_publish: Option<Instant>,
    last_lifecycle_publish: Option<Instant>,
}

impl App {
    /// Create a new application instance.
    #[must_use]
    pub fn new(config: Config) -> Self {
        let night_mode = NightModeManager::new(
            &config.night_mode,
            config
                .location
                .latitude
                .is_some()
                .then_some(&config.location),
        );

        let screen_manager = if config.screen.enabled {
            match ScreenManager::new(&config.screen) {
                Ok(manager) => Some(manager),
                Err(e) => {
                    warn!(error = %e, "Screen control not available");
                    None
                }
            }
        } else {
            None
        };

        Self {
            config,
            mqtt_client: None,
            screen_manager,
            night_mode,
            motion_active: false,
            last_state_publish: None,
            last_lifecycle_publish: None,
        }
    }

    /// Initialize the current motion state from GPIO.
    pub async fn initialize_motion_state(&mut self, motion_detected: bool) {
        self.set_motion_state(motion_detected, "startup");
        info!(
            gpio_active = motion_detected,
            state = Self::motion_payload(motion_detected),
            "Initial PIR state loaded"
        );

        if motion_detected {
            self.wake_screen().await;
        }
    }

    /// Return the current motion state.
    #[cfg(test)]
    #[must_use]
    pub const fn motion_active(&self) -> bool {
        self.motion_active
    }

    /// Last successful state publish enqueue time.
    #[must_use]
    pub const fn last_state_publish(&self) -> Option<Instant> {
        self.last_state_publish
    }

    /// Last successful discovery or availability publish enqueue time.
    #[must_use]
    pub const fn last_lifecycle_publish(&self) -> Option<Instant> {
        self.last_lifecycle_publish
    }

    /// Publish the currently known motion state to MQTT.
    pub async fn publish_current_motion_state(
        &mut self,
        reason: &'static str,
    ) -> Result<(), MqttError> {
        if !self.config.mqtt.enabled {
            return Ok(());
        }

        let Some(client) = self.mqtt_client.as_ref() else {
            warn!(
                reason,
                state = Self::motion_payload(self.motion_active),
                "MQTT client unavailable; motion state not published"
            );
            return Ok(());
        };

        let state = self.motion_active;
        match client.publish_motion(state).await {
            Ok(()) => {
                self.last_state_publish = Some(Instant::now());
                info!(
                    reason,
                    state = Self::motion_payload(state),
                    retain = true,
                    "MQTT motion state publish queued"
                );
                Ok(())
            }
            Err(e) => {
                warn!(
                    reason,
                    state = Self::motion_payload(state),
                    error = %e,
                    "MQTT motion state publish failed"
                );
                Err(e)
            }
        }
    }

    /// Handle a motion event.
    pub async fn handle_motion(&mut self, event: MotionEvent) -> Result<()> {
        match event {
            MotionEvent::Detected => {
                let changed = self.set_motion_state(true, "pir_detected");
                info!(state = "detected", changed, "Motion event");

                if let Err(e) = self.publish_current_motion_state("pir_detected").await {
                    warn!(error = %e, "Failed to publish detected motion state");
                }

                if changed {
                    self.wake_screen().await;
                }
            }
            MotionEvent::Cleared => {
                let changed = self.set_motion_state(false, "pir_cleared");
                info!(state = "cleared", changed, "Motion event");

                if let Err(e) = self.publish_current_motion_state("pir_cleared").await {
                    warn!(error = %e, "Failed to publish cleared motion state");
                }

                if changed {
                    self.adjust_screen_after_clear().await;
                }
            }
        }

        Ok(())
    }

    /// Handle MQTT connection events.
    pub async fn handle_mqtt_event(&mut self, event: MqttEvent) -> Result<()> {
        match event {
            MqttEvent::Connected => {
                info!("MQTT connected, republishing discovery, availability, and current state");
                if let Some(ref client) = self.mqtt_client {
                    let client_id = self.config.client_id();
                    if let Err(e) = client.publish_discovery(&client_id).await {
                        error!(error = %e, "Failed to publish discovery");
                    } else {
                        self.last_lifecycle_publish = Some(Instant::now());
                    }
                    if let Err(e) = client.publish_availability(true).await {
                        error!(error = %e, "Failed to publish availability");
                    } else {
                        self.last_lifecycle_publish = Some(Instant::now());
                    }
                } else if self.config.mqtt.enabled {
                    warn!("MQTT connected event received but client is unavailable");
                }

                if let Err(e) = self.publish_current_motion_state("mqtt_connected").await {
                    error!(error = %e, "Failed to publish current state after MQTT connect");
                }
            }
            MqttEvent::Disconnected => {
                warn!("MQTT disconnected, will reconnect automatically");
            }
            MqttEvent::Error(msg) => {
                error!(message = %msg, "MQTT error");
            }
        }
        Ok(())
    }

    /// Gracefully disconnect from MQTT.
    pub async fn shutdown(&self) {
        if let Some(ref client) = self.mqtt_client {
            if let Err(e) = client.disconnect().await {
                warn!(error = %e, "Error disconnecting from MQTT");
            }
        }
    }

    const fn motion_payload(motion_detected: bool) -> &'static str {
        if motion_detected {
            "ON"
        } else {
            "OFF"
        }
    }

    fn set_motion_state(&mut self, motion_detected: bool, reason: &'static str) -> bool {
        let previous = self.motion_active;
        self.motion_active = motion_detected;
        let changed = previous ^ motion_detected;

        if changed {
            info!(
                reason,
                previous_state = Self::motion_payload(previous),
                new_state = Self::motion_payload(motion_detected),
                "Motion state transition"
            );
            true
        } else {
            info!(
                reason,
                state = Self::motion_payload(motion_detected),
                "Motion state unchanged"
            );
            false
        }
    }

    async fn wake_screen(&mut self) {
        if let Some(ref mut manager) = self.screen_manager {
            if let Err(e) = manager.on_motion().await {
                warn!(error = %e, "Failed to wake screen");
            }
        }
    }

    async fn adjust_screen_after_clear(&mut self) {
        if let Some(ref mut manager) = self.screen_manager {
            let result = if self.night_mode.is_night_mode() {
                manager.on_night_mode().await
            } else {
                manager.on_motion_timeout().await
            };
            if let Err(e) = result {
                warn!(error = %e, "Failed to adjust screen");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mqtt::MqttPublishFuture;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum FakePublish {
        Discovery(String),
        Availability(bool),
        Motion(bool),
        Disconnect,
    }

    #[derive(Clone)]
    struct FakeMqttPublisher {
        records: Arc<Mutex<Vec<FakePublish>>>,
        fail_motion: bool,
    }

    impl FakeMqttPublisher {
        fn new(fail_motion: bool) -> Self {
            Self {
                records: Arc::new(Mutex::new(Vec::new())),
                fail_motion,
            }
        }

        fn records(&self) -> Vec<FakePublish> {
            self.records
                .lock()
                .expect("records mutex should not be poisoned")
                .clone()
        }

        fn push(&self, publish: FakePublish) -> Result<(), MqttError> {
            let mut records = self
                .records
                .lock()
                .map_err(|_| MqttError::InvalidConfig("records mutex poisoned".to_string()))?;
            records.push(publish);
            Ok(())
        }
    }

    impl MqttPublisher for FakeMqttPublisher {
        fn publish_discovery<'a>(&'a self, client_id: &'a str) -> MqttPublishFuture<'a> {
            Box::pin(async move {
                self.push(FakePublish::Discovery(client_id.to_string()))?;
                Ok(())
            })
        }

        fn publish_availability(&self, online: bool) -> MqttPublishFuture<'_> {
            Box::pin(async move {
                self.push(FakePublish::Availability(online))?;
                Ok(())
            })
        }

        fn publish_motion(&self, motion_detected: bool) -> MqttPublishFuture<'_> {
            Box::pin(async move {
                self.push(FakePublish::Motion(motion_detected))?;
                if self.fail_motion {
                    return Err(MqttError::InvalidConfig(
                        "forced publish failure".to_string(),
                    ));
                }
                Ok(())
            })
        }

        fn disconnect(&self) -> MqttPublishFuture<'_> {
            Box::pin(async move {
                self.push(FakePublish::Disconnect)?;
                Ok(())
            })
        }
    }

    fn test_config() -> Config {
        let mut config = Config::default();
        config.device_name = "officescreen".to_string();
        config.display_name = Some("Office Screen".to_string());
        config
    }

    #[tokio::test]
    async fn startup_publish_uses_current_state() {
        let mut app = App::new(test_config());
        let fake = FakeMqttPublisher::new(false);
        app.mqtt_client = Some(Box::new(fake.clone()));

        app.initialize_motion_state(false).await;
        app.publish_current_motion_state("startup")
            .await
            .expect("startup state publish should succeed");

        assert_eq!(fake.records(), vec![FakePublish::Motion(false)]);
        assert!(!app.motion_active());
        assert!(app.last_state_publish().is_some());
    }

    #[tokio::test]
    async fn reconnect_republishes_lifecycle_and_current_state() {
        let mut app = App::new(test_config());
        let fake = FakeMqttPublisher::new(false);
        app.mqtt_client = Some(Box::new(fake.clone()));

        app.initialize_motion_state(true).await;
        app.handle_mqtt_event(MqttEvent::Connected)
            .await
            .expect("connect handling should succeed");

        assert_eq!(
            fake.records(),
            vec![
                FakePublish::Discovery("mrpir-officescreen".to_string()),
                FakePublish::Availability(true),
                FakePublish::Motion(true),
            ]
        );
        assert!(app.last_lifecycle_publish().is_some());
        assert!(app.last_state_publish().is_some());
    }

    #[tokio::test]
    async fn detected_and_cleared_events_publish_state_even_as_edges() {
        let mut app = App::new(test_config());
        let fake = FakeMqttPublisher::new(false);
        app.mqtt_client = Some(Box::new(fake.clone()));

        app.handle_motion(MotionEvent::Detected)
            .await
            .expect("detected event should succeed");
        app.handle_motion(MotionEvent::Cleared)
            .await
            .expect("cleared event should succeed");

        assert_eq!(
            fake.records(),
            vec![FakePublish::Motion(true), FakePublish::Motion(false)]
        );
        assert!(!app.motion_active());
    }

    #[tokio::test]
    async fn publish_failure_is_logged_and_does_not_stop_motion_handling() {
        let mut app = App::new(test_config());
        let fake = FakeMqttPublisher::new(true);
        app.mqtt_client = Some(Box::new(fake.clone()));

        app.handle_motion(MotionEvent::Detected)
            .await
            .expect("motion handling should continue after publish failure");

        assert_eq!(fake.records(), vec![FakePublish::Motion(true)]);
        assert!(app.motion_active());
        assert!(app.last_state_publish().is_none());
    }
}
