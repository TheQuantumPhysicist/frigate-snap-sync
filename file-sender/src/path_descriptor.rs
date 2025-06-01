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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentitySource {
    InMemory(String),
    OnDisk(std::path::PathBuf),
}

impl IdentitySource {
    #[must_use]
    pub fn display(&self) -> impl std::fmt::Display + '_ {
        struct DisplayWrapper<'a>(&'a IdentitySource);

        impl std::fmt::Display for DisplayWrapper<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.0 {
                    IdentitySource::InMemory(_) => write!(f, "<in-memory>"),
                    IdentitySource::OnDisk(path) => write!(f, "{}", path.display()),
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

    pub fn into_key(self) -> Result<String, SftpError> {
        match self {
            IdentitySource::InMemory(data) => Ok(data),
            IdentitySource::OnDisk(path_buf) => {
                if !path_buf.exists() {
                    return Err(SftpError::PrivKeyNotFoundInPath(path_buf.clone()));
                }

                let result =
                    std::fs::read_to_string(path_buf).map_err(SftpError::PrivKeyReadError)?;
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
        identity: IdentitySource,
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
                    username: username.to_string(),
                    remote_address: host.to_string(),
                    remote_path: remote_path.to_string(),
                    identity: IdentitySource::OnDisk(identity.into()),
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
        let (key, value) = part.split_once('=').ok_or_else(|| {
            anyhow::anyhow!(
                "Invalid format. Expected key=value. Found: {}",
                part.to_string()
            )
        })?;

        if !key.is_ascii() {
            return Err(anyhow::anyhow!(
                "Keys for path descriptor must be ascii. Found invalid key: `{key}`"
            ));
        }

        let key = key.to_lowercase();

        if result_map.contains_key(&key) {
            return Err(anyhow::anyhow!("Duplicate key: {}", part.to_string()));
        }

        if !allowed_keys.contains(key.as_str()) {
            return Err(anyhow::anyhow!(
                "Unexpected key for descriptor `{describing_what}`. Key: {}",
                key.to_string()
            ));
        }

        result_map.insert(key.to_string(), value.to_string());
    }

    for &key in required_keys {
        if !result_map.contains_key(key) {
            return Err(anyhow::anyhow!(
                "Required key `{}` for descriptor `{describing_what}` not found.",
                key.to_string()
            ));
        }
    }

    Ok(result_map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_utils::asserts::assert_str_contains;

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
                    identity: IdentitySource::OnDisk("/home/user/key.pem".into()),
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
                    identity: IdentitySource::OnDisk("/home/user/key.pem".into()),
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
                    identity: IdentitySource::OnDisk("/home/user/key.pem".into()),
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
                    &to_parse,
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
                    identity: IdentitySource::OnDisk("/home/user/key.pem".into()),
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
                    &to_parse,
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

        assert_str_contains(
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

        assert_str_contains(
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

        assert_str_contains(
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

        assert_str_contains(
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

        assert_str_contains(
            &parse_key_vals_string(input, "test", &required_keys, &optional_keys)
                .unwrap_err()
                .to_string(),
            "Unexpected key for descriptor `test`",
        );
    }
}
