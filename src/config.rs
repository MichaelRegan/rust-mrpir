//! Configuration management for mrpir.
//!
//! Uses `figment` for layered configuration:
//! 1. Built-in defaults
//! 2. System config: /etc/mrpir/config.toml
//! 3. User config: ~/.config/mrpir/config.toml  
//! 4. Local config: ./config.toml
//! 5. Environment variables: MRPIR_*

use figment::{
    providers::{Env, Format, Serialized, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::ConfigError;

/// Main configuration structure.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    /// Device name for MQTT topics and Home Assistant
    #[serde(default = "default_device_name")]
    pub device_name: String,

    /// Display name shown in Home Assistant
    #[serde(default)]
    pub display_name: Option<String>,

    /// PIR sensor configuration
    #[serde(default)]
    pub sensor: SensorConfig,

    /// MQTT configuration
    #[serde(default)]
    pub mqtt: MqttConfig,

    /// Screen control configuration
    #[serde(default)]
    pub screen: ScreenConfig,

    /// Night mode configuration
    #[serde(default)]
    pub night_mode: NightModeConfig,

    /// Location for sunrise/sunset calculations
    #[serde(default)]
    pub location: LocationConfig,

    /// Logging configuration
    #[serde(default)]
    pub logging: LoggingConfig,
}

/// PIR sensor configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SensorConfig {
    /// GPIO pin number for the PIR sensor
    #[serde(default = "default_gpio_pin")]
    pub gpio_pin: u8,

    /// Delay in seconds before considering motion stopped
    #[serde(default = "default_no_motion_delay")]
    pub no_motion_delay_secs: u64,

    /// Polling interval in milliseconds
    #[serde(default = "default_poll_interval")]
    pub poll_interval_ms: u64,
}

/// MQTT broker configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MqttConfig {
    /// Enable MQTT functionality
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// MQTT broker hostname or IP
    #[serde(default = "default_mqtt_host")]
    pub host: String,

    /// MQTT broker port
    #[serde(default = "default_mqtt_port")]
    pub port: u16,

    /// MQTT username (optional)
    #[serde(default)]
    pub username: Option<String>,

    /// MQTT password (optional)
    #[serde(default)]
    pub password: Option<String>,

    /// Client ID for MQTT connection
    #[serde(default)]
    pub client_id: Option<String>,

    /// Enable Home Assistant MQTT discovery
    #[serde(default = "default_true")]
    pub ha_discovery: bool,

    /// Home Assistant discovery prefix
    #[serde(default = "default_ha_prefix")]
    pub ha_discovery_prefix: String,

    /// Keep-alive interval in seconds
    #[serde(default = "default_keep_alive")]
    pub keep_alive_secs: u64,
}

/// Screen control configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScreenConfig {
    /// Enable screen control
    #[serde(default)]
    pub enabled: bool,

    /// Screen control method
    #[serde(default)]
    pub method: ScreenMethod,

    /// Brightness when dimmed (0-255)
    #[serde(default)]
    pub dim_brightness: u8,

    /// Brightness when bright (0-255)
    #[serde(default = "default_bright_brightness")]
    pub bright_brightness: u8,

    /// Path to brightness sysfs file (for manual control)
    #[serde(default)]
    pub brightness_path: Option<PathBuf>,

    /// Transition time in seconds for brightness changes
    #[serde(default = "default_transition_time")]
    pub transition_time_secs: u64,

    /// Timeout before dimming after no motion (seconds)
    #[serde(default = "default_screen_timeout")]
    pub motion_timeout_secs: u64,
}

/// Screen control methods.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ScreenMethod {
    /// No screen control
    #[default]
    None,
    /// Use brightness crate (sysfs-based)
    Brightness,
    /// Use Wayland wlr-output-power-management
    Wayland,
    /// Use xscreensaver command (legacy)
    Xscreensaver,
}

/// Night mode configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NightModeConfig {
    /// Enable night mode
    #[serde(default)]
    pub enabled: bool,

    /// Use sunrise/sunset instead of fixed hours
    #[serde(default)]
    pub use_sun_times: bool,

    /// Hour to start night mode (24-hour format)
    #[serde(default = "default_night_start")]
    pub start_hour: u8,

    /// Hour to end night mode (24-hour format)
    #[serde(default = "default_night_end")]
    pub end_hour: u8,

    /// Delay after sunset before enabling night mode (seconds)
    #[serde(default = "default_sundown_timeout")]
    pub sundown_delay_secs: u64,

    /// Screen off delay during night mode (seconds)
    #[serde(default = "default_screen_off_delay")]
    pub screen_off_delay_secs: u64,
}

/// Location configuration for sunrise/sunset.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LocationConfig {
    /// Friendly location name
    #[serde(default)]
    pub name: Option<String>,

    /// Region name
    #[serde(default)]
    pub region: Option<String>,

    /// Latitude in decimal degrees
    #[serde(default)]
    pub latitude: Option<f64>,

    /// Longitude in decimal degrees
    #[serde(default)]
    pub longitude: Option<f64>,
}

/// Logging configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error)
    #[serde(default = "default_log_level")]
    pub level: String,

    /// Log to file
    #[serde(default)]
    pub file: Option<PathBuf>,
}

// Default value functions
fn default_device_name() -> String {
    hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "mrpir".to_string())
}

fn default_gpio_pin() -> u8 {
    17
}

fn default_no_motion_delay() -> u64 {
    5
}

