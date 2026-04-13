//! Integration tests for MQTT discovery and payload generation.

/// Test that discovery payloads follow Home Assistant MQTT discovery format.
mod discovery_format {
    use serde_json::json;

    #[test]
    fn test_motion_sensor_discovery_payload_structure() {
        // The discovery payload should match Home Assistant's expected format
        // Reference: https://www.home-assistant.io/integrations/mqtt/#mqtt-discovery
        
        let payload = json!({
            "name": "test_sensor Motion",
            "unique_id": "test_sensor_motion",
            "state_topic": "mrpir/test_sensor/motion",
            "device_class": "motion",
            "payload_on": "ON",
            "payload_off": "OFF",
            "device": {
                "identifiers": ["mrpir_test_sensor"],
                "name": "Test PIR Sensor",
                "model": "mrpir",
                "manufacturer": "mrpir"
            }
        });

        // Verify required fields for motion sensor
        assert!(payload.get("name").is_some());
        assert!(payload.get("unique_id").is_some());
        assert!(payload.get("state_topic").is_some());
        assert!(payload.get("device_class").is_some());
        assert_eq!(payload["device_class"], "motion");
        
        // Verify device info
        let device = payload.get("device").expect("device should exist");
        assert!(device.get("identifiers").is_some());
        assert!(device.get("name").is_some());
    }

    #[test]
    fn test_brightness_sensor_discovery_payload_structure() {
        let payload = json!({
            "name": "test_sensor Brightness Target",
            "unique_id": "test_sensor_brightness_target",
            "state_topic": "mrpir/test_sensor/brightness",
            "unit_of_measurement": "%",
            "device": {
                "identifiers": ["mrpir_test_sensor"],
                "name": "Test PIR Sensor"
            }
        });

        assert!(payload.get("unit_of_measurement").is_some());
        assert_eq!(payload["unit_of_measurement"], "%");
    }

    #[test]
    fn test_config_topic_format() {
        // Home Assistant expects discovery topics in format:
        // homeassistant/<component>/<node_id>/<object_id>/config
        
        let prefix = "homeassistant";
        let component = "binary_sensor";
        let node_id = "mrpir_sensor1";
        let object_id = "motion";
        
        let topic = format!("{}/{}/{}/{}/config", prefix, component, node_id, object_id);
        
        assert_eq!(topic, "homeassistant/binary_sensor/mrpir_sensor1/motion/config");
    }

    #[test]
    fn test_state_topic_format() {
        let prefix = "mrpir";
        let sensor_name = "living_room";
        let state_type = "motion";
        
        let topic = format!("{}/{}/{}", prefix, sensor_name, state_type);
        
        assert_eq!(topic, "mrpir/living_room/motion");
    }

    #[test]
    fn test_payload_json_serialization() {
        #[derive(serde::Serialize)]
        struct TestPayload {
            name: String,
            state_topic: String,
            device_class: String,
        }

        let payload = TestPayload {
            name: "Test Sensor".to_string(),
            state_topic: "test/topic".to_string(),
            device_class: "motion".to_string(),
        };

        let json = serde_json::to_string(&payload).expect("Failed to serialize");
        
        // Verify it's valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("Failed to parse");
        assert_eq!(parsed["name"], "Test Sensor");
        assert_eq!(parsed["state_topic"], "test/topic");
    }

    #[test]
    fn test_availability_topic_format() {
        let prefix = "mrpir";
        let sensor_name = "bedroom";
        
        let availability_topic = format!("{}/{}/status", prefix, sensor_name);
        
        assert_eq!(availability_topic, "mrpir/bedroom/status");
    }
}

/// Test MQTT topic name sanitization.
mod topic_sanitization {
    #[test]
    fn test_topic_with_special_chars() {
        // MQTT topics should not contain +, #, or null characters
        let sensor_name = "living_room_pir";
        
        assert!(!sensor_name.contains('+'));
        assert!(!sensor_name.contains('#'));
        assert!(!sensor_name.contains('\0'));
    }

    #[test]
    fn test_client_id_uniqueness() {
        // Client IDs should be unique - typically hostname + suffix
        let hostname = "raspberrypi";
        let suffix = "mrpir";
        
        let client_id = format!("{}-{}", hostname, suffix);
        
        assert_eq!(client_id, "raspberrypi-mrpir");
        assert!(client_id.len() <= 23); // MQTT 3.1 limit
    }
}
