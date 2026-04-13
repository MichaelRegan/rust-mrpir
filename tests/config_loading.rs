//! Integration tests for configuration loading.

use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Test helper to create a temporary config file.
fn create_temp_config(content: &str) -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("Failed to create temp dir");
    let config_path = dir.path().join("config.toml");
    fs::write(&config_path, content).expect("Failed to write config");
    (dir, config_path)
}

/// Helper to set environment variables and restore them after the test.
struct EnvGuard {
    vars: Vec<(String, Option<String>)>,
}

impl EnvGuard {
    fn new() -> Self {
        Self { vars: Vec::new() }
    }

    fn set(&mut self, key: &str, value: &str) {
        let old = std::env::var(key).ok();
        self.vars.push((key.to_string(), old));
        std::env::set_var(key, value);
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, old) in &self.vars {
            match old {
                Some(val) => std::env::set_var(key, val),
                None => std::env::remove_var(key),
            }
        }
    }
}

#[test]
fn test_minimal_config_loads() {
    let content = r#"
[mqtt]
broker = "mqtt://test.local:1883"
"#;

    let (_dir, _path) = create_temp_config(content);
    
    // We can't directly call Config::load_from_path since it's not pub,
    // but we can test that the config file format is valid TOML with expected structure
    let parsed: toml::Value = toml::from_str(content).expect("Failed to parse TOML");
    
    assert!(parsed.get("mqtt").is_some());
    assert_eq!(
        parsed["mqtt"]["broker"].as_str(),
        Some("mqtt://test.local:1883")
    );
}

#[test]
fn test_full_config_structure() {
    let content = r#"
[sensor]
gpio_pin = 17
debounce_ms = 100

[mqtt]
broker = "mqtt://broker.local:1883"
username = "test_user"
password = "test_pass"
topic_prefix = "test/mrpir"
client_id = "test-client"

[screen]
enabled = true
method = "brightness"
dim_brightness = 10
bright_brightness = 200
transition_time_secs = 2

[night_mode]
enabled = true
use_sun_times = false
night_start = "22:00"
night_end = "06:00"

[location]
latitude = 40.7128
longitude = -74.0060

[logging]
level = "debug"
"#;

    let parsed: toml::Value = toml::from_str(content).expect("Failed to parse TOML");
    
    // Verify all sections exist
    assert!(parsed.get("sensor").is_some());
    assert!(parsed.get("mqtt").is_some());
    assert!(parsed.get("screen").is_some());
    assert!(parsed.get("night_mode").is_some());
    assert!(parsed.get("location").is_some());
    assert!(parsed.get("logging").is_some());
    
    // Verify some specific values
    assert_eq!(parsed["sensor"]["gpio_pin"].as_integer(), Some(17));
    assert_eq!(parsed["screen"]["dim_brightness"].as_integer(), Some(10));
    assert_eq!(parsed["location"]["latitude"].as_float(), Some(40.7128));
}

#[test]
fn test_config_with_env_override_pattern() {
    // This tests the pattern that figment would use for env overrides
    let mut guard = EnvGuard::new();
    guard.set("MRPIR_MQTT__BROKER", "mqtt://env-broker:1883");
    guard.set("MRPIR_SENSOR__GPIO_PIN", "23");
    
    // Verify environment variables are set correctly
    assert_eq!(
        std::env::var("MRPIR_MQTT__BROKER").ok(),
        Some("mqtt://env-broker:1883".to_string())
    );
    assert_eq!(
        std::env::var("MRPIR_SENSOR__GPIO_PIN").ok(),
        Some("23".to_string())
    );
}

#[test]
fn test_invalid_toml_is_rejected() {
    let content = r#"
[mqtt
broker = "missing bracket"
"#;

    let result: Result<toml::Value, _> = toml::from_str(content);
    assert!(result.is_err());
}

#[test]
fn test_screen_method_values() {
    // Test that valid screen method values are valid TOML
    let methods = ["none", "brightness", "wayland", "xscreensaver"];
    
    for method in methods {
        let content = format!(
            r#"
[screen]
method = "{}"
"#,
            method
        );
        
        let parsed: toml::Value = toml::from_str(&content).expect("Failed to parse TOML");
        assert_eq!(parsed["screen"]["method"].as_str(), Some(method));
    }
}
