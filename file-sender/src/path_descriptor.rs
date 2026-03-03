use crate::store_sftp::SftpError;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Display,
    path::PathBuf,
    str::FromStr,
};

const LOCAL_PREFIX: &str = "local";
const SFTP_PREFIX: &str = "sftp";

const SFTP_KEY_USER: &str = "username";
const SFTP_KEY_HOST: &str = "host";
const SFTP_KEY_PATH: &str = "remote-path";
const SFTP_KEY_IDENTITY: &str = "identity";

const LOCAL_KEY_PATH: &str = "path";

const S3_PREFIX: &str = "s3";
const S3_KEY_BUCKET: &str = "bucket";
const S3_KEY_PATH: &str = "s3-path";
const S3_KEY_REGION: &str = "region"; // optional
const S3_KEY_ENDPOINT: &str = "endpoint"; // optional (for LocalStack)
const S3_KEY_CREDENTIALS_PATH: &str = "credentials-path";
const S3_KEY_CREDENTIALS_PROFILE: &str = "profile"; // profile in the credentials-file

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StringFileData {
    InMemory(String),
    OnDisk(std::path::PathBuf),
}

impl StringFileData {
    #[must_use]
    pub fn display(&self) -> impl std::fmt::Display + '_ {
        struct DisplayWrapper<'a>(&'a StringFileData);

        impl std::fmt::Display for DisplayWrapper<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.0 {
                    StringFileData::InMemory(_) => write!(f, "<in-memory>"),
                    StringFileData::OnDisk(path) => write!(f, "{}", path.display()),
                }
            }
        }

        DisplayWrapper(self)
    }

    pub fn from_path(p: impl Into<PathBuf>) -> Self {
        Self::OnDisk(p.into())
    }

    #[must_use]
    pub fn from_memory(d: String) -> Self {
        Self::InMemory(d)
    }

    /// Reads the data in the file into a string
    pub fn into_file_data(self) -> Result<String, SftpError> {
        match self {
            StringFileData::InMemory(data) => Ok(data),
            StringFileData::OnDisk(path_buf) => {
                if !path_buf.exists() {
                    return Err(SftpError::FileNotFound(path_buf.clone()));
                }

                let result = std::fs::read_to_string(path_buf).map_err(SftpError::FileReadError)?;
                Ok(result)
            }
        }
    }
}

/// Defines a destination to which an upload will be made
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathDescriptor {
    Local(PathBuf),
    Sftp {
        username: String,
        remote_address: String,
        remote_path: String,
        identity: StringFileData,
    },
    S3 {
        bucket: String,
        base_path: PathBuf, // normalized no leading '/', may end with '/'
        region: Option<String>,
        endpoint: Option<String>,
        credentials_path: StringFileData,
        credentials_profile: Option<String>, // default is [default] according AWS docs
    },
}

impl Display for PathDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            PathDescriptor::Local(p) => format!("{LOCAL_PREFIX}:{LOCAL_KEY_PATH}={}", p.display()),
            PathDescriptor::Sftp {
                username,
                remote_address,
                remote_path,
                identity,
            } => {
                format!(
                    "{SFTP_PREFIX}:{SFTP_KEY_USER}={username};{SFTP_KEY_HOST}={remote_address};{SFTP_KEY_PATH}={remote_path};{SFTP_KEY_IDENTITY}={}",
                    identity.display()
                )
            }
            PathDescriptor::S3 {
                bucket,
                base_path,
                region,
                endpoint,
                credentials_path,
                credentials_profile,
            } => {
                let bucket = bucket.clone();
                let base_path = base_path.display().to_string();
                let region = region
                    .as_ref()
                    .map(|r| format!(";{S3_KEY_REGION}={r}"))
                    .unwrap_or_default();
                let endpoint = endpoint
                    .as_ref()
                    .map(|ep| format!(";{S3_KEY_ENDPOINT}={ep}"))
                    .unwrap_or_default();
                let credentials_path = credentials_path.display().to_string();
                let profile = credentials_profile
                    .as_ref()
                    .map(|p| format!(";{S3_KEY_CREDENTIALS_PROFILE}={p}"))
                    .unwrap_or_default();

                format!(
                    "{S3_PREFIX}:{S3_KEY_BUCKET}={bucket};{S3_KEY_PATH}={base_path}{region}{endpoint};{S3_KEY_CREDENTIALS_PATH}={credentials_path}{profile}",
                )
            }
        };
        s.fmt(f)
    }
}

impl FromStr for PathDescriptor {
    type Err = anyhow::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let (dest_type, dest_data) = input.split_once(':').ok_or(anyhow::anyhow!(
            "Path descriptor does not contain the path type before ':'"
        ))?;

