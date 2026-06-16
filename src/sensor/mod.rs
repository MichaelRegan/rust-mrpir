//! PIR sensor module using rppal GPIO.

use rppal::gpio::{Gpio, InputPin, Level};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::config::SensorConfig;
use crate::error::SensorError;

/// Motion events from the PIR sensor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionEvent {
    /// Motion was detected
    Detected,
    /// Motion stopped (after `no_motion_delay`)
    Cleared,
}

/// PIR motion sensor using GPIO.
pub struct PirSensor {
    pin: InputPin,
    config: SensorConfig,
}

impl PirSensor {
    /// Create a new PIR sensor on the specified GPIO pin.
    pub fn new(config: &SensorConfig) -> Result<Self, SensorError> {
        info!(pin = config.gpio_pin, "Initializing PIR sensor");

        let gpio = Gpio::new()?;
        let pin = gpio
            .get(config.gpio_pin)
            .map_err(|e| {
                error!(pin = config.gpio_pin, error = %e, "Failed to get GPIO pin");
                SensorError::GpioInit(e)
            })?
            .into_input_pulldown();

        info!(pin = config.gpio_pin, "PIR sensor initialized successfully");

        Ok(Self {
            pin,
            config: config.clone(),
        })
    }

    /// Read the current sensor state.
    pub fn read(&self) -> bool {
        self.pin.read() == Level::High
    }

    /// Run the sensor polling loop, sending events to the provided channel.
    ///
    /// This method runs indefinitely until cancelled.
    pub async fn run(
        &self,
        tx: mpsc::Sender<MotionEvent>,
        shutdown: CancellationToken,
        health_tx: watch::Sender<Instant>,
        initial_state: bool,
    ) {
        let poll_interval = Duration::from_millis(self.config.poll_interval_ms);
        let no_motion_delay = Duration::from_secs(self.config.no_motion_delay_secs);

        let mut last_state = initial_state;
        let mut motion_active = initial_state;
        let mut low_since: Option<Instant> = None;

        info!(
            poll_interval_ms = self.config.poll_interval_ms,
            no_motion_delay_secs = self.config.no_motion_delay_secs,
            initial_gpio_active = initial_state,
            "Starting PIR sensor polling"
        );

        loop {
            tokio::select! {
                () = shutdown.cancelled() => {
                    info!("PIR sensor shutting down");
                    break;
                }
                () = tokio::time::sleep(poll_interval) => {
                    let now = Instant::now();
                    let current_state = self.read();
                    if health_tx.send(now).is_err() {
                        debug!("PIR health receiver dropped");
                    }

                    if current_state {
                        if !last_state {
                            debug!("PIR sensor: rising edge detected");
                        }

                        if low_since.take().is_some() {
                            info!("No-motion timer cancelled");
                        }

                        if !motion_active {
                            motion_active = true;
                            info!("PIR motion detected");
                            if !send_motion_event(&tx, MotionEvent::Detected).await {
                                break;
                            }
                        }
                    } else if motion_active {
                        if last_state && low_since.is_none() {
                            low_since = Some(now);
                            info!(
                                no_motion_delay_secs = self.config.no_motion_delay_secs,
                                "No-motion timer started"
                            );
                        }

                        if let Some(started_at) = low_since {
                            let elapsed = now.duration_since(started_at);
                            if elapsed >= no_motion_delay {
                                let elapsed_ms = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
                                info!(elapsed_ms, "No-motion timer fired");
                                motion_active = false;
                                low_since = None;
                                if !send_motion_event(&tx, MotionEvent::Cleared).await {
                                    break;
                                }
                            }
                        }
                    }

                    last_state = current_state;
                }
            }
        }
    }
}

async fn send_motion_event(tx: &mpsc::Sender<MotionEvent>, event: MotionEvent) -> bool {
    match tx.send(event).await {
        Ok(()) => true,
        Err(e) => {
            warn!(event = ?e.0, error = %e, "Failed to send motion event");
            false
        }
    }
}

/// Mock PIR sensor for testing without hardware.
#[cfg(test)]
pub struct MockPirSensor {
    state: std::sync::atomic::AtomicBool,
}

#[cfg(test)]
impl MockPirSensor {
    pub fn new() -> Self {
        Self {
            state: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn set_motion(&self, detected: bool) {
        self.state
            .store(detected, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn read(&self) -> bool {
        self.state.load(std::sync::atomic::Ordering::SeqCst)
    }
}
