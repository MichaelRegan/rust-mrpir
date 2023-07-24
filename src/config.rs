use std::env;

pub struct Config {
    pub mqtt_server: String,
    pub mqtt_username: String,
    pub mqtt_password: String,
    pub config_payload: String,
    pub config_topic: String,
    pub motion_topc: String,
}

impl Config {
    pub fn new() -> Self {
        env_logger::init();

        let mqtt_server = env::var("MQTT_SERVER").expect("Please set MQTT_SERVER as an environment variable: tcp://[servername or IP]:port");
        let device_name = env::var("DEVICE_NAME").expect("Please set DEVICE_NAME as an environment variable");
        let config_payload = env::var("CONFIG_PAYLOAD").unwrap_or_else(|_| r#"{"name": "officetest_motion", "device_class": "motion", "unique_id": "officetest_id", "state_topic": "homeassistant/binary_sensor/officetest/state"}"#.to_string());
        let mqtt_username: String = env::var("MQTT_USERNAME").unwrap_or_else(|_| "iot".to_string());
        let mqtt_password: String = env::var("MQTT_PASSWORD").expect("Please set MQTT_PASSWORD as an environment variable");

        let config_topic = "homeassistant/binary_sensor/".to_string() + &device_name + "/config";
        let motion_topc = "homeassistant/binary_sensor/".to_string() + &device_name + "/state";

        info!("Using MQTT Server: {mqtt_server}");
        info!("Using Device Name: {device_name}");
        info!("Using Config Topic: {config_topic}");
        info!("Using Config Payload: {config_payload}");
        info!("Using MQTT Username: {mqtt_username}");
        info!("using metric topic: {}", &motion_topc);

        Self {
            mqtt_server,
            config_payload,
            config_topic,
            motion_topc,
            mqtt_username,
            mqtt_password,
        }
    }
}