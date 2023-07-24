#[macro_use]
extern crate log;
mod config;
use config::Config;
use mqtt::{Message, MessageBuilder};
//use mqtt::{MessageBuilder, topic};
use pir_motion_sensor::sensor::helpers::spawn_detection_threads;
use pir_motion_sensor::sensor::motion::MotionSensor;
//use std::time::Duration;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;
//use std::process::Command;
use std::{sync::Arc, time::SystemTime};
use std::{env, process, process::Command, time::Duration};
use paho_mqtt as mqtt;


#[tokio::main]
async fn main() {
    env_logger::init();
    info!("starting up");

    let config = Config::new();

    // Create a client & define connect options
    info!("Connecting to MQTT server: {}", config.mqtt_server);
    let _host = env::args()
        .nth(1)
        .unwrap_or_else(|| config.mqtt_server.to_string());

    let mut cli = mqtt::Client::new(config.mqtt_server).unwrap_or_else(|e| {
        println!("Error creating the client: {:?}", e);
        process::exit(1);
    });
    
    // Use 5sec timeouts for sync calls.
    cli.set_timeout(Duration::from_secs(5));

    let conn_opts = mqtt::ConnectOptionsBuilder::new()
        .keep_alive_interval(Duration::from_secs(20))
        .user_name(config.mqtt_username)
        .password(config.mqtt_password)
        .clean_session(true)
        .finalize();

    // Connect and wait for it to complete or fail.
    // The default connection uses MQTT v3.x
    if let Err(e) = cli.connect(conn_opts) {
        println!("Unable to connect: {:?}", e);
        process::exit(1);
    }

    // Setup MQTT configuration message
    let msg = MessageBuilder::new()
        .topic(config.config_topic)
        .payload(config.config_payload)
        .qos(0)
        .retained(true)
        .finalize();

    // Publish configuration to the broker
    if let Err(e) = cli.publish(msg) {
        println!("Unable to publish: {:?}", e);
    }
    
    info!("setup PIR sensor: ");

    // channel for sensor data
    #[allow(clippy::type_complexity)]
    let (detections_channel_sender, mut detections_channel_receiver): (
        Sender<(String, SystemTime)>,
        Receiver<(String, SystemTime)>,
    ) = mpsc::channel(10);

    //
    // sensors initialization - check README for more details about sensor parameters
    // in this example we initialize 4 motion sensors
    //
    let sensors_list = vec![
        // Bedroom
        MotionSensor::new(
            String::from("pir_Sensor"), // name
            17,                                 // gpio PIN number
            200,                               // sensor refresh rate in miliseconds
            1000,                               // sensor motion time period in miliseconds
            5,                                 // sensor minimal triggering number
            detections_channel_sender.clone(), // channel where sensor thread will be sending detections
            None, // None for real GPIO usage, Some(Vec<u64>) for unit tests, please check tests/* */
        ),
    ];

    // starting detector in the background
    let mut sensors = Vec::new();

    // bulding list of sensors to use it later
    sensors_list.into_iter().for_each(|sensor| {
        let s = Arc::new(Mutex::new(sensor));
        sensors.push(s);
    });

    // cancellation token which can be later used to stop sensors threads
    let token = CancellationToken::new();

    // helper function to run important threads (via tokio::spawn)
    // you don't have deal this is you don't want to - just leave it as it is
    spawn_detection_threads(sensors, token.clone());

    let mut last_detection = SystemTime::now();
    let mut motion_state = false;

    // main loop
    loop {
        if let Ok(detection_message) = detections_channel_receiver.try_recv() {
            // valid detection received
            // each "valid" detection contains the sensor name and time of detection as SystemTime
          
            let (detection_name, detection_time) = detection_message;
            if detection_time.duration_since(last_detection).unwrap().as_secs() > 1 {
                info!("detection happened, sensor: {detection_name}, time: {detection_time:?} ");
                motion_state = true;

                // Reset last detection time
                last_detection = SystemTime::now();
            
    
                // Publish it to the broker
                if let Err(e) = cli.publish(Message::new(&config.motion_topc, "ON", 0)) {
                    error!("Unable to publish: {:?}", e);
                }
                
                // Shell command
                let hello = Command::new("sh")
                .arg("-c")
                .arg("echo hello")
                .output()
                .expect("failed to execute process");

                tokio::time::sleep(Duration::from_millis(1000)).await;

                info!("detection happened, sensor: {detection_name}, time: {detection_time:?}, test {hello:?} ");
                //
                // put your action here like alarm or turn on/off light
                // to interact with rest GPIOs you can check rppal lib examples here: https://github.com/golemparts/rppal/tree/master/examples
                //
            }         
        }
        if SystemTime::now().duration_since(last_detection).unwrap().as_secs() > 2 && motion_state == true {
            info!("Reset sensor after 2 seconds of no motion: timeout: {}", SystemTime::now().duration_since(last_detection).unwrap().as_secs());
            
            motion_state = false;
            // Publish it to the broker
            if let Err(e) = cli.publish(Message::new(&config.motion_topc, "OFF", 0)) {
                error!("Unable to publish: {:?}", e);
            }
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}