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

use anyhow::{anyhow, Context, Result};
use std::future;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use app::App;
use config::Config;
use mqtt::MqttClient;
use sensor::PirSensor;

const STATE_REFRESH_INTERVAL: Duration = Duration::from_secs(120);
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(10);
const HEALTH_STARTUP_GRACE: Duration = Duration::from_secs(30);

#[derive(Debug)]
struct MqttRuntimeState {
    connected: bool,
    connected_since: Option<Instant>,
    disconnected_since: Option<Instant>,
}

impl MqttRuntimeState {
    fn new(enabled: bool) -> Self {
        Self {
            connected: false,
            connected_since: None,
            disconnected_since: enabled.then_some(Instant::now()),
        }
    }

    fn mark_connected(&mut self) {
        self.connected = true;
        self.connected_since = Some(Instant::now());
        self.disconnected_since = None;
    }

    fn mark_disconnected(&mut self) {
        self.connected = false;
        self.connected_since = None;
        self.disconnected_since = Some(Instant::now());
    }
}

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

    // Set up PIR sensor
    let (motion_tx, mut motion_rx) = mpsc::channel(10);
    let (sensor_tick_tx, sensor_tick_rx) = watch::channel(Instant::now());
    let sensor_shutdown = shutdown_token.clone();

    let sensor = match PirSensor::new(&config.sensor) {
        Ok(sensor) => sensor,
        Err(e) => {
            error!(error = %e, "Failed to initialize PIR sensor");
            error!("This program requires access to Raspberry Pi GPIO.");
            error!("Make sure you're running on a Raspberry Pi with proper permissions.");
            return Err(e.into());
        }
    };

    let initial_motion_state = sensor.read();
    info!(
        pin = config.sensor.gpio_pin,
        gpio_level = if initial_motion_state { "high" } else { "low" },
        motion_state = if initial_motion_state { "ON" } else { "OFF" },
        "Initial PIR GPIO level"
    );
    app.initialize_motion_state(initial_motion_state).await;

    tokio::spawn(async move {
        sensor
            .run(
                motion_tx,
                sensor_shutdown,
                sensor_tick_tx,
                initial_motion_state,
            )
            .await;
    });

    // Set up MQTT if enabled
    let mut mqtt_rx = None;
    let mut mqtt_progress_rx = None;
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
            Ok((client, rx, progress_rx)) => {
                app.mqtt_client = Some(Box::new(client));
                mqtt_rx = Some(rx);
                mqtt_progress_rx = Some(progress_rx);
                info!("MQTT client initialized");

                if let Err(e) = app.publish_current_motion_state("startup").await {
                    warn!(error = %e, "Failed to queue startup MQTT state publish");
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to create MQTT client");
                return Err(e.into());
            }
        }
    } else {
        info!("MQTT disabled by configuration");
    }

    // Notify systemd we're ready
    let _ = sd_notify::notify(true, &[sd_notify::NotifyState::Ready]);

    // Get watchdog interval if running under systemd
    // Check WATCHDOG_USEC env var directly as fallback for user services
    let watchdog_interval = {
        let mut usec = 0u64;
        if sd_notify::watchdog_enabled(false, &mut usec) && usec > 0 {
            info!(interval_ms = usec / 2000, "Systemd watchdog enabled");
            Some(std::time::Duration::from_micros(usec) / 2)
        } else if let Ok(usec_str) = std::env::var("WATCHDOG_USEC") {
            if let Ok(usec) = usec_str.parse::<u64>() {
                info!(
                    interval_ms = usec / 2000,
                    "Systemd watchdog enabled (from env)"
                );
                Some(std::time::Duration::from_micros(usec) / 2)
            } else {
                None
            }
        } else {
            info!("Systemd watchdog not enabled");
            None
        }
    };

    let mut mqtt_runtime = MqttRuntimeState::new(config.mqtt.enabled);
    let mut state_refresh = tokio::time::interval_at(
        tokio::time::Instant::now() + STATE_REFRESH_INTERVAL,
        STATE_REFRESH_INTERVAL,
    );
    state_refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut health_check = tokio::time::interval_at(
        tokio::time::Instant::now() + HEALTH_CHECK_INTERVAL,
        HEALTH_CHECK_INTERVAL,
    );
    health_check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut watchdog_tick = watchdog_interval.map(|interval| {
        let mut ticker = tokio::time::interval_at(tokio::time::Instant::now() + interval, interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        ticker
    });

    info!("Entering main loop");

    // Main event loop
    loop {
        tokio::select! {
            // Check for shutdown
            () = shutdown_token.cancelled() => {
                info!("Shutdown signal received");
                break;
            }

            // Handle motion events
            motion_event = motion_rx.recv() => {
                match motion_event {
                    Some(event) => {
                        if let Err(e) = app.handle_motion(event).await {
                            error!(error = %e, "Error handling motion event");
                        }
                    }
                    None => {
                        let error = anyhow!("PIR sensor event channel closed unexpectedly");
                        error!(error = %error, "Health failure; terminating for systemd restart");
                        shutdown_token.cancel();
                        app.shutdown().await;
                        let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Stopping]);
                        return Err(error);
                    }
                }
            }

            // Handle MQTT events
            event = async {
                match &mut mqtt_rx {
                    Some(rx) => rx.recv().await,
                    None => future::pending().await,
                }
            } => {
                match event {
                    Some(event) => {
                        match &event {
                            mqtt::MqttEvent::Connected => mqtt_runtime.mark_connected(),
                            mqtt::MqttEvent::Disconnected => mqtt_runtime.mark_disconnected(),
                            mqtt::MqttEvent::Error(_) => {}
                        }

                        if let Err(e) = app.handle_mqtt_event(event).await {
                            error!(error = %e, "Error handling MQTT event");
                        }
                    }
                    None => {
                        let error = anyhow!("MQTT event channel closed unexpectedly");
                        error!(error = %error, "Health failure; terminating for systemd restart");
                        shutdown_token.cancel();
                        app.shutdown().await;
                        let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Stopping]);
                        return Err(error);
                    }
                }
            }

            // Periodic retained state heartbeat for Home Assistant and broker restarts
            _ = state_refresh.tick(), if config.mqtt.enabled => {
                if let Err(e) = app.publish_current_motion_state("periodic_refresh").await {
                    warn!(error = %e, "Failed to queue periodic MQTT state refresh");
                }
            }

            // Internal health monitor
            _ = health_check.tick() => {
                if let Err(e) = check_runtime_health(
                    &config,
                    &app,
                    &sensor_tick_rx,
                    mqtt_progress_rx.as_ref(),
                    &mqtt_runtime,
                ) {
                    error!(error = %e, "Health failure; terminating for systemd restart");
                    shutdown_token.cancel();
                    app.shutdown().await;
                    let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Stopping]);
                    return Err(e);
                }
            }

            // Systemd watchdog ping
            () = async {
                if let Some(interval) = watchdog_tick.as_mut() {
                    interval.tick().await;
                } else {
                    future::pending::<()>().await;
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

fn check_runtime_health(
    config: &Config,
    app: &App,
    sensor_tick_rx: &watch::Receiver<Instant>,
    mqtt_progress_rx: Option<&watch::Receiver<Instant>>,
    mqtt_runtime: &MqttRuntimeState,
) -> Result<()> {
    let now = Instant::now();
    let last_pir_tick = *sensor_tick_rx.borrow();
    let pir_elapsed = now.duration_since(last_pir_tick);
    if pir_elapsed > pir_stall_timeout(config) {
        return Err(anyhow!(
            "PIR polling loop stalled: no tick for {:?}",
            pir_elapsed
        ));
    }

    if config.mqtt.enabled {
        let Some(progress_rx) = mqtt_progress_rx else {
            return Err(anyhow!(
                "MQTT enabled but event loop progress monitor is unavailable"
            ));
        };

        let mqtt_progress_elapsed = now.duration_since(*progress_rx.borrow());
        if mqtt_progress_elapsed > mqtt_event_loop_stall_timeout(config) {
            return Err(anyhow!(
                "MQTT event loop stalled: no progress for {:?}",
                mqtt_progress_elapsed
            ));
        }

        if mqtt_runtime.connected {
            let connected_for = mqtt_runtime
                .connected_since
                .map_or_else(|| Duration::from_secs(0), |since| now.duration_since(since));

            if app.last_lifecycle_publish().is_none() && connected_for > HEALTH_STARTUP_GRACE {
                return Err(anyhow!(
                    "MQTT connected for {:?} without successful discovery or availability publish",
                    connected_for
                ));
            }

            if app.last_state_publish().is_none() && connected_for > HEALTH_STARTUP_GRACE {
                return Err(anyhow!(
                    "MQTT connected for {:?} without successful state publish",
                    connected_for
                ));
            }

            if let Some(last_state_publish) = app.last_state_publish() {
                let state_publish_elapsed = now.duration_since(last_state_publish);
                if state_publish_elapsed > state_publish_stall_timeout() {
                    return Err(anyhow!(
                        "MQTT state publish stale: last successful state publish was {:?} ago",
                        state_publish_elapsed
                    ));
                }
            }
        } else if let Some(disconnected_since) = mqtt_runtime.disconnected_since {
            let disconnected_for = now.duration_since(disconnected_since);
            if disconnected_for > mqtt_disconnect_timeout(config) {
                return Err(anyhow!(
                    "MQTT disconnected for {:?}; terminating so systemd can restart the service",
                    disconnected_for
                ));
            }
        }
    }

    Ok(())
}

fn pir_stall_timeout(config: &Config) -> Duration {
    let timeout = Duration::from_millis(config.sensor.poll_interval_ms.saturating_mul(20));
    if timeout < Duration::from_secs(30) {
        Duration::from_secs(30)
    } else {
        timeout
    }
}

fn mqtt_event_loop_stall_timeout(config: &Config) -> Duration {
    Duration::from_secs(config.mqtt.keep_alive_secs.saturating_mul(2).max(90))
}

fn mqtt_disconnect_timeout(config: &Config) -> Duration {
    Duration::from_secs(config.mqtt.keep_alive_secs.saturating_mul(3).max(120))
}

fn state_publish_stall_timeout() -> Duration {
    Duration::from_secs(
        STATE_REFRESH_INTERVAL
            .as_secs()
            .saturating_mul(3)
            .saturating_add(30),
    )
}
