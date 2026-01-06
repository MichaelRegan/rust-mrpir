//! Error types for mrpir.
//!
//! Uses `thiserror` for typed errors in library code.
//! Main uses `anyhow` for ergonomic error propagation.

use thiserror::Error;

/// Errors that can occur in the MQTT module.
#[derive(Error, Debug)]
pub enum MqttError {
    #[error("Failed to connect to MQTT broker: {0}")]
    ConnectionFailed(#[from] rumqttc::ConnectionError),

    #[error("Failed to publish message: {0}")]
    PublishFailed(#[from] rumqttc::ClientError),

    #[error("Invalid MQTT configuration: {0}")]
    InvalidConfig(String),
}

/// Errors that can occur in the sensor module.
#[derive(Error, Debug)]
pub enum SensorError {
    #[error("GPIO initialization failed: {0}")]
    GpioInit(#[from] rppal::gpio::Error),

    #[error("Invalid GPIO pin: {0}")]
    InvalidPin(u8),

    #[error("Sensor read failed: {0}")]
    ReadFailed(String),
}

/// Errors that can occur in the screen control module.
#[derive(Error, Debug)]
pub enum ScreenError {
    #[error("Screen control not available: {0}")]
    NotAvailable(String),

    #[error("Failed to set brightness: {0}")]
    BrightnessFailed(String),

    #[error("Wayland connection failed: {0}")]
    WaylandFailed(String),

    #[error("Screen control operation failed: {0}")]
    OperationFailed(String),
}

/// Errors that can occur in configuration loading.
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Configuration loading failed: {0}")]
    LoadFailed(#[from] figment::Error),

    #[error("Invalid configuration value: {field} - {message}")]
    InvalidValue { field: String, message: String },

    #[error("Missing required configuration: {0}")]
    MissingRequired(String),
}
