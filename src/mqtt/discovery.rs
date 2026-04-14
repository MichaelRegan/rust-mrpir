//! Home Assistant MQTT Discovery payload structures.

use serde::Serialize;

/// Home Assistant device information.
#[derive(Debug, Clone, Serialize)]
pub struct HaDevice {
    /// Device identifiers (usually client_id)
    pub identifiers: Vec<String>,

    /// Human-readable device name
    pub name: String,

    /// Device manufacturer
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,

    /// Device model
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Software version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sw_version: Option<String>,
}

/// Origin information for MQTT discovery (required by Home Assistant 2024.1+).
#[derive(Debug, Clone, Serialize)]
pub struct HaOrigin {
    /// Name of the application providing this entity
    pub name: String,

    /// Software version of the application
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sw: Option<String>,

    /// Support URL for the application
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Home Assistant MQTT Discovery payload for a binary sensor.
#[derive(Debug, Clone, Serialize)]
pub struct HaDiscoveryPayload {
    /// Sensor name displayed in Home Assistant
    pub name: String,

    /// Device class (motion, occupancy, etc.)
    pub device_class: String,

    /// Unique identifier for this entity
    pub unique_id: String,

    /// MQTT topic where state updates are published
    pub state_topic: String,

    /// Payload that indicates motion detected
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_on: Option<String>,

    /// Payload that indicates no motion
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_off: Option<String>,

    /// Availability topic
    #[serde(skip_serializing_if = "Option::is_none")]
    pub availability_topic: Option<String>,

    /// Payload that indicates the device is available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_available: Option<String>,

    /// Payload that indicates the device is not available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_not_available: Option<String>,

    /// Device information for grouping in Home Assistant
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<HaDevice>,

    /// Origin information (required by Home Assistant 2024.1+)
    #[serde(rename = "o")]
    pub origin: HaOrigin,

    /// Icon override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

impl HaDiscoveryPayload {
    /// Create a new motion sensor discovery payload.
    pub fn motion_sensor(
        device_name: &str,
        display_name: &str,
        client_id: &str,
        ha_prefix: &str,
    ) -> Self {
        let state_topic = format!("{ha_prefix}/binary_sensor/{device_name}/state");
        let availability_topic = format!("{ha_prefix}/binary_sensor/{device_name}/availability");

        Self {
            name: format!("{display_name} Motion"),
            device_class: "motion".to_string(),
            // Match Python format for seamless migration: pir_{device}_id_{device}_id
            unique_id: format!("pir_{device_name}_id_{device_name}_id"),
            state_topic,
            payload_on: Some("ON".to_string()),
            payload_off: Some("OFF".to_string()),
            availability_topic: Some(availability_topic),
            payload_available: Some("online".to_string()),
            payload_not_available: Some("offline".to_string()),
            device: Some(HaDevice {
                identifiers: vec![client_id.to_string()],
                name: display_name.to_string(),
                manufacturer: Some("mrpir".to_string()),
                model: Some("PIR Motion Sensor".to_string()),
                sw_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            origin: HaOrigin {
                name: "mrpir".to_string(),
                sw: Some(env!("CARGO_PKG_VERSION").to_string()),
                url: Some("https://github.com/MichaelRegan/rust-mrpir".to_string()),
            },
            icon: Some("mdi:motion-sensor".to_string()),
        }
    }

    /// Get the discovery config topic.
    pub fn config_topic(device_name: &str, ha_prefix: &str) -> String {
        format!("{ha_prefix}/binary_sensor/{device_name}/config")
    }

    /// Serialize to JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discovery_payload() {
        let payload = HaDiscoveryPayload::motion_sensor(
            "bedroom",
            "Bedroom",
            "mrpir-bedroom",
            "homeassistant",
        );

        assert_eq!(payload.name, "Bedroom Motion");
        assert_eq!(payload.device_class, "motion");
        assert_eq!(
            payload.state_topic,
            "homeassistant/binary_sensor/bedroom/state"
        );
        assert_eq!(payload.origin.name, "mrpir");

        let json = payload.to_json().unwrap();
        assert!(json.contains("motion"));
        assert!(json.contains("bedroom"));
        // Verify origin is serialized with abbreviated key "o"
        assert!(json.contains(r#""o":"#));
    }

    #[test]
    fn test_config_topic() {
        let topic = HaDiscoveryPayload::config_topic("bedroom", "homeassistant");
        assert_eq!(topic, "homeassistant/binary_sensor/bedroom/config");
    }
}
