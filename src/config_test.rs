use super::*;

#[test]
fn test_new() {
    // Set up environment variables
    env::set_var("MQTT_SERVER", "tcp://localhost:1883");
    env::set_var("DEVICE_NAME", "test_device");
    env::set_var("CONFIG_PAYLOAD", r#"{"name": "test_motion", "device_class": "motion", "unique_id": "test_id", "state_topic": "homeassistant/binary_sensor/test/state"}"#);
    env::set_var("MQTT_USERNAME", "test_user");
    env::set_var("MQTT_PASSWORD", "test_password");

    let config = Config::new();

    assert_eq!(config.mqtt_server, "tcp://localhost:1883");
    assert_eq!(config.mqtt_username, "test_user");
    assert_eq!(config.mqtt_password, "test_password");
    assert_eq!(config.config_payload, r#"{"name": "test_motion", "device_class": "motion", "unique_id": "test_id", "state_topic": "homeassistant/binary_sensor/test/state"}"#);
    assert_eq!(config.config_topic, "homeassistant/binary_sensor/test_device/config");
    assert_eq!(config.motion_topc, "homeassistant/binary_sensor/test_device/state");
}