//! Tests for the `Config` struct.
//!
//! These tests ensure that the `Config` struct is working correctly and that its methods and
//! functionality are correct.
//!
//! The tests in this file can be run with `cargo test`.
//!
//! [`Config`]: ../src/config.rs.html
//! 
#[cfg(test)]
mod tests {
    use super::*;

    /// Test the `Clone` implementation for the `Config` struct.
    #[test]
    fn test_clone() {
        let config = Config {
            mqtt_server: "test.mosquitto.org".to_string(),
            mqtt_port: 1883,
            config_payload: "test payload".to_string(),
            config_topic: "test/config".to_string(),
            motion_topic: "test/motion".to_string(),
            mqtt_username: Some("testuser".to_string()),
            mqtt_password: Some("testpass".to_string()),
            mqtt_persistence_file: Some("/tmp/mqtt-persistence".to_string()),
            mqtt_client_id: "test-client".to_string(),
        };

        let cloned_config = config.clone();

        assert_eq!(config.mqtt_server, cloned_config.mqtt_server);
        assert_eq!(config.mqtt_port, cloned_config.mqtt_port);
        assert_eq!(config.config_payload, cloned_config.config_payload);
        assert_eq!(config.config_topic, cloned_config.config_topic);
        assert_eq!(config.motion_topic, cloned_config.motion_topic);
        assert_eq!(config.mqtt_username, cloned_config.mqtt_username);
        assert_eq!(config.mqtt_password, cloned_config.mqtt_password);
        assert_eq!(config.mqtt_persistence_file, cloned_config.mqtt_persistence_file);
        assert_eq!(config.mqtt_client_id, cloned_config.mqtt_client_id);
    }
}