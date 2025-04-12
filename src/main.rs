use config::VideoSyncConfig;
use logging::init_logging;
use mqtt_handler::MqttHandler;

mod config;
mod mqtt_handler;

// TODO: next:
// 1. Do more parsing into the enum `CapturedPayloads`, and use it to send results with some sender

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logging();

    let config = VideoSyncConfig::from_file_or_default("config.yaml")?;

    let mut handler = MqttHandler::new(config).await?;

    handler.wait().await;

    Ok(())
}
