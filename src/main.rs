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

mod app;
mod config;
mod error;
mod mqtt;
mod screen;
mod sensor;
mod time_events;

use anyhow::{Context, Result};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use app::App;
use config::Config;
use mqtt::MqttClient;
use sensor::PirSensor;

/// Initialize logging with tracing.
fn init_logging(config: &Config) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
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

    info!(version = %env!("CARGO_PKG_VERSION"), "mrpir starting up");
    info!(
        device = %config.device_name,
        pin = config.sensor.gpio_pin,
        mqtt_enabled = config.mqtt.enabled,
        screen_enabled = config.screen.enabled,
        "Configuration loaded"
    );

    // Create application
    let mut app = App::new(config.clone());

    // Set up shutdown signal handling with CancellationToken
    let shutdown_token = CancellationToken::new();
    let shutdown_token_signal = shutdown_token.clone();

    tokio::spawn(async move {
        let ctrl_c = tokio::signal::ctrl_c();

        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigterm =
                signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");

            tokio::select! {
                result = ctrl_c => {
                    if let Err(e) = result {
                        error!(error = %e, "Failed to listen for SIGINT");
                    }
                    info!(signal = "SIGINT", "Shutdown signal received");
                }
                _ = sigterm.recv() => {
                    info!(signal = "SIGTERM", "Shutdown signal received");
                }
            }
        }

        #[cfg(not(unix))]
        {
            if let Err(e) = ctrl_c.await {
                error!(error = %e, "Failed to listen for Ctrl+C");
            }
            info!(signal = "Ctrl+C", "Shutdown signal received");
        }

        shutdown_token_signal.cancel();
    });

    // Notify systemd we're ready
    let _ = sd_notify::notify(true, &[sd_notify::NotifyState::Ready]);

    // Set up MQTT if enabled
    let mut mqtt_rx = None;
    if config.mqtt.enabled {
        info!(
            host = %config.mqtt.host,
            port = config.mqtt.port,
            "Connecting to MQTT broker"
        );

        match MqttClient::new(
            &config.mqtt,
            &config.device_name,
            config.display_name(),
            &config.client_id(),
        ) {
            Ok((client, rx)) => {
                app.mqtt_client = Some(client);
                mqtt_rx = Some(rx);
                info!("MQTT client initialized");
            }
            Err(e) => {
                error!("Failed to create MQTT client: {}", e);
                warn!("Continuing without MQTT");
            }
        }
    }

    // Set up PIR sensor
    let (motion_tx, mut motion_rx) = mpsc::channel(10);
    let sensor_shutdown = shutdown_token.clone();

    let sensor_result = PirSensor::new(&config.sensor);

    match sensor_result {
        Ok(sensor) => {
            info!(pin = config.sensor.gpio_pin, "PIR sensor initialized");
            tokio::spawn(async move {
                sensor.run(motion_tx, sensor_shutdown).await;
            });
        }
        Err(e) => {
            error!(error = %e, "Failed to initialize PIR sensor");
            error!("This program requires access to Raspberry Pi GPIO.");
            error!("Make sure you're running on a Raspberry Pi with proper permissions.");
            return Err(e.into());
        }
    }

    // Get watchdog interval if running under systemd
    // Check WATCHDOG_USEC env var directly as fallback for user services
    let watchdog_interval = {
        let mut usec = 0u64;
        if sd_notify::watchdog_enabled(false, &mut usec) && usec > 0 {
            info!(interval_ms = usec / 2000, "Systemd watchdog enabled");
            Some(std::time::Duration::from_micros(usec) / 2)
        } else if let Ok(usec_str) = std::env::var("WATCHDOG_USEC") {
            if let Ok(usec) = usec_str.parse::<u64>() {
                info!(interval_ms = usec / 2000, "Systemd watchdog enabled (from env)");
                Some(std::time::Duration::from_micros(usec) / 2)
            } else {
                None
            }
        } else {
            info!("Systemd watchdog not enabled");
            None
        }
    };

    info!("Entering main loop");

    // Main event loop
    loop {
        tokio::select! {
            // Check for shutdown
            _ = shutdown_token.cancelled() => {
                info!("Shutdown signal received");
                break;
            }

            // Handle motion events
            Some(event) = motion_rx.recv() => {
                if let Err(e) = app.handle_motion(event).await {
                    error!(error = %e, "Error handling motion event");
                }
            }

            // Handle MQTT events
            event = async {
                match &mut mqtt_rx {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                if let Some(event) = event {
                    if let Err(e) = app.handle_mqtt_event(event).await {
                        error!(error = %e, "Error handling MQTT event");
                    }
                }
            }

            // Systemd watchdog ping
            () = async {
                match watchdog_interval {
                    Some(interval) => tokio::time::sleep(interval).await,
                    None => std::future::pending().await,
                }
            } => {
                let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Watchdog]);
            }
        }
    }

    // Graceful shutdown
    info!("Shutting down...");

    // Cancel any remaining tasks
    shutdown_token.cancel();

    // Disconnect MQTT gracefully
    app.shutdown().await;

    // Notify systemd we're stopping
    let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Stopping]);

    info!("Goodbye!");
    Ok(())
}
