use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;

use crate::path_descriptor::PathDescriptor;

// TODO: make this async by using blocking ops in tokio+
// TODO: consider moving sftp to https://crates.io/crates/russh to use async

#[async_trait]
pub trait StoreDestination: Send + Sync {
    type Error;

    async fn ls(&self, path: &Path) -> Result<Vec<PathBuf>, Self::Error>;
    async fn del_file(&self, path: &Path) -> Result<(), Self::Error>;
    async fn mkdir_p(&self, path: &Path) -> Result<(), Self::Error>;
    async fn put(&self, from: &Path, to: &Path) -> Result<(), Self::Error>;
    async fn put_from_memory(&self, from: &[u8], to: &Path) -> Result<(), Self::Error>;
    async fn dir_exists(&self, path: &Path) -> Result<bool, Self::Error>;
    async fn file_exists(&self, path: &Path) -> Result<bool, Self::Error>;
    fn path_descriptor(&self) -> &Arc<PathDescriptor>;
}
