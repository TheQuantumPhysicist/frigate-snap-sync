use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::path_descriptor::PathDescriptor;

// TODO: make this async by using blocking ops in tokio+
// TODO: consider moving sftp to https://crates.io/crates/russh to use async

pub trait StoreDestination {
    type Error;

    fn ls(&self, path: &Path) -> Result<Vec<PathBuf>, Self::Error>;
    fn del_file(&self, path: &Path) -> Result<(), Self::Error>;
    fn mkdir_p(&self, path: &Path) -> Result<(), Self::Error>;
    fn put(&self, from: &Path, to: &Path) -> Result<(), Self::Error>;
    fn put_from_memory(&self, from: &[u8], to: &Path) -> Result<(), Self::Error>;
    fn dir_exists(&self, path: &Path) -> Result<bool, Self::Error>;
    fn file_exists(&self, path: &Path) -> Result<bool, Self::Error>;
    fn path_descriptor(&self) -> &Arc<PathDescriptor>;
}