        match dest_type.to_lowercase().as_str() {
            // Format: `local:path=/home/user/something.txt``
            LOCAL_PREFIX => {
                let key_vals = parse_key_vals_string(dest_data, dest_type, &[LOCAL_KEY_PATH], &[])?;
                let path = key_vals
                    .get(LOCAL_KEY_PATH)
                    .expect("Must exist since verified in parser");
                Ok(PathDescriptor::Local(path.into()))
            }

            // Format: sftp:username=<username>;host=example.com;port=22;remote-path=/home/user2/something_else;identity=/home/user/key.pem
            SFTP_PREFIX => {
                const ERR: &str = "Must exist from parser";

                let key_vals = parse_key_vals_string(
                    dest_data,
                    dest_type,
                    &[
                        SFTP_KEY_USER,
                        SFTP_KEY_HOST,
                        SFTP_KEY_PATH,
                        SFTP_KEY_IDENTITY,
                    ],
                    &[],
                )?;

                let username = key_vals.get(SFTP_KEY_USER).expect(ERR);
                let host = key_vals.get(SFTP_KEY_HOST).expect(ERR);
                let remote_path = key_vals.get(SFTP_KEY_PATH).expect(ERR);
                let identity = key_vals.get(SFTP_KEY_IDENTITY).expect(ERR);

                // Check valid port
                if let Some((_host, port)) = host.split_once(':') {
                    let _port = port
                        .parse::<u16>()
                        .map_err(|_| anyhow::anyhow!("Failed to parse port: `{port}`"))?;
                }

                // A query entry with identity must exist
                Ok(PathDescriptor::Sftp {
                    username: username.clone(),
                    remote_address: host.clone(),
                    remote_path: remote_path.clone(),
                    identity: StringFileData::OnDisk(identity.into()),
                })
            }

            S3_PREFIX => {
                let map = parse_key_vals_string(
                    dest_data,
                    S3_PREFIX,
                    &[S3_KEY_BUCKET, S3_KEY_PATH, S3_KEY_CREDENTIALS_PATH],
                    &[S3_KEY_REGION, S3_KEY_ENDPOINT, S3_KEY_CREDENTIALS_PROFILE],
                )?;

                let bucket = map
                    .get(S3_KEY_BUCKET)
                    .ok_or_else(|| anyhow::anyhow!("missing {S3_KEY_BUCKET}"))?
                    .clone();
                let path = map
                    .get(S3_KEY_PATH)
                    .ok_or_else(|| anyhow::anyhow!("missing {S3_KEY_PATH}"))?
                    .clone();
                let credentials_path = map
                    .get(S3_KEY_CREDENTIALS_PATH)
                    .ok_or_else(|| anyhow::anyhow!("missing {S3_KEY_CREDENTIALS_PATH}"))?
                    .clone();

                let region = map.get(S3_KEY_REGION).cloned();
                let endpoint = map.get(S3_KEY_ENDPOINT).cloned();

                let mut base_path = PathBuf::from(path);
                if !base_path.as_os_str().is_empty()
                    && !base_path.as_os_str().to_string_lossy().ends_with('/')
                {
                    base_path.push("");
                }
                let credentials_profile = map.get(S3_KEY_CREDENTIALS_PROFILE).cloned();

                Ok(PathDescriptor::S3 {
                    bucket,
                    base_path,
                    region,
                    endpoint,
                    credentials_path: StringFileData::OnDisk(credentials_path.into()),
                    credentials_profile,
                })
            }

            _ => Err(anyhow::anyhow!(
                "Unknown path descriptor prefix used: `dest_type`"
            )),
        }
    }
}

