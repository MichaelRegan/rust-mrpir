//! Application state and event handling.

use anyhow::Result;
use tracing::{error, info, warn};

use crate::config::Config;
use crate::mqtt::{MqttClient, MqttEvent};
use crate::screen::ScreenManager;
use crate::sensor::MotionEvent;
use crate::time_events::NightModeManager;

/// Application state.
pub struct App {
    config: Config,
    pub mqtt_client: Option<MqttClient>,
    screen_manager: Option<ScreenManager>,
    night_mode: NightModeManager,
    motion_active: bool,
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
        }
    }

    /// Handle a motion event.
    pub async fn handle_motion(&mut self, event: MotionEvent) -> Result<()> {
        match event {
            MotionEvent::Detected => {
                if !self.motion_active {
                    self.motion_active = true;

                    info!(state = "detected", "Motion event");

                    // Publish to MQTT
                    if let Some(ref client) = self.mqtt_client {
                        if let Err(e) = client.publish_motion(true).await {
                            warn!(error = %e, "Failed to publish motion");
                        }
                    }

                    // Wake screen
                    if let Some(ref mut manager) = self.screen_manager {
                        if let Err(e) = manager.on_motion().await {
                            warn!(error = %e, "Failed to wake screen");
                        }
                    }
                }
            }
            MotionEvent::Cleared => {
                if self.motion_active {
                    self.motion_active = false;

                    info!(state = "cleared", "Motion event");

                    // Publish to MQTT
                    if let Some(ref client) = self.mqtt_client {
                        if let Err(e) = client.publish_motion(false).await {
                            warn!(error = %e, "Failed to publish motion cleared");
                        }
                    }

                    // Check if we should dim or turn off
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
        }

        Ok(())
    }

    /// Handle MQTT connection events.
    pub async fn handle_mqtt_event(&self, event: MqttEvent) -> Result<()> {
        match event {
            MqttEvent::Connected => {
                info!("MQTT connected, publishing discovery");
                if let Some(ref client) = self.mqtt_client {
                    let client_id = self.config.client_id();
                    if let Err(e) = client.publish_discovery(&client_id).await {
                        error!(error = %e, "Failed to publish discovery");
                    }
                    if let Err(e) = client.publish_availability(true).await {
                        error!(error = %e, "Failed to publish availability");
                    }
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
}
