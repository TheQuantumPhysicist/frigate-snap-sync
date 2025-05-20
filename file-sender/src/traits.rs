use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;

use crate::path_descriptor::PathDescriptor;

// TODO: make this async by using blocking ops in tokio

/// A representation of store location, remote possibly, where we data can be sent.
/// All the functions (docs) in this trait assume that we're dealing with a remote system.
/// However, this also applies to local systems.
#[async_trait]
pub trait StoreDestination: Send + Sync {
    type Error;

    /// Any initialization needed for the destination.
    async fn init(&self) -> Result<(), Self::Error>;

    /// List the available files at the given remote path
    async fn ls(&self, path: &Path) -> Result<Vec<PathBuf>, Self::Error>;

    /// Delete the file at the given remote path
    async fn del_file(&self, path: &Path) -> Result<(), Self::Error>;

    /// Create a directory at the given remote path, recursively
    async fn mkdir_p(&self, path: &Path) -> Result<(), Self::Error>;

    /// Copy the file `from` the given LOCAL PATH, `to` the given remote path
    async fn put(&self, from: &Path, to: &Path) -> Result<(), Self::Error>;

    /// Copy the given raw data in `from` to the given remote path in `to`.
    async fn put_from_memory(&self, from: &[u8], to: &Path) -> Result<(), Self::Error>;

    /// Reads a given remote file `from` the given path and returns it in the result
    async fn get_to_memory(&self, from: &Path) -> Result<Vec<u8>, Self::Error>;

    /// Returns true if the given path is a directory, and exists
    async fn dir_exists(&self, path: &Path) -> Result<bool, Self::Error>;

    /// Returns true if the given path is a file, and exists
    async fn file_exists(&self, path: &Path) -> Result<bool, Self::Error>;

    /// Returns a local copy of the PathDescriptor object. This is done primarily to simplify some processes.
    fn path_descriptor(&self) -> &Arc<PathDescriptor>;
}
