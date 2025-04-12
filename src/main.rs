use config::VideoSyncConfig;
use logging::init_logging;
use mqtt_handler::config::MqttHandlerConfig;

mod config;

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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logging();

    let config = VideoSyncConfig::from_file_or_default("config.yaml")?;

    let mqtt_config = MqttHandlerConfig::from(&config);

    let mut handler = mqtt_handler::MqttHandler::new(mqtt_config)?;

    handler.wait().await;

    Ok(())
}
