mod config;
mod state;
pub mod system;

use config::VideoSyncConfig;
use file_sender::{make_store, path_descriptor::PathDescriptor};
use frigate_api_caller::{config::FrigateApiConfig, make_frigate_client};
use logging::init_logging;
use mqtt_handler::config::MqttHandlerConfig;
use std::sync::Arc;
use system::SyncSystem;

impl From<&VideoSyncConfig> for FrigateApiConfig {
    fn from(config: &VideoSyncConfig) -> Self {
        Self {
            frigate_api_base_url: config.frigate_api_address().to_string(),
            frigate_api_proxy: config.frigate_api_proxy().map(str::to_string),
        }
    }
}

impl From<&VideoSyncConfig> for MqttHandlerConfig {
    fn from(config: &VideoSyncConfig) -> Self {
        MqttHandlerConfig {
            mqtt_frigate_topic_prefix: config.mqtt_frigate_topic_prefix().to_string(),
            mqtt_host: config.mqtt_host().to_string(),
            mqtt_port: config.mqtt_port(),
            mqtt_keep_alive_seconds: config.mqtt_keep_alive_seconds(),
            mqtt_username: config.mqtt_username().map(ToOwned::to_owned),
            mqtt_password: config.mqtt_password().map(ToOwned::to_owned),
            mqtt_client_id: config.mqtt_client_id().to_string(),
        }
    }
}

pub async fn run() -> anyhow::Result<()> {
    init_logging();

    let config = VideoSyncConfig::from_file_or_default("config.yaml")?;

    let frigate_api_maker = move |cfg: &FrigateApiConfig| make_frigate_client(cfg.clone());
    let file_sender_maker = move |pd: &Arc<PathDescriptor>| make_store(pd);

    let (stop_sender, stop_receiver) = tokio::sync::mpsc::unbounded_channel();

    ctrlc::set_handler(move || {
        tracing::info!("Sending a terminate (Ctrl+C) signal");
        stop_sender
            .send(())
            .expect("Could not send signal on channel.");
    })
    .expect("Error setting Ctrl+C handler");

    let mut sync_sys = SyncSystem::new(
        config,
        frigate_api_maker,
        file_sender_maker,
        Some(stop_receiver),
    );

    sync_sys.run().await?;

    Ok(())
}
