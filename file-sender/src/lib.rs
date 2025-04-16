use std::path::{Path, PathBuf};

use store_local::LocalStore;
use store_sftp::SftpImpl;
use traits::StoreDestination;

mod store_local;
mod store_sftp;
pub mod traits;

#[must_use]
fn make_local_store(
    destination_dir: impl AsRef<Path>,
) -> Box<dyn StoreDestination<Error = anyhow::Error>> {
    Box::new(LocalStore::new(destination_dir))
}

#[must_use]
fn make_sftp_store(
    host: &str,
    username: &str,
    priv_key_path: impl AsRef<Path>,
    destination_path: impl Into<PathBuf>,
) -> Box<dyn StoreDestination<Error = anyhow::Error>> {
    Box::new(
        SftpImpl::new_with_public_key(host, username, &priv_key_path, destination_path)
            .expect("sftp session initialization failed"),
    )
}

#[must_use]
pub fn make_store<E: std::error::Error>(
    store: &str,
) -> Option<Box<dyn StoreDestination<Error = anyhow::Error>>> {
    if let Some(d) = parse_path(store) {
        let res = match d {
            PathDescriptor::Local(p) => make_local_store(p),
            PathDescriptor::Remote {
                user,
                address,
                path,
                identity,
            } => make_sftp_store(&address, &user, &identity, path),
        };
        Some(res)
    } else {
        None
    }
}

#[derive(Debug, PartialEq)]
enum PathDescriptor {
    Local(String),
    Remote {
        user: String,
        address: String,
        path: String,
        identity: PathBuf,
    },
}

fn parse_query(query: &str) -> Option<PathBuf> {
    query.split('&').find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        match (parts.next(), parts.next()) {
            (Some("identity"), Some(val)) => Some(PathBuf::from(val)),
            _ => None,
        }
    })
}

fn parse_path(input: &str) -> Option<PathDescriptor> {
    if let Some(rest) = input.strip_prefix("local:") {
        return Some(PathDescriptor::Local(rest.to_string()));
    } else if let Some((user_host, path_query)) = input.split_once(':') {
        if let Some((user, address)) = user_host.split_once('@') {
            if let Some((path, query)) = path_query.split_once('?') {
                if let Some(identity) = parse_query(query) {
                    return Some(PathDescriptor::Remote {
                        user: user.to_string(),
                        address: address.to_string(),
                        path: path.to_string(),
                        identity,
                    });
                }
            }
        }
    }

    None
}

// TODO: write PathDescriptor parsing tests
