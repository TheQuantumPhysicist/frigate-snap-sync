use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;

use crate::path_descriptor::PathDescriptor;
use crate::traits::StoreDestination;
pub struct LocalStore {
    path_descriptor: Arc<PathDescriptor>,
    dest_dir: PathBuf,
}

impl LocalStore {
    pub fn new<P: AsRef<Path>>(
        path_descriptor: Arc<PathDescriptor>,
        dest_dir: P,
    ) -> anyhow::Result<Self> {
        let dest_dir = dest_dir.as_ref();
        tracing::debug!("Creating local storage object in {}", dest_dir.display());

        let res = Self {
            path_descriptor,
            dest_dir: dest_dir.to_path_buf(),
        };

        res.mkdir_p(dest_dir.as_ref()).context(format!(
            "(Re-)creating local directory: {}",
            dest_dir.display()
        ))?;

        Ok(res)
    }

    fn resolve<P: AsRef<Path>>(&self, path: &P) -> PathBuf {
        self.dest_dir.join(path)
    }
}

impl StoreDestination for LocalStore {
    type Error = anyhow::Error;

    fn ls(&self, path: &Path) -> Result<Vec<PathBuf>, Self::Error> {
        let full_path = self.resolve(&path);
        tracing::debug!("Calling 'ls' on path: {}", full_path.display());
        fs::read_dir(full_path)?
            .map(|res| res.map(|e| e.file_name().into()))
            .collect::<Result<_, std::io::Error>>()
            .map_err(Into::into)
    }

    fn del_file(&self, path: &Path) -> Result<(), Self::Error> {
        let full_path = self.resolve(&path);
        tracing::debug!("Calling 'del_file' on path: {}", full_path.display());
        fs::remove_file(full_path).map_err(Into::into)
    }

    fn mkdir_p(&self, path: &Path) -> Result<(), Self::Error> {
        fs::create_dir_all(self.resolve(&path)).map_err(Into::into)
    }

    fn put(&self, from: &Path, to: &Path) -> Result<(), Self::Error> {
        let to_path = self.resolve(&to);
        tracing::debug!(
            "Calling 'put' from path {} to path: {}",
            from.display(),
            to_path.display()
        );
        fs::copy(from, to_path).map(|_| ()).map_err(Into::into)
    }

    fn put_from_memory(&self, from: &[u8], to: &Path) -> Result<(), Self::Error> {
        let to_path = self.resolve(&to);
        tracing::debug!(
            "Calling 'put_from_memory' for memory data with size {} bytes to path: {}",
            from.len(),
            to_path.display()
        );
        Ok(fs::write(to_path, from)?)
    }

    fn dir_exists(&self, path: &Path) -> Result<bool, Self::Error> {
        let full_path = self.resolve(&path);
        tracing::debug!("Calling 'dir_exists' on path: {}", full_path.display());
        Ok(full_path.is_dir())
    }

    fn file_exists(&self, path: &Path) -> Result<bool, Self::Error> {
        tracing::debug!("Calling 'file_exists' on path: {}", path.display());
        Ok(self.resolve(&path).is_file())
    }

    fn path_descriptor(&self) -> &Arc<PathDescriptor> {
        &self.path_descriptor
    }
}

// TODO: test with some temp dir
