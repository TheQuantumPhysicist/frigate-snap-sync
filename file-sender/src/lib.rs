pub mod path_descriptor;
mod store_local;
mod store_sftp;
pub mod traits;

use path_descriptor::PathDescriptor;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use store_local::LocalStore;
use store_sftp::SftpImpl;
use traits::StoreDestination;

pub fn make_store(
    path_descriptor: &Arc<PathDescriptor>,
) -> anyhow::Result<Box<dyn StoreDestination<Error = anyhow::Error>>> {
    match path_descriptor.as_ref() {
        PathDescriptor::Local(p) => make_local_store(path_descriptor.clone(), p),
        PathDescriptor::Sftp {
            username,
            remote_address,
            remote_path,
            identity,
        } => make_sftp_store(
            path_descriptor.clone(),
            remote_address,
            username,
            identity,
            remote_path,
        ),
    }
}

fn make_local_store(
    path_descriptor: Arc<PathDescriptor>,
    destination_dir: impl AsRef<Path>,
) -> anyhow::Result<Box<dyn StoreDestination<Error = anyhow::Error>>> {
    let store = LocalStore::new(path_descriptor, destination_dir)?;
    Ok(Box::new(store))
}

fn make_sftp_store(
    path_descriptor: Arc<PathDescriptor>,
    host: &str,
    username: &str,
    priv_key_path: impl AsRef<Path>,
    destination_path: impl Into<PathBuf>,
) -> anyhow::Result<Box<dyn StoreDestination<Error = anyhow::Error>>> {
    let sftp = SftpImpl::new_with_public_key(
        path_descriptor,
        host,
        username,
        &priv_key_path,
        destination_path,
    )?;

    Ok(Box::new(sftp))
}
