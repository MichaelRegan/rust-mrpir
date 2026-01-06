//! PIR sensor module using rppal GPIO.

use rppal::gpio::{Gpio, InputPin, Level};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::config::SensorConfig;
use crate::error::SensorError;

/// Motion events from the PIR sensor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MotionEvent {
    /// Motion was detected
    Detected,
    /// Motion stopped (after no_motion_delay)
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
        info!("Initializing PIR sensor on GPIO pin {}", config.gpio_pin);

        let gpio = Gpio::new()?;
        let pin = gpio
            .get(config.gpio_pin)
            .map_err(|e| {
                error!("Failed to get GPIO pin {}: {}", config.gpio_pin, e);
                SensorError::GpioInit(e)
            })?
            .into_input_pulldown();

        info!("PIR sensor initialized successfully");

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
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) {
        let poll_interval = Duration::from_millis(self.config.poll_interval_ms);
        let no_motion_delay = Duration::from_secs(self.config.no_motion_delay_secs);

        let mut last_state = false;
        let mut motion_active = false;
        let mut last_motion_time = std::time::Instant::now();

        info!(
            "Starting PIR sensor polling (interval: {:?}, no_motion_delay: {:?})",
            poll_interval, no_motion_delay
        );

        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        info!("PIR sensor shutting down");
                        break;
                    }
                }
                _ = tokio::time::sleep(poll_interval) => {
                    let current_state = self.read();

                    // Rising edge: motion just detected
                    if current_state && !last_state {
                        debug!("PIR sensor: rising edge detected");
                        last_motion_time = std::time::Instant::now();

                        if !motion_active {
                            motion_active = true;
                            if let Err(e) = tx.send(MotionEvent::Detected).await {
                                warn!("Failed to send motion event: {}", e);
                            }
                        }
                    }

                    // Update motion time while motion is active
                    if current_state {
                        last_motion_time = std::time::Instant::now();
                    }

                    // Check for motion timeout
                    if motion_active && !current_state {
                        let elapsed = last_motion_time.elapsed();
                        if elapsed >= no_motion_delay {
                            debug!("PIR sensor: motion cleared after {:?}", elapsed);
                            motion_active = false;
                            if let Err(e) = tx.send(MotionEvent::Cleared).await {
                                warn!("Failed to send motion cleared event: {}", e);
                            }
                        }
                    }

                    last_state = current_state;
                }
            }
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
