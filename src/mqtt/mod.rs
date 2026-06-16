//! MQTT client and Home Assistant discovery for mrpir.

mod client;
mod discovery;

#[cfg(test)]
pub use client::MqttPublishFuture;
pub use client::{MqttClient, MqttEvent, MqttPublisher};
