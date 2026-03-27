//! mrpir - Motion-activated screen control for Raspberry Pi
//!
//! A Rust daemon that monitors a PIR motion sensor and:
//! - Publishes motion events to MQTT with Home Assistant discovery
//! - Controls screen brightness based on motion
//! - Supports night mode with sunrise/sunset awareness
//!
//! ## Configuration
//!
//! Configuration is loaded from (in order of priority):
//! 1. Environment variables (`MRPIR_*`)
//! 2. Local `config.toml`
//! 3. User config `~/.config/mrpir/config.toml`
//! 4. System config `/etc/mrpir/config.toml`
//!
//! See the README for full configuration options.

mod config;
mod error;
mod mqtt;
mod screen;
mod sensor;
mod time_events;

use anyhow::{Context, Result};
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use config::Config;
use mqtt::{MqttClient, MqttEvent};
use screen::ScreenManager;
use sensor::{MotionEvent, PirSensor};
use time_events::NightModeManager;

/// Application state.
struct App {
    config: Config,
    mqtt_client: Option<MqttClient>,
    screen_manager: Option<ScreenManager>,
    night_mode: NightModeManager,
    motion_active: bool,
    last_motion: std::time::Instant,
}

impl App {
    /// Create a new application instance.
    fn new(config: Config) -> Result<Self> {
        let night_mode = NightModeManager::new(
            &config.night_mode,
            if config.location.latitude.is_some() {
                Some(&config.location)
            } else {
                None
            },
        );

        let screen_manager = if config.screen.enabled {
            match ScreenManager::new(&config.screen) {
                Ok(manager) => Some(manager),
                Err(e) => {
                    warn!("Screen control not available: {}", e);
                    None
                }
            }
        } else {
            None
        };

        Ok(Self {
            config,
            mqtt_client: None,
            screen_manager,
            night_mode,
            motion_active: false,
            last_motion: std::time::Instant::now(),
        })
    }

    /// Handle a motion event.
    async fn handle_motion(&mut self, event: MotionEvent) -> Result<()> {
        match event {
            MotionEvent::Detected => {
                if !self.motion_active {
                    self.motion_active = true;
                    self.last_motion = std::time::Instant::now();

                    info!("Motion detected");

                    // Publish to MQTT
                    if let Some(ref client) = self.mqtt_client {
                        if let Err(e) = client.publish_motion(true).await {
                            warn!("Failed to publish motion: {}", e);
                        }
                    }

                    // Wake screen
                    if let Some(ref mut manager) = self.screen_manager {
                        if let Err(e) = manager.on_motion().await {
                            warn!("Failed to wake screen: {}", e);
                        }
                    }
                }
                // Always update last motion time
                self.last_motion = std::time::Instant::now();
            }
            MotionEvent::Cleared => {
                if self.motion_active {
                    self.motion_active = false;

                    info!("Motion cleared");

                    // Publish to MQTT
                    if let Some(ref client) = self.mqtt_client {
                        if let Err(e) = client.publish_motion(false).await {
                            warn!("Failed to publish motion cleared: {}", e);
                        }
                    }

                    // Check if we should dim or turn off
                    if self.night_mode.is_night_mode() {
                        if let Some(ref mut manager) = self.screen_manager {
                            if let Err(e) = manager.on_night_mode().await {
                                warn!("Failed to apply night mode: {}", e);
                            }
                        }
                    } else if let Some(ref mut manager) = self.screen_manager {
                        if let Err(e) = manager.on_motion_timeout().await {
                            warn!("Failed to dim screen: {}", e);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Handle MQTT connection events.
    async fn handle_mqtt_event(&mut self, event: MqttEvent) -> Result<()> {
        match event {
            MqttEvent::Connected => {
                info!("MQTT connected, publishing discovery");
                if let Some(ref client) = self.mqtt_client {
                    let client_id = self.config.client_id();
                    if let Err(e) = client.publish_discovery(&client_id).await {
                        error!("Failed to publish discovery: {}", e);
                    }
                    if let Err(e) = client.publish_availability(true).await {
                        error!("Failed to publish availability: {}", e);
                    }
                }
            }
            MqttEvent::Disconnected => {
                warn!("MQTT disconnected, will reconnect automatically");
            }
            MqttEvent::Error(msg) => {
                error!("MQTT error: {}", msg);
            }
        }
        Ok(())
    }
}

/// Initialize logging with tracing.
fn init_logging(config: &Config) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| {
            tracing_subscriber::EnvFilter::new(format!("mrpir={}", config.logging.level))
        });

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}

/// Main entry point.
#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration
    let config = Config::load().context("Failed to load configuration")?;

    // Initialize logging
    init_logging(&config);

    info!("mrpir v{} starting up", env!("CARGO_PKG_VERSION"));
    info!("Device name: {}", config.device_name);
    info!("PIR sensor on GPIO pin {}", config.sensor.gpio_pin);

    // Create application
    let mut app = App::new(config.clone()).context("Failed to initialize application")?;

    // Set up shutdown signal handling
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let shutdown_tx_clone = shutdown_tx.clone();

    tokio::spawn(async move {
        #[cfg(unix)]
        {
            let ctrl_c = tokio::signal::ctrl_c();
            let mut sigterm =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("Failed to install SIGTERM handler");

            tokio::select! {
                _ = ctrl_c => {
                    info!("Received SIGINT, shutting down...");
                }
                _ = sigterm.recv() => {
                    info!("Received SIGTERM, shutting down...");
                }
            }
        }

        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
            info!("Received SIGINT, shutting down...");
        }

        let _ = shutdown_tx_clone.send(true);
    });

    // Notify systemd we're ready
    let _ = sd_notify::notify(&[sd_notify::NotifyState::Ready]);

    // Set up MQTT if enabled
    let mut mqtt_rx = None;
    if config.mqtt.enabled {
        info!(
            "Connecting to MQTT broker at {}:{}",
            config.mqtt.host, config.mqtt.port
        );

        match MqttClient::new(
            &config.mqtt,
            &config.device_name,
            config.display_name(),
            &config.client_id(),
        )
        .await
        {
            Ok((client, rx)) => {
                app.mqtt_client = Some(client);
                mqtt_rx = Some(rx);
                info!("MQTT client initialized");
            }
            Err(e) => {
                error!("Failed to create MQTT client: {}", e);
                if config.mqtt.enabled {
                    warn!("Continuing without MQTT");
                }
            }
        }
    }

    // Set up PIR sensor
    let (motion_tx, mut motion_rx) = mpsc::channel(10);
    let sensor_shutdown_rx = shutdown_rx.clone();

    let sensor_result = PirSensor::new(&config.sensor);

    match sensor_result {
        Ok(sensor) => {
            info!("PIR sensor initialized");
            tokio::spawn(async move {
                sensor.run(motion_tx, sensor_shutdown_rx).await;
            });
        }
        Err(e) => {
            error!("Failed to initialize PIR sensor: {}", e);
            error!("This program requires access to Raspberry Pi GPIO.");
            error!("Make sure you're running on a Raspberry Pi with proper permissions.");
            return Err(e.into());
        }
    }

    // Get watchdog interval if running under systemd
    let watchdog_interval = sd_notify::watchdog_enabled().map(|d| d / 2);

    info!("Entering main loop");

    // Main event loop
    loop {
        tokio::select! {
            // Check for shutdown
            _ = async {
                let mut rx = shutdown_rx.clone();
                rx.changed().await.ok();
                if *rx.borrow() {
                    Some(())
                } else {
                    None
                }
            } => {
                info!("Shutdown signal received");
                break;
            }

            // Handle motion events
            Some(event) = motion_rx.recv() => {
                if let Err(e) = app.handle_motion(event).await {
                    error!("Error handling motion event: {}", e);
                }
            }

            // Handle MQTT events
            Some(event) = async {
                match &mut mqtt_rx {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                if let Err(e) = app.handle_mqtt_event(event).await {
                    error!("Error handling MQTT event: {}", e);
                }
            }

            // Systemd watchdog ping
            _ = async {
                if let Some(interval) = watchdog_interval {
                    tokio::time::sleep(interval).await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {
                let _ = sd_notify::notify(&[sd_notify::NotifyState::Watchdog]);
            }
        }
    }

    // Graceful shutdown
    info!("Shutting down...");

    // Signal shutdown
    let _ = shutdown_tx.send(true);

    // Disconnect MQTT gracefully
    if let Some(ref client) = app.mqtt_client {
        if let Err(e) = client.disconnect().await {
            warn!("Error disconnecting from MQTT: {}", e);
        }
    }

    // Notify systemd we're stopping
    let _ = sd_notify::notify(&[sd_notify::NotifyState::Stopping]);

    info!("Goodbye!");
    Ok(())
}
