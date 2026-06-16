//! Error types for mrpir.
//!
//! Uses `thiserror` for typed errors in library code.
//! Main uses `anyhow` for ergonomic error propagation.

use thiserror::Error;

/// Errors that can occur in the MQTT module.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum MqttError {
    #[error("failed to connect to MQTT broker: {0}")]
    ConnectionFailed(#[from] rumqttc::ConnectionError),

    #[error("failed to publish message: {0}")]
    PublishFailed(#[from] rumqttc::ClientError),

    #[error("timed out queueing MQTT publish to {topic} after {timeout_secs}s")]
    PublishTimedOut { topic: String, timeout_secs: u64 },

    #[error("invalid MQTT configuration: {0}")]
    InvalidConfig(String),
}

/// Errors that can occur in the sensor module.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum SensorError {
    #[error("GPIO initialization failed: {0}")]
    GpioInit(#[from] rppal::gpio::Error),

    #[error("invalid GPIO pin: {0}")]
    InvalidPin(u8),

    #[error("sensor read failed: {0}")]
    ReadFailed(String),
}

/// Errors that can occur in the screen control module.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum ScreenError {
    #[error("screen control not available: {0}")]
    NotAvailable(String),

    #[error("failed to set brightness: {0}")]
    BrightnessFailed(String),

    #[error("Wayland connection failed: {0}")]
    WaylandFailed(String),

    #[error("screen control operation failed: {0}")]
    OperationFailed(String),
}

/// Errors that can occur in configuration loading.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum ConfigError {
    #[error("configuration loading failed: {0}")]
    LoadFailed(#[from] figment::Error),

    #[error("invalid configuration value: {field} - {message}")]
    InvalidValue { field: String, message: String },

    #[error("missing required configuration: {0}")]
    MissingRequired(String),
}
