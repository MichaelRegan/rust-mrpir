//! Manages basic configuration variables for the MQTT server and PIR sensor.
//!
//! The following environment variables are used and should be stored in an “.env” file:
//!
//! * mqtt_server = Defualt is 'mqtt://localhost'
//! * mqtt_port = Default is '1883'
//! * mqtt_username = Default is 'iot'
//! * mqtt_password = Default is 'password'
//! * motion_topic = Constructed from the device name
//! * motion_topic = Constructed from the device name
//! * motion_topic = Constructed from the device name
//! * mqtt_persistence_file = Default is '/tmp/mqtt_persistence_file'
//! * mqtt_client_id = Required, no default
//!
//!
//! [`PIR sensor`]: https://michaelregan.github.io/posts/motion-sensor-for-pi/
#[doc(inline)]

use std::env;

/// Gets environment variables and constructs required configuration attributes
pub struct Config {

    /// The required device name from the environment
    pub device_name: String,

    /// The MQTT server address from the environment, or use the default
    pub mqtt_server: String,
    
    /// The MQTT port from the environment, or use the default
    pub mqtt_port: String,
    
    /// The required MQTT username from the environment, or use the default: 'iot'
    pub mqtt_username: String,
    
    /// The MQTT password from the environment, or use the default
    pub mqtt_password: String,
    
    /// The MQTT payload for the device configuration
    pub config_payload: String,
    
    /// The MQTT topic for the device configuration
    pub config_topic: String,
    
    /// The MQTT topic for the motion sensor state
    pub motion_topic: String,
    
    /// The MQTT persistence file path from the environment, or use the default
    pub mqtt_persistence_file: String,
    
    /// The required device name from the environment, or panic if it's not set
    pub mqtt_client_id: String,

    /// The required pin used for the PIR sensor
    pub pir_pin: u8,
}

// Implementation of the Config struct
impl Config {
    /// Create a new configuration from environment variables.
    ///
    /// Required environment variables:
    /// * mqtt_client_id
    /// * device_name
    /// * pir_pin
    /// 
    /// Optional environment variables:
    /// * mqtt_server [default: 'mqtt://localhost']
    /// * mqtt_port [default: '1883']
    /// * mqtt_username [default: 'iot']
    /// * mqtt_password [default: 'password']
    /// * mqtt_persistence_file [default: '/tmp/mqtt_persistence_file']
    /// 
    pub fn new() -> Self {

        // Get the MQTT server address from the environment, or use the default
        let mqtt_server_url = env::var("MQTT_SERVER").unwrap_or_else(|_| {
            warn!("MQTT_SERVER not set, using default value 'mqtt://localhost'");
            "mqtt://localhost".to_string()
        });

        // Get the MQTT port from the environment, or use the default
        let mqtt_port = env::var("MQTT_PORT").unwrap_or_else(|_| {
            warn!("MQTT_PORT not set, using default value '1883'");
            "1883".to_string()
        });

        // Construct the MQTT server address
        let mqtt_server = mqtt_server_url + ":" + &mqtt_port;

        // Get the required MQTT client ID from the environment, or panic if it's not set
        let mqtt_client_id: String = match env::var("MQTT_CLIENT_ID") {
            Ok(val) => val,
            Err(e) => {
                error!("MQTT_CLIENT_ID not set, please set it as an environment variable to uniquely identify this device:");
                panic!("Please set MQTT_CLIENT_ID as an environment variable to uniquely identify this device: {}", e)
            }
        };

        // Get the MQTT username from the environment, or use the default
        let mqtt_username: String = env::var("MQTT_USERNAME").unwrap_or_else(|_| {
            warn!("MQTT_USERNAME not set, using default value 'iot'");
            "iot".to_string()
        });

        // Get the MQTT password from the environment, or use the default
        let mqtt_password: String = env::var("MQTT_PASSWORD").unwrap_or_else(|_| {
            warn!("MQTT_PASSWORD not set, using default value 'password'");
            "password".to_string()
        });

        // Get the requied device name from the environment, or panic if it's not set
        let device_name = match env::var("DEVICE_NAME") {
            Ok(val) => val,
            Err(e) => {
                error!("DEVICE_NAME not set, please set it as an environment variable for the PIR device:");
                panic!("Please set DEVICE_NAME as an environment variable for the PIR device: {}", e)
            }            
        };

        // Get the MQTT persistence file path from the environment, or use the default
        let mqtt_persistence_file: String = env::var("MQTT_PERSISTENCE_FILE").unwrap_or_else(|_| {
            warn!("MQTT_PERSISTENCE_FILE not set, using default value '/tmp/mqtt_persistence_file'");
            "/tmp/mqtt_persistence_file".to_string()
        });

        // Construct the MQTT configuration topic for the device configuration
        let config_topic = "homeassistant/binary_sensor/".to_string() + &device_name + "/config";

        // Convert the payload to a string
        //let config_payload = config_payload_build.to_owned();
        let config_payload = format!("{{\"name\": \"{device_name}_motion\", \"device_class\": \"motion\", \"unique_id\": \"{mqtt_client_id}_{device_name}_id\", \"state_topic\": \"homeassistant/binary_sensor/{device_name}/state\"}}");

        // Construct the MQTT topic for the motion sensor state
        let motion_topic = format!("homeassistant/binary_sensor/{device_name}/state");

        let pir_pin: u8 = match env::var("PIR_PIN") {
            Ok(val) => val.parse().unwrap(),
            Err(e) => {
                error!("PIR_PIN not set, please set it as an environment variable for the PIR device:");
                panic!("Please set PIR_PIN as an environment variable for the PIR device: {}", e)
            }
        };

        // Log the configuration
        info!("Using MQTT Server: {mqtt_server}");
        info!("Using MQTT Port: {mqtt_port}");
        info!("Using Device Name: {device_name}");
        info!("Using Config Topic: {config_topic}");
        info!("Using Config Payload: {config_payload}");
        info!("Using MQTT Username: {mqtt_username}");
        info!("using metion topic: {}", &motion_topic);
        info!("Using MQTT Persistence File: {mqtt_persistence_file}");
        info!("Using MQTT Client ID: {mqtt_client_id}");
        info!("Using PIR Pin: {pir_pin}");
        
        // Return the configuration
        Self {
            device_name,
            mqtt_server,
            mqtt_port,
            config_payload,
            config_topic,
            motion_topic,
            mqtt_username,
            mqtt_password,
            mqtt_persistence_file,
            mqtt_client_id,
            pir_pin,
        }
    }
}

/// This implementation creates a new Config struct with all of the fields cloned from the original struct using the clone method. This allows you to create a new Config instance that is a copy of an existing instance.
impl Clone for Config {
    fn clone(&self) -> Self {
        Self {
            mqtt_server: self.mqtt_server.clone(),
            mqtt_port: self.mqtt_port.clone(),
            config_payload: self.config_payload.clone(),
            config_topic: self.config_topic.clone(),
            motion_topic: self.motion_topic.clone(),
            mqtt_username: self.mqtt_username.clone(),
            mqtt_password: self.mqtt_password.clone(),
            mqtt_persistence_file: self.mqtt_persistence_file.clone(),
            mqtt_client_id: self.mqtt_client_id.clone(),
            pir_pin: self.pir_pin.clone(),
            device_name: self.device_name.clone(),
        }
    }
}