use file_sender::path_descriptor::PathDescriptor;
use frigate_api_caller::config::FrigateApiAuthConfig;
use serde::{Deserialize, Deserializer, de::Error};
use std::{
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
};

const DEFAULT_FRIGATE_TOPIC_PREFIX: &str = "frigate";
const DEFAULT_MQTT_PORT: u16 = 1883;
const DEFAULT_MQTT_KEEP_ALIVE_SECONDS: u64 = 5;
const DEFAULT_MQTT_CLIENT_ID: &str = "sam-frigate-snap-sync";
const DEFAULT_DELAY_AFTER_STARTUP: u64 = 0;

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("Config file doesn't exist in the provided path. Given path: `{0}`")]
    ConfigFileDoesNotExist(PathBuf),
    #[error("File exists but it could not be read to a string for parsing: `{0}`")]
    FileExistsButCannotBeReadToString(std::io::Error),
    #[error("Could not parse file to config; either invalid yaml or missing config: `{0}`")]
    FileFormatCouldNotBeParsed(serde_yml::Error),
    #[error("Invalid Frigate API auth config: `{0}`")]
    InvalidFrigateApiAuthConfig(String),
    #[error("Could not read Frigate API password file `{0}`: `{1}`")]
    FrigateApiPasswordFileCouldNotBeRead(PathBuf, std::io::Error),
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
    frigate_api_username: Option<String>,
    frigate_api_password: Option<String>,
    frigate_api_password_file: Option<PathBuf>,

    #[serde(deserialize_with = "upload_destinations_from_str")]
    upload_destinations: PathDescriptors,

    delay_after_startup: Option<u64>,
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

    pub fn frigate_api_auth(&self) -> Result<Option<FrigateApiAuthConfig>, ConfigError> {
        match (
            &self.frigate_api_username,
            &self.frigate_api_password,
            &self.frigate_api_password_file,
        ) {
            (None, None, None) => Ok(None),
            (Some(_), Some(_), Some(_)) => Err(ConfigError::InvalidFrigateApiAuthConfig(
                "set only one of frigate_api_password or frigate_api_password_file".to_string(),
            )),
            (Some(username), Some(password), None) => Ok(Some(FrigateApiAuthConfig {
                username: username.clone(),
                password: password.clone(),
            })),
            (Some(username), None, Some(path)) => {
                let password = std::fs::read_to_string(path)
                    .map_err(|e| ConfigError::FrigateApiPasswordFileCouldNotBeRead(path.clone(), e))?
                    .trim()
                    .to_string();
                Ok(Some(FrigateApiAuthConfig {
                    username: username.clone(),
                    password,
                }))
            }
            (Some(_), None, None) => Err(ConfigError::InvalidFrigateApiAuthConfig(
                "frigate_api_password or frigate_api_password_file is required when Frigate API auth is configured".to_string(),
            )),
            (None, _, _) => Err(ConfigError::InvalidFrigateApiAuthConfig(
                "frigate_api_username is required when Frigate API auth is configured".to_string(),
            )),
        }
    }

    pub fn upload_destinations(&self) -> &PathDescriptors {
        &self.upload_destinations
    }

    pub fn delay_after_startup(&self) -> std::time::Duration {
        let delay = self
            .delay_after_startup
            .unwrap_or(DEFAULT_DELAY_AFTER_STARTUP);

        std::time::Duration::from_secs(delay)
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

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ExpectedFrigateApiAuth {
        None,
        Password,
        PasswordFile,
        Error,
    }

    fn workspace_root() -> std::path::PathBuf {
        use std::str::FromStr;
        let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");
        let cargo_manifest_dir = std::path::PathBuf::from_str(cargo_manifest_dir).unwrap();
        let workspace_root = cargo_manifest_dir.parent().unwrap();
        workspace_root.to_owned()
    }

    #[test]
    fn example_config() {
        let _config =
            VideoSyncConfig::from_file_or_default(workspace_root().join("config.yaml.example"))
                .unwrap();
    }

    #[rstest]
    #[case(None, None, false, ExpectedFrigateApiAuth::None)]
    #[case(Some("snap-sync"), None, false, ExpectedFrigateApiAuth::Error)]
    #[case(None, Some("secret-password"), false, ExpectedFrigateApiAuth::Error)]
    #[case(None, None, true, ExpectedFrigateApiAuth::Error)]
    #[case(
        Some("snap-sync"),
        Some("secret-password"),
        false,
        ExpectedFrigateApiAuth::Password
    )]
    #[case(Some("snap-sync"), None, true, ExpectedFrigateApiAuth::PasswordFile)]
    #[case(None, Some("secret-password"), true, ExpectedFrigateApiAuth::Error)]
    #[case(
        Some("snap-sync"),
        Some("secret-password"),
        true,
        ExpectedFrigateApiAuth::Error
    )]
    fn frigate_api_auth_validation_matrix(
        #[case] username: Option<&str>,
        #[case] password: Option<&str>,
        #[case] has_password_file: bool,
        #[case] expected: ExpectedFrigateApiAuth,
    ) {
        let password_file = if has_password_file {
            let password_file = tempfile::NamedTempFile::new().unwrap();
            std::fs::write(password_file.path(), "file-password\n").unwrap();
            Some(password_file)
        } else {
            None
        };
        let password_file_path = password_file
            .as_ref()
            .map(|password_file| password_file.path().to_path_buf());

        let result = test_config(
            username.map(ToString::to_string),
            password.map(ToString::to_string),
            password_file_path,
        )
        .frigate_api_auth();

        match expected {
            ExpectedFrigateApiAuth::None => assert!(result.unwrap().is_none()),
            ExpectedFrigateApiAuth::Password => {
                let auth = result.unwrap().unwrap();
                assert_eq!(auth.username, "snap-sync");
                assert_eq!(auth.password, "secret-password");
            }
            ExpectedFrigateApiAuth::PasswordFile => {
                let auth = result.unwrap().unwrap();
                assert_eq!(auth.username, "snap-sync");
                assert_eq!(auth.password, "file-password");
            }
            ExpectedFrigateApiAuth::Error => {
                assert!(matches!(
                    result.unwrap_err(),
                    ConfigError::InvalidFrigateApiAuthConfig(_)
                ));
            }
        }
    }

    fn test_config(
        frigate_api_username: Option<String>,
        frigate_api_password: Option<String>,
        frigate_api_password_file: Option<PathBuf>,
    ) -> VideoSyncConfig {
        VideoSyncConfig {
            mqtt_frigate_topic_prefix: None,
            mqtt_host: "127.0.0.1".to_string(),
            mqtt_port: None,
            mqtt_keep_alive_seconds: None,
            mqtt_username: None,
            mqtt_password: None,
            mqtt_client_id: None,
            frigate_api_address: "https://127.0.0.1:8971".to_string(),
            frigate_api_proxy: None,
            frigate_api_username,
            frigate_api_password,
            frigate_api_password_file,
            upload_destinations: vec![Arc::new(
                PathDescriptor::from_str("local:path=/tmp").unwrap(),
            )]
            .into(),
            delay_after_startup: None,
        }
    }
}