fn parse_key_vals_string(
    input: &str,
    describing_what: &str,
    required_keys: &[&str],
    optional_keys: &[&str],
) -> anyhow::Result<BTreeMap<String, String>> {
    let mut result_map = BTreeMap::new();

    let allowed_keys: BTreeSet<_> = required_keys.iter().chain(optional_keys).copied().collect();

    for part in input.split(';') {
        let part = part.trim();
        let (key, value) = part
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("Invalid format. Expected key=value. Found: {part}"))?;

        if !key.is_ascii() {
            return Err(anyhow::anyhow!(
                "Keys for path descriptor must be ascii. Found invalid key: `{key}`"
            ));
        }

        let key = key.to_lowercase();

        if result_map.contains_key(&key) {
            return Err(anyhow::anyhow!("Duplicate key: {part}"));
        }

        if !allowed_keys.contains(key.as_str()) {
            return Err(anyhow::anyhow!(
                "Unexpected key for descriptor `{describing_what}`. Key: {key}"
            ));
        }

        result_map.insert(key.clone(), value.to_string());
    }

    for &key in required_keys {
        if !result_map.contains_key(key) {
            return Err(anyhow::anyhow!(
                "Required key `{key}` for descriptor `{describing_what}` not found."
            ));
        }
    }

    Ok(result_map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_utils::assert_str_contains;

    #[test]
    fn path_descriptor_parser() {
        {
            let d = PathDescriptor::from_str("local:path=/home/user/something.txt").unwrap();
            assert_eq!(d, PathDescriptor::Local("/home/user/something.txt".into()));
        }

        {
            let d = PathDescriptor::from_str(
                "sftp:username=user;host=example.com;remote-path=/home/user2/something_else.txt;identity=/home/user/key.pem",
            )
            .unwrap();
            assert_eq!(
                d,
                PathDescriptor::Sftp {
                    username: "user".to_string(),
                    remote_address: "example.com".to_string(),
                    remote_path: "/home/user2/something_else.txt".to_string(),
                    identity: StringFileData::OnDisk("/home/user/key.pem".into()),
                }
            );
        }

        // With non-default port
        {
            let d = PathDescriptor::from_str(
                "sftp:username=user;host=example.com:8888;remote-path=/home/user2/something_else.txt;identity=/home/user/key.pem",
            )
            .unwrap();
            assert_eq!(
                d,
                PathDescriptor::Sftp {
                    username: "user".to_string(),
                    remote_address: "example.com:8888".to_string(),
                    remote_path: "/home/user2/something_else.txt".to_string(),
                    identity: StringFileData::OnDisk("/home/user/key.pem".into()),
                }
            );
        }

        assert!(
            PathDescriptor::from_str(
                "sftp:user@example.com:/home/user2/something_else.txt?xyz=/home/user/key.pem"
            )
            .is_err()
        );
        assert!(
            PathDescriptor::from_str("sftp:user@example.com:/home/user2/something_else.txt")
                .is_err()
        );
        assert!(
            PathDescriptor::from_str(
                "sftp:user:/home/user2/something_else.txt?identity=/home/user/key.pem"
            )
            .is_err()
        );
        assert!(PathDescriptor::from_str("abc:/home/user").is_err());
        assert!(PathDescriptor::from_str("/home/user").is_err());
    }

    #[test]
    fn s3_path_descriptor_parse_and_display() {
        let s = format!(
            "s3:bucket=my-bucket;s3-path=sync/;region=us-east-1;endpoint=http://127.0.0.1:4566;{S3_KEY_CREDENTIALS_PATH}=/tmp/aws-creds;profile=my-profile"
        );
        let d = PathDescriptor::from_str(&s).unwrap();
        assert_eq!(
            d,
            PathDescriptor::S3 {
                bucket: "my-bucket".into(),
                base_path: PathBuf::from("sync/"),
                region: Some("us-east-1".into()),
                endpoint: Some("http://127.0.0.1:4566".into()),
                credentials_path: StringFileData::OnDisk("/tmp/aws-creds".into()),
                credentials_profile: Some("my-profile".to_string())
            }
        );
        let serialized = d.to_string();
        assert!(serialized.contains("bucket=my-bucket"));
        assert!(serialized.contains("s3-path=sync/"));
        assert!(serialized.contains(&format!("{S3_KEY_CREDENTIALS_PATH}=/tmp/aws-creds")));
        assert!(serialized.contains("profile=my-profile"));
    }

    #[test]
    fn s3_profile_omitted_when_none() {
        let s = format!(
            "s3:bucket=b;s3-path=sync/;region=us-east-1;{S3_KEY_CREDENTIALS_PATH}=/tmp/creds"
        );
        let d = PathDescriptor::from_str(&s).unwrap();

        // Parsed shape
        if let PathDescriptor::S3 {
            credentials_profile,
            ..
        } = &d
        {
            assert!(credentials_profile.is_none());
        } else {
            panic!("expected S3 descriptor");
        }

        // Serialized form should NOT include profile=
        let serialized = d.to_string();
        assert!(!serialized.contains("profile="));
    }

    #[test]
    fn s3_base_path_normalized_trailing_slash() {
        let d = PathDescriptor::from_str(&format!(
            "s3:bucket=b;s3-path=sync;{S3_KEY_CREDENTIALS_PATH}=/tmp/creds"
        ))
        .unwrap();
        if let PathDescriptor::S3 { base_path, .. } = d {
            assert_eq!(base_path, PathBuf::from("sync/"));
        }
    }

    #[test]
    fn path_descriptor_parse_back_and_forth() {
        {
            let s = "local:path=/home/user/something.txt";
            let d = PathDescriptor::from_str(s).unwrap();
            assert_eq!(d, PathDescriptor::Local("/home/user/something.txt".into()));
            assert_eq!(d.to_string(), s);
        }

        {
            let s = "sftp:username=user;host=example.com;remote-path=/home/user2/something_else.txt;identity=/home/user/key.pem";
            let d = PathDescriptor::from_str(s).unwrap();
            assert_eq!(
                d,
                PathDescriptor::Sftp {
                    username: "user".to_string(),
                    remote_address: "example.com".to_string(),
                    remote_path: "/home/user2/something_else.txt".to_string(),
                    identity: StringFileData::OnDisk("/home/user/key.pem".into()),
                }
            );
            {
                let serialized = d.to_string();
                assert!(serialized.contains(&format!("{SFTP_KEY_USER}=user")));
                assert!(serialized.contains(&format!("{SFTP_KEY_HOST}=example.com")));
                assert!(
                    serialized.contains(&format!("{SFTP_KEY_PATH}=/home/user2/something_else.txt"))
                );
                assert!(serialized.contains(&format!("{SFTP_KEY_IDENTITY}=/home/user/key.pem")));
                let to_parse = serialized.strip_prefix("sftp:").unwrap();
                parse_key_vals_string(
                    to_parse,
                    "sftp",
                    &[
                        SFTP_KEY_USER,
                        SFTP_KEY_HOST,
                        SFTP_KEY_PATH,
                        SFTP_KEY_IDENTITY,
                    ],
                    &[],
                )
                .unwrap();
            }
        }

        // With non-default port
        {
            let s = "sftp:username=user;host=example.com:8822;remote-path=/home/user2/something_else.txt;identity=/home/user/key.pem";
            let d = PathDescriptor::from_str(s).unwrap();
            assert_eq!(
                d,
                PathDescriptor::Sftp {
                    username: "user".to_string(),
                    remote_address: "example.com:8822".to_string(),
                    remote_path: "/home/user2/something_else.txt".to_string(),
                    identity: StringFileData::OnDisk("/home/user/key.pem".into()),
                }
            );
            {
                let serialized = d.to_string();
                assert!(serialized.contains(&format!("{SFTP_KEY_USER}=user")));
                assert!(serialized.contains(&format!("{SFTP_KEY_HOST}=example.com:8822")));
                assert!(
                    serialized.contains(&format!("{SFTP_KEY_PATH}=/home/user2/something_else.txt"))
                );
                assert!(serialized.contains(&format!("{SFTP_KEY_IDENTITY}=/home/user/key.pem")));
                let to_parse = serialized.strip_prefix("sftp:").unwrap();
                parse_key_vals_string(
                    to_parse,
                    "sftp",
                    &[
                        SFTP_KEY_USER,
                        SFTP_KEY_HOST,
                        SFTP_KEY_PATH,
                        SFTP_KEY_IDENTITY,
                    ],
                    &[],
                )
                .unwrap();
            }
        }
    }

    #[test]
    fn key_value_parse_valid_input() {
        let input = "name=john;age=30";
        let required_keys = ["name"];
        let optional_keys = ["age"];

        let expected_map: BTreeMap<String, String> = [
            ("name".to_string(), "john".to_string()),
            ("age".to_string(), "30".to_string()),
        ]
        .into();

        assert_eq!(
            parse_key_vals_string(input, "test", &required_keys, &optional_keys).unwrap(),
            expected_map
        );
    }

    #[test]
    fn key_value_missing_required_key() {
        let input = "age=30";
        let required_keys = ["name"];
        let optional_keys = ["age"];

        assert_str_contains!(
            &parse_key_vals_string(input, "test", &required_keys, &optional_keys)
                .unwrap_err()
                .to_string(),
            "Required key",
        );
    }

    #[test]
    fn key_value_invalid_format() {
        let input = "invalid_part";
        let required_keys = [];
        let optional_keys = [];

        assert_str_contains!(
            &parse_key_vals_string(input, "test", &required_keys as &[&str], &optional_keys)
                .unwrap_err()
                .to_string(),
            "Invalid format. Expected key=value",
        );
    }

    #[test]
    fn key_value_duplicate_key() {
        let input = "name=john;Name=doe";
        let required_keys = ["name"];
        let optional_keys = [];

        assert_str_contains!(
            &parse_key_vals_string(input, "test", &required_keys as &[&str], &optional_keys)
                .unwrap_err()
                .to_string(),
            "Duplicate key:",
        );
    }

    #[test]
    fn key_value_non_ascii_key() {
        let input = "number=juan;näm=Sam";
        let required_keys = ["number"];
        let optional_keys = ["näm"];

        assert_str_contains!(
            &parse_key_vals_string(input, "test", &required_keys as &[&str], &optional_keys)
                .unwrap_err()
                .to_string(),
            "Keys for path descriptor must be ascii",
        );
    }

    #[test]
    fn key_value_unknown_key() {
        let input = "unknown=value";
        let required_keys = [];
        let optional_keys = [];

        assert_str_contains!(
            &parse_key_vals_string(input, "test", &required_keys, &optional_keys)
                .unwrap_err()
                .to_string(),
            "Unexpected key for descriptor `test`",
        );
    }
}
