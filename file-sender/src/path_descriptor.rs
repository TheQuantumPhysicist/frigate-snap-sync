use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
pub enum PathDescriptor {
    Local(String),
    Sftp {
        username: String,
        remote_address: String,
        remote_path: String,
        identity: PathBuf,
    },
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

pub fn parse_path(input: &str) -> Option<PathDescriptor> {
    let (dest_type, dest_data) = input.split_once(':')?;
    match dest_type {
        // Format: local:/home/user/something.txt
        "local" => return Some(PathDescriptor::Local(dest_data.to_string())),
        // Format: sftp:user@example.com:/home/user2/something_else.txt?identity=/home/user/key.pem
        "sftp" => {
            if let Some((user_host, path_query)) = dest_data.split_once(':') {
                if let Some((user, address)) = user_host.split_once('@') {
                    if let Some((path, query)) = path_query.split_once('?') {
                        let parsed_query = parse_query(query)?;

                        // A query entry with identity must exist
                        if let Some(identity) = parsed_query.get("identity") {
                            return Some(PathDescriptor::Sftp {
                                username: user.to_string(),
                                remote_address: address.to_string(),
                                remote_path: path.to_string(),
                                identity: identity.into(),
                            });
                        }
                    }
                }
            }
        }
        _ => return None,
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_descriptor_parser() {
        let d = parse_path("local:/home/user/something.txt");
        assert_eq!(
            d,
            Some(PathDescriptor::Local(
                "/home/user/something.txt".to_string()
            ))
        );

        let d = parse_path(
            "sftp:user@example.com:/home/user2/something_else.txt?identity=/home/user/key.pem",
        );
        assert_eq!(
            d,
            Some(PathDescriptor::Sftp {
                username: "user".to_string(),
                remote_address: "example.com".to_string(),
                remote_path: "/home/user2/something_else.txt".to_string(),
                identity: "/home/user/key.pem".into()
            })
        );

        assert!(
            parse_path(
                "sftp:user@example.com:/home/user2/something_else.txt?xyz=/home/user/key.pem"
            )
            .is_none()
        );
        assert!(parse_path("sftp:user@example.com:/home/user2/something_else.txt").is_none());
        assert!(
            parse_path("sftp:user:/home/user2/something_else.txt?identity=/home/user/key.pem")
                .is_none()
        );
        assert!(parse_path("abc:/home/user").is_none());
        assert!(parse_path("/home/user").is_none());
    }
}
