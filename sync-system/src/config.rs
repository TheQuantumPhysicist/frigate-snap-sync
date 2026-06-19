use file_sender::path_descriptor::PathDescriptor;
use frigate_api_caller::config::FrigateApiAuthConfig;
use serde::{Deserialize, Deserializer, de::Error};
use std::{
    fmt,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
};
use zeroize::{Zeroize, ZeroizeOnDrop};

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
    #[error("Frigate API password file could not be read. Path: `{0}`. Error: {1}")]
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
    frigate_api_auth: Option<FrigateApiAuthSource>,

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

    pub fn upload_destinations(&self) -> &PathDescriptors {
        &self.upload_destinations
    }

    pub fn delay_after_startup(&self) -> std::time::Duration {
        let delay = self
            .delay_after_startup
            .unwrap_or(DEFAULT_DELAY_AFTER_STARTUP);

        std::time::Duration::from_secs(delay)
    }

    // - The block is validated into a legal source at deserialization time.
    // - So this just hands out the parsed value.
    // - Resolving it (which may read a password file) is a separate, explicit step.
    pub fn frigate_api_auth(&self) -> Option<&FrigateApiAuthSource> {
        self.frigate_api_auth.as_ref()
    }
}

// - The legal, parsed choices for Frigate auth credentials.
// - username plus exactly one of an inline password or a password file.
// - "Both secrets" and "no secret" cannot be represented, so they cannot exist
//   past deserialization; the Deserialize impl rejects them at the parse boundary.
#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop)]
pub enum FrigateApiAuthSource {
    Inline {
        #[zeroize(skip)]
        username: String,
        password: String,
    },
    File {
        #[zeroize(skip)]
        username: String,
        #[zeroize(skip)]
        password_file: PathBuf,
    },
}

// Custom Debug so the inline password never reaches logs or error output.
// VideoSyncConfig derives Debug and holds this type, so the derived form would
// otherwise print the secret.
impl fmt::Debug for FrigateApiAuthSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inline {
                username,
                password: _,
            } => f
                .debug_struct("FrigateApiAuthSource::Inline")
                .field("username", username)
                .field("password", &"<redacted>")
                .finish(),
            Self::File {
                username,
                password_file,
            } => f
                .debug_struct("FrigateApiAuthSource::File")
                .field("username", username)
                .field("password_file", password_file)
                .finish(),
        }
    }
}

impl<'de> Deserialize<'de> for FrigateApiAuthSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // - The wire shape is a product: username plus two optional secrets.
        // - It is private and collapsed to the sum right here, so the two illegal
        //   combinations are rejected at parse time with a precise message.
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            username: String,
            password: Option<String>,
            password_file: Option<PathBuf>,
        }

        let raw = Raw::deserialize(deserializer)?;
        match (raw.password, raw.password_file) {
            (Some(password), None) => Ok(Self::Inline {
                username: raw.username,
                password,
            }),
            (None, Some(password_file)) => Ok(Self::File {
                username: raw.username,
                password_file,
            }),
            (Some(_), Some(_)) => Err(D::Error::custom(
                "`frigate_api_auth`: set only one of `password` or `password_file`",
            )),
            (None, None) => Err(D::Error::custom(
                "`frigate_api_auth`: one of `password` or `password_file` is required",
            )),
        }
    }
}

impl FrigateApiAuthSource {
    // - Resolves the chosen source into concrete credentials.
    // - Only the File variant performs I/O: it reads and trims the password file.
    // - Takes &self (not self) because the type zeroizes on drop and so cannot be
    //   moved out of; the secret is cloned into the result, which zeroizes in turn.
    pub fn resolve(&self) -> Result<FrigateApiAuthConfig, ConfigError> {
        match self {
            Self::Inline { username, password } => Ok(FrigateApiAuthConfig {
                username: username.clone(),
                password: password.clone(),
            }),
            Self::File {
                username,
                password_file,
            } => {
                let password = std::fs::read_to_string(password_file)
                    .map_err(|e| {
                        ConfigError::FrigateApiPasswordFileCouldNotBeRead(password_file.clone(), e)
                    })?
                    .trim()
                    .to_string();
                Ok(FrigateApiAuthConfig {
                    username: username.clone(),
                    password,
                })
            }
        }
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

    #[test]
    fn auth_deserializes_inline() {
        let source: FrigateApiAuthSource = serde_yml::from_str("username: u\npassword: p").unwrap();
        assert_eq!(
            source,
            FrigateApiAuthSource::Inline {
                username: "u".to_string(),
                password: "p".to_string(),
            }
        );
    }

    #[test]
    fn auth_deserializes_file() {
        let source: FrigateApiAuthSource =
            serde_yml::from_str("username: u\npassword_file: /secret").unwrap();
        assert_eq!(
            source,
            FrigateApiAuthSource::File {
                username: "u".to_string(),
                password_file: PathBuf::from("/secret"),
            }
        );
    }

    #[test]
    fn auth_rejects_both_secrets() {
        let result: Result<FrigateApiAuthSource, _> =
            serde_yml::from_str("username: u\npassword: p\npassword_file: /secret");
        assert!(result.is_err());
    }

    #[test]
    fn auth_rejects_no_secret() {
        let result: Result<FrigateApiAuthSource, _> = serde_yml::from_str("username: u");
        assert!(result.is_err());
    }

    #[test]
    fn auth_requires_username() {
        let result: Result<FrigateApiAuthSource, _> = serde_yml::from_str("password: p");
        assert!(result.is_err());
    }

    #[test]
    fn auth_rejects_unknown_field() {
        let result: Result<FrigateApiAuthSource, _> =
            serde_yml::from_str("username: u\npassword: p\nbogus: x");
        assert!(result.is_err());
    }

    #[test]
    fn resolve_inline_passes_through() {
        let source = FrigateApiAuthSource::Inline {
            username: "u".to_string(),
            password: "p".to_string(),
        };
        assert_eq!(
            source.resolve().unwrap(),
            FrigateApiAuthConfig {
                username: "u".to_string(),
                password: "p".to_string(),
            }
        );
    }

    #[test]
    fn resolve_file_reads_and_trims() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("frigate_pw");
        std::fs::write(&path, "  secret-pass\n").unwrap();

        let source = FrigateApiAuthSource::File {
            username: "u".to_string(),
            password_file: path,
        };
        assert_eq!(
            source.resolve().unwrap(),
            FrigateApiAuthConfig {
                username: "u".to_string(),
                password: "secret-pass".to_string(),
            }
        );
    }
}
