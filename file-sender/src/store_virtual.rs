use crate::{path_descriptor::PathDescriptor, traits::StoreDestination};
use anyhow::Context;
use async_trait::async_trait;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub struct InMemoryFileSystem {
    root: vfs::VfsPath,
    path_descriptor: Arc<PathDescriptor>,
}

impl InMemoryFileSystem {
    pub fn new(path_descriptor: Arc<PathDescriptor>) -> Self {
        let fs = vfs::MemoryFS::default();
        Self {
            root: vfs::VfsPath::new(fs),
            path_descriptor,
        }
    }
}

fn path_as_str(path: &Path) -> String {
    path.to_str()
        .unwrap_or_else(|| panic!("Failed to convert path `{}` to string", path.display()))
        .to_string()
}

#[async_trait]
impl StoreDestination for InMemoryFileSystem {
    type Error = anyhow::Error;

    async fn init(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn ls(&self, path: &Path) -> Result<Vec<PathBuf>, Self::Error> {
        let path = path_as_str(path);
        let path = self.root.join(path).context("path join failed")?;
        let dir_read = path.read_dir().context("Read dir")?;

        let entries = dir_read.map(|v| v.filename().into()).collect::<Vec<_>>();

        Ok(entries)
    }

    async fn del_file(&self, path: &Path) -> Result<(), Self::Error> {
        let path = path_as_str(path);
        let path = self.root.join(path).context("path join failed")?;
        path.remove_file().context("del file")
    }

    async fn mkdir_p(&self, path: &Path) -> Result<(), Self::Error> {
        let path = path_as_str(path);
        let path = self.root.join(path).context("path join failed")?;
        path.create_dir_all().context("create_dir_all failed")
    }

    async fn put(&self, from: &Path, to: &Path) -> Result<(), Self::Error> {
        let data = std::fs::read(from).context("Reading local file in put")?;
        self.put_from_memory(&data, to)
            .await
            .context("Put in memory called from put")
    }

    async fn put_from_memory(&self, from: &[u8], to: &Path) -> Result<(), Self::Error> {
        let to = path_as_str(to);
        let to = self.root.join(to).context("path join failed")?;

        to.create_file()
            .context("create_file")?
            .write_all(from)
            .context("write_all")
    }

    async fn get_to_memory(&self, from: &Path) -> Result<Vec<u8>, Self::Error> {
        tracing::debug!("Calling 'get_to_memory' on path: `{}`", from.display());
        let from = path_as_str(from);
        let from = self.root.join(from).context("path join failed")?;

        let mut reader = from.open_file().context("Opening file")?;
        let mut result = Vec::new();
        reader
            .read_to_end(&mut result)
            .context("Read in get_to_memory")?;
        Ok(result)
    }

    async fn dir_exists(&self, path: &Path) -> Result<bool, Self::Error> {
        let path = path_as_str(path);
        let path = self.root.join(path).context("path join failed")?;

        path.is_dir().context("is_dir")
    }

    async fn file_exists(&self, path: &Path) -> Result<bool, Self::Error> {
        let path = path_as_str(path);
        let path = self.root.join(path).context("path join failed")?;

        path.is_file().context("is_file")
    }

    fn path_descriptor(&self) -> Arc<PathDescriptor> {
        self.path_descriptor.clone()
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn basic() {}
}
