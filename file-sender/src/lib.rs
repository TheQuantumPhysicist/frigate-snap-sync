use std::path::Path;

use store_local::LocalStore;
use store_sftp::{SftpError, SftpImpl};
use traits::StoreDestination;

mod store_local;
mod store_sftp;
pub mod traits;

#[must_use]
pub fn make_local_store(
    destination_dir: impl AsRef<Path>,
) -> Box<dyn StoreDestination<Error = std::io::Error>> {
    Box::new(LocalStore::new(destination_dir))
}

#[must_use]
pub fn make_sftp_store(
    host: &str,
    username: &str,
    priv_key_path: &Path,
) -> Box<dyn StoreDestination<Error = SftpError>> {
    Box::new(
        SftpImpl::new_with_public_key(host, username, &priv_key_path)
            .expect("sftp session initialization failed"),
    )
}
