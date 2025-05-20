pub mod path_descriptor;
mod store_local;
mod store_sftp;
mod store_virtual;
pub mod traits;

use path_descriptor::{IdentitySource, PathDescriptor};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use store_local::LocalStore;
use store_sftp::AsyncSftpImpl;
use store_virtual::InMemoryFileSystem;
use traits::StoreDestination;

pub fn make_store(
    path_descriptor: &Arc<PathDescriptor>,
) -> anyhow::Result<Arc<dyn StoreDestination<Error = anyhow::Error>>> {
    match path_descriptor.as_ref() {
        PathDescriptor::Local(p) => Ok(make_local_store(path_descriptor.clone(), p)),
        PathDescriptor::Sftp {
            username,
            remote_address,
            remote_path,
            identity,
        } => make_sftp_store(
            path_descriptor.clone(),
            remote_address,
            username,
            identity.clone(),
            remote_path,
        ),
    }
}

fn make_local_store(
    path_descriptor: Arc<PathDescriptor>,
    destination_dir: impl AsRef<Path>,
) -> Arc<dyn StoreDestination<Error = anyhow::Error>> {
    let store = LocalStore::new(path_descriptor, destination_dir);
    Arc::new(store)
}

fn make_sftp_store(
    path_descriptor: Arc<PathDescriptor>,
    host: &str,
    username: &str,
    priv_key_path: IdentitySource,
    destination_path: impl Into<PathBuf>,
) -> anyhow::Result<Arc<dyn StoreDestination<Error = anyhow::Error>>> {
    let sftp = AsyncSftpImpl::new_with_public_key(
        path_descriptor,
        host,
        username,
        priv_key_path,
        destination_path,
    )?;

    Ok(Arc::new(sftp))
}

#[must_use]
pub fn make_inmemory_filesystem() -> Arc<dyn StoreDestination<Error = anyhow::Error>> {
    Arc::new(InMemoryFileSystem::new(Arc::new(PathDescriptor::Local(
        String::new().into(),
    ))))
}

#[cfg(test)]
mod tests;
