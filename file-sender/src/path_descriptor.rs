use std::{fmt::Display, path::PathBuf, str::FromStr};

const LOCAL_PREFIX: &str = "local";
const SFTP_PREFIX: &str = "sftp";

/// Defines a destination to which an upload will be made
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathDescriptor {
    Local(PathBuf),
    Sftp {
        username: String,
        remote_address: String,
        remote_path: String,
        identity: PathBuf,
    },
}

impl Display for PathDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            PathDescriptor::Local(p) => format!("{LOCAL_PREFIX}:{}", p.display()),
            PathDescriptor::Sftp {
                username,
                remote_address,
                remote_path,
                identity,
            } => format!(
                "{SFTP_PREFIX}:{username}@{remote_address}:{remote_path}?identity={}",
                identity.display()
            ),
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
            // Format: local:/home/user/something.txt
            LOCAL_PREFIX => Ok(PathDescriptor::Local(dest_data.into())),

            // Format: sftp:user@example.com:/home/user2/something_else.txt?identity=/home/user/key.pem
            SFTP_PREFIX => {
                let (user_host, path_query) = dest_data.split_once(':').ok_or(anyhow::anyhow!(
                    "sftp path descriptor does not seem to start with a username@host before ':'"
                ))?;

                let (user, address) = user_host.split_once('@').ok_or(anyhow::anyhow!(
                    "sftp path descriptor does not seem to contain a username before '@'"
                ))?;

                let (path, query) = path_query.split_once('?').ok_or(anyhow::anyhow!(
                    "sftp path descriptor does not seem to contain a query (for identity, at least) specified after '?'"
                ))?;

                let parsed_query = parse_query(query).ok_or(anyhow::anyhow!(
                    "sftp descriptor failed to parse query after the '?'; queries are expected to be written in the form `?property1=value1&property2=value2`, etc."
                ))?;

                // A query entry with identity must exist
                let identity = parsed_query.get("identity").ok_or(anyhow::anyhow!(
                    "Could not find value for identity in the sftp query after '?'"
                ))?;

                Ok(PathDescriptor::Sftp {
                    username: user.to_string(),
                    remote_address: address.to_string(),
                    remote_path: path.to_string(),
                    identity: identity.into(),
                })
            }

            _ => Err(anyhow::anyhow!(
                "Unknown path descriptor prefix used: `dest_type`"
            )),
        }
    }
}

fn parse_query(query: &str) -> Option<std::collections::HashMap<String, String>> {
    let mut map = std::collections::HashMap::new();

    for pair in query.split('&') {
        let (k, v) = pair.split_once('=')?;
        let k = k.trim();
        let v = v.trim();
        if k.is_empty() || v.is_empty() {
            return None;
        }
        map.insert(k.to_string(), v.to_string());
    }

    Some(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_descriptor_parser() {
        {
            let d = PathDescriptor::from_str("local:/home/user/something.txt").unwrap();
            assert_eq!(d, PathDescriptor::Local("/home/user/something.txt".into()));
        }

        {
            let d = PathDescriptor::from_str(
                "sftp:user@example.com:/home/user2/something_else.txt?identity=/home/user/key.pem",
            )
            .unwrap();
            assert_eq!(
                d,
                PathDescriptor::Sftp {
                    username: "user".to_string(),
                    remote_address: "example.com".to_string(),
                    remote_path: "/home/user2/something_else.txt".to_string(),
                    identity: "/home/user/key.pem".into()
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
            let s = "local:/home/user/something.txt";
            let d = PathDescriptor::from_str(s).unwrap();
            assert_eq!(d, PathDescriptor::Local("/home/user/something.txt".into()));
            assert_eq!(d.to_string(), s);
        }

        {
            let s =
                "sftp:user@example.com:/home/user2/something_else.txt?identity=/home/user/key.pem";
            let d = PathDescriptor::from_str(s).unwrap();
            assert_eq!(
                d,
                PathDescriptor::Sftp {
                    username: "user".to_string(),
                    remote_address: "example.com".to_string(),
                    remote_path: "/home/user2/something_else.txt".to_string(),
                    identity: "/home/user/key.pem".into()
                }
            );
            assert_eq!(d.to_string(), s);
        }
    }
}
