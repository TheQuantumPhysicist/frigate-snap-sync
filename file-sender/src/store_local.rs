use anyhow::Context;
use async_trait::async_trait;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;

use crate::path_descriptor::PathDescriptor;
use crate::traits::StoreDestination;
pub struct LocalStore {
    path_descriptor: Arc<PathDescriptor>,
    dest_dir: PathBuf,
}

impl LocalStore {
    pub fn new<P: AsRef<Path>>(path_descriptor: Arc<PathDescriptor>, dest_dir: P) -> Self {
        let dest_dir = dest_dir.as_ref();
        tracing::debug!("Creating local storage object in {}", dest_dir.display());

        Self {
            path_descriptor,
            dest_dir: dest_dir.to_path_buf(),
        }
    }

    fn resolve<P: AsRef<Path>>(&self, path: &P) -> PathBuf {
        self.dest_dir.join(path)
    }
}

#[async_trait]
impl StoreDestination for LocalStore {
    type Error = anyhow::Error;

    async fn init(&self) -> Result<(), Self::Error> {
        self.mkdir_p(self.dest_dir.as_ref()).await.context(format!(
            "(Re-)creating local directory: {}",
            self.dest_dir.display()
        ))?;

        Ok(())
    }

    async fn ls(&self, path: &Path) -> Result<Vec<PathBuf>, Self::Error> {
        let full_path = self.resolve(&path);
        tracing::debug!("Calling 'ls' on path: `{}`", full_path.display());

        let mut entries = Vec::new();

        let mut dir_read = fs::read_dir(full_path).await?;

        while let Some(entry) = dir_read.next_entry().await? {
            entries.push(entry.file_name().into());
        }

        Ok(entries)
    }

    async fn del_file(&self, path: &Path) -> Result<(), Self::Error> {
        let full_path = self.resolve(&path);
        tracing::debug!("Calling 'del_file' on path: `{}`", full_path.display());
        fs::remove_file(full_path).await.map_err(Into::into)
    }

    async fn mkdir_p(&self, path: &Path) -> Result<(), Self::Error> {
        fs::create_dir_all(self.resolve(&path))
            .await
            .map_err(Into::into)
    }

    async fn put(&self, from: &Path, to: &Path) -> Result<(), Self::Error> {
        let to_path = self.resolve(&to);
        tracing::debug!(
            "Calling 'put' from path `{}` to path: `{}`",
            from.display(),
            to_path.display()
        );
        fs::copy(from, to_path)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    async fn put_from_memory(&self, from: &[u8], to: &Path) -> Result<(), Self::Error> {
        let to_path = self.resolve(&to);
        tracing::debug!(
            "Calling 'put_from_memory' for memory data with size {} bytes to path: `{}`",
            from.len(),
            to_path.display()
        );
        Ok(fs::write(to_path, from).await?)
    }

    async fn get_to_memory(&self, from: &Path) -> Result<Vec<u8>, Self::Error> {
        let from_path = self.resolve(&from);
        tracing::debug!("Calling 'get_to_memory' on path: `{}`", from_path.display());
        let result = std::fs::read(from_path)?;
        Ok(result)
    }

    async fn dir_exists(&self, path: &Path) -> Result<bool, Self::Error> {
        let full_path = self.resolve(&path);
        tracing::debug!("Calling 'dir_exists' on path: `{}`", full_path.display());
        Ok(full_path.is_dir())
    }

    async fn file_exists(&self, path: &Path) -> Result<bool, Self::Error> {
        tracing::debug!("Calling 'file_exists' on path: `{}`", path.display());
        Ok(self.resolve(&path).is_file())
    }

    fn path_descriptor(&self) -> Arc<PathDescriptor> {
        self.path_descriptor.clone()
    }
}
