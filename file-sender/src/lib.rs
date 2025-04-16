use std::path::{Path, PathBuf};

use path_descriptor::{PathDescriptor, parse_path};
use store_local::LocalStore;
use store_sftp::SftpImpl;
use traits::StoreDestination;

mod path_descriptor;
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
            PathDescriptor::Sftp {
                username: user,
                remote_address: address,
                remote_path: path,
                identity,
            } => make_sftp_store(&address, &user, &identity, path),
        };
        Some(res)
    } else {
        None
    }
}
