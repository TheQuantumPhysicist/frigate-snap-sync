use file_sender::path_descriptor::PathDescriptor;
use serde::{Deserialize, Deserializer, de::Error};
use std::{
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
};

const DEFAULT_FRIGATE_TOPIC_PREFIX: &str = "frigate";
const DEFAULT_MQTT_PORT: u16 = 1883;
const DEFAULT_MQTT_KEEP_ALIVE_SECONDS: u64 = 5;
const DEFAULT_MQTT_CLIENT_ID: &str = "sam-frigate-video-sync";

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("Config file doesn't exist in the provided (or default) path: {0}")]
    ConfigFileDoesNotExist(PathBuf),
    #[error("File exists but it could not be read to a string for parsing: {0}")]
    FileExistsButCannotBeReadToString(std::io::Error),
    #[error("Could not parse file to config; either invalid yaml or missing config: {0}")]
    FileFormatCouldNotBeParsed(serde_yml::Error),
}

#[must_use]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct VideoSyncConfig {
    mqtt_frigate_topic_prefix: Option<String>,
    mqtt_host: String,
    mqtt_port: Option<u16>,
    mqtt_keep_alive_seconds: Option<u64>,
    mqtt_username: Option<String>,
    mqtt_password: Option<String>,
    mqtt_client_id: Option<String>,

    frigate_api_address: String,
    frigate_api_proxy: Option<String>,

    #[serde(deserialize_with = "upload_destinations_from_str")]
    upload_destinations: PathDescriptors,
}

impl VideoSyncConfig {
    pub fn from_file_or_default<P: AsRef<Path>>(path: P) -> Result<VideoSyncConfig, ConfigError> {
        if !path.as_ref().exists() {
            return Err(ConfigError::ConfigFileDoesNotExist(
                path.as_ref().to_path_buf(),
            ));
        }

        let config_file_data = std::fs::read_to_string(path)
            .map_err(ConfigError::FileExistsButCannotBeReadToString)?;

        let config: VideoSyncConfig = serde_yml::from_str(&config_file_data)
            .map_err(ConfigError::FileFormatCouldNotBeParsed)?;

        Ok(config)
    }

    pub fn mqtt_frigate_topic_prefix(&self) -> &str {
        self.mqtt_frigate_topic_prefix
            .as_deref()
            .unwrap_or(DEFAULT_FRIGATE_TOPIC_PREFIX)
    }

    pub fn mqtt_host(&self) -> &str {
        &self.mqtt_host
    }

    pub fn mqtt_port(&self) -> u16 {
        self.mqtt_port.unwrap_or(DEFAULT_MQTT_PORT)
    }

    pub fn mqtt_keep_alive_seconds(&self) -> u64 {
        self.mqtt_keep_alive_seconds
            .unwrap_or(DEFAULT_MQTT_KEEP_ALIVE_SECONDS)
    }

    pub fn mqtt_username(&self) -> Option<&str> {
        self.mqtt_username.as_deref()
    }

    pub fn mqtt_password(&self) -> Option<&str> {
        self.mqtt_password.as_deref()
    }

    pub fn mqtt_client_id(&self) -> &str {
        self.mqtt_client_id
            .as_deref()
            .unwrap_or(DEFAULT_MQTT_CLIENT_ID)
    }

    pub fn set_mqtt_frigate_topic_prefix(&mut self, value: Option<String>) {
        self.mqtt_frigate_topic_prefix = value;
    }

    pub fn frigate_api_address(&self) -> &str {
        &self.frigate_api_address
    }

    pub fn frigate_api_proxy(&self) -> Option<&str> {
        match &self.frigate_api_proxy {
            Some(s) => Some(s.as_str()),
            None => None,
        }
    }

    pub fn upload_destinations(&self) -> &PathDescriptors {
        &self.upload_destinations
    }
}

fn upload_destinations_from_str<'de, D>(deserializer: D) -> Result<PathDescriptors, D::Error>
where
    D: Deserializer<'de>,
{
    let d_vec = Vec::<String>::deserialize(deserializer)?;
    if d_vec.is_empty() {
        return Err(D::Error::custom(
            "Upload destinations cannot be empty. Include one at least",
        ));
    }

    let mut result = Vec::with_capacity(d_vec.len());
    for d in d_vec {
        let path_descriptor = PathDescriptor::from_str(&d)
            .map_err(|e| D::Error::custom(format!("Invalid path descriptor provided: {e}")))?;
        result.push(Arc::new(path_descriptor));
    }
    Ok(result.into())
}

// A shallow version of a collection of `PathDescriptor` objects
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct PathDescriptors {
    pub path_descriptors: Arc<Vec<Arc<PathDescriptor>>>,
}

impl From<Arc<Vec<Arc<PathDescriptor>>>> for PathDescriptors {
    fn from(v: Arc<Vec<Arc<PathDescriptor>>>) -> Self {
        Self {
            path_descriptors: v,
        }
    }
}

impl From<Vec<Arc<PathDescriptor>>> for PathDescriptors {
    fn from(v: Vec<Arc<PathDescriptor>>) -> Self {
        Self {
            path_descriptors: Arc::new(v),
        }
    }
}