fn default_poll_interval() -> u64 {
    100
}

fn default_true() -> bool {
    true
}

fn default_mqtt_host() -> String {
    "localhost".to_string()
}

fn default_mqtt_port() -> u16 {
    1883
}

fn default_ha_prefix() -> String {
    "homeassistant".to_string()
}

fn default_keep_alive() -> u64 {
    60
}

fn default_bright_brightness() -> u8 {
    230
}

fn default_transition_time() -> u64 {
    2
}

fn default_screen_timeout() -> u64 {
    30
}

fn default_night_start() -> u8 {
    22
}

fn default_night_end() -> u8 {
    6
}

fn default_sundown_timeout() -> u64 {
    3600
}

fn default_screen_off_delay() -> u64 {
    3600
}

fn default_log_level() -> String {
    "info".to_string()
}

// Default trait implementations
impl Default for Config {
    fn default() -> Self {
        Self {
            device_name: default_device_name(),
            display_name: None,
            sensor: SensorConfig::default(),
            mqtt: MqttConfig::default(),
            screen: ScreenConfig::default(),
            night_mode: NightModeConfig::default(),
            location: LocationConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

impl Default for SensorConfig {
    fn default() -> Self {
        Self {
            gpio_pin: default_gpio_pin(),
            no_motion_delay_secs: default_no_motion_delay(),
            poll_interval_ms: default_poll_interval(),
        }
    }
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            host: default_mqtt_host(),
            port: default_mqtt_port(),
            username: None,
            password: None,
            client_id: None,
            ha_discovery: true,
            ha_discovery_prefix: default_ha_prefix(),
            keep_alive_secs: default_keep_alive(),
        }
    }
}

impl Default for ScreenConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            method: ScreenMethod::None,
            dim_brightness: 0,
            bright_brightness: default_bright_brightness(),
            brightness_path: None,
            transition_time_secs: default_transition_time(),
            motion_timeout_secs: default_screen_timeout(),
        }
    }
}

impl Default for NightModeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            use_sun_times: false,
            start_hour: default_night_start(),
            end_hour: default_night_end(),
            sundown_delay_secs: default_sundown_timeout(),
            screen_off_delay_secs: default_screen_off_delay(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            file: None,
        }
    }
}

impl Config {
    /// Load configuration from all sources.
    ///
    /// Priority (highest to lowest):
    /// 1. Environment variables (MRPIR_*)
    /// 2. Local config.toml
    /// 3. User config ~/.config/mrpir/config.toml
    /// 4. System config /etc/mrpir/config.toml
    /// 5. Built-in defaults
    pub fn load() -> Result<Self, ConfigError> {
        let home_config = dirs::config_dir()
            .map(|p| p.join("mrpir/config.toml"))
            .unwrap_or_else(|| PathBuf::from("~/.config/mrpir/config.toml"));

        let config: Config = Figment::new()
            // Start with defaults
            .merge(Serialized::defaults(Config::default()))
            // System config
            .merge(Toml::file("/etc/mrpir/config.toml"))
            // User config
            .merge(Toml::file(&home_config))
            // Local config
            .merge(Toml::file("config.toml"))
            // Environment variables (MRPIR_MQTT__HOST, MRPIR_SENSOR__GPIO_PIN, etc.)
            .merge(Env::prefixed("MRPIR_").split("__"))
            .extract()?;

        config.validate()?;
        Ok(config)
    }

    /// Validate configuration values.
    fn validate(&self) -> Result<(), ConfigError> {
        // Validate GPIO pin range (Raspberry Pi has pins 0-27)
        if self.sensor.gpio_pin > 27 {
            return Err(ConfigError::InvalidValue {
                field: "sensor.gpio_pin".to_string(),
                message: format!("GPIO pin must be 0-27, got {}", self.sensor.gpio_pin),
            });
        }

        // Validate night mode hours
        if self.night_mode.start_hour > 23 || self.night_mode.end_hour > 23 {
            return Err(ConfigError::InvalidValue {
                field: "night_mode.start_hour/end_hour".to_string(),
                message: "Hours must be 0-23".to_string(),
            });
        }

        // Validate location if sun times are enabled
        if self.night_mode.use_sun_times {
            if self.location.latitude.is_none() || self.location.longitude.is_none() {
                return Err(ConfigError::MissingRequired(
                    "location.latitude and location.longitude required when use_sun_times is enabled".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Get the effective client ID.
    pub fn client_id(&self) -> String {
        self.mqtt
            .client_id
            .clone()
            .unwrap_or_else(|| format!("mrpir-{}", self.device_name))
    }

    /// Get the display name for Home Assistant.
    pub fn display_name(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.device_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.sensor.gpio_pin, 17);
        assert_eq!(config.mqtt.port, 1883);
        assert!(config.mqtt.enabled);
    }

    #[test]
    fn test_validation_invalid_gpio() {
        let mut config = Config::default();
        config.sensor.gpio_pin = 50;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_sun_times_without_location() {
        let mut config = Config::default();
        config.night_mode.use_sun_times = true;
        config.location.latitude = None;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_client_id_generation() {
        let mut config = Config::default();
        config.device_name = "bedroom".to_string();
        config.mqtt.client_id = None;
        assert_eq!(config.client_id(), "mrpir-bedroom");

        config.mqtt.client_id = Some("custom-id".to_string());
        assert_eq!(config.client_id(), "custom-id");
    }
}
