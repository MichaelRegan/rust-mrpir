//! MQTT client and Home Assistant discovery for mrpir.

mod client;
mod discovery;

pub use client::MqttClient;
pub use discovery::{HaDevice, HaDiscoveryPayload};
