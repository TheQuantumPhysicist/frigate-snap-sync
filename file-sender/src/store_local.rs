use std::fs;
use std::path::Path;
use std::path::PathBuf;

use crate::traits::StoreDestination;
pub struct LocalStore {
    dest_dir: PathBuf,
}

impl LocalStore {
    pub fn new<P: AsRef<Path>>(dest_dir: P) -> Self {
        Self {
            dest_dir: dest_dir.as_ref().to_path_buf(),
        }
    }

    fn resolve<P: AsRef<Path>>(&self, path: &P) -> PathBuf {
        self.dest_dir.join(path)
    }
}

impl StoreDestination for LocalStore {
    type Error = anyhow::Error;

    fn ls(&self, path: &Path) -> Result<Vec<PathBuf>, Self::Error> {
        let full_path = self.resolve(&path);
        fs::read_dir(full_path)?
            .map(|res| res.map(|e| e.file_name().into()))
            .collect::<Result<_, std::io::Error>>()
            .map_err(Into::into)
    }

    fn del_file(&self, path: &Path) -> Result<(), Self::Error> {
        fs::remove_file(self.resolve(&path)).map_err(Into::into)
    }

    fn mkdir_p(&self, path: &Path) -> Result<(), Self::Error> {
        fs::create_dir_all(self.resolve(&path)).map_err(Into::into)
    }

    fn put(&self, from: &Path, to: &Path) -> Result<(), Self::Error> {
        fs::copy(from, self.resolve(&to))
            .map(|_| ())
            .map_err(Into::into)
    }

    fn put_from_memory(&self, from: &[u8], to: &Path) -> Result<(), Self::Error> {
        Ok(fs::write(to, from)?)
    }

    fn dir_exists(&self, path: &Path) -> Result<bool, Self::Error> {
        Ok(self.resolve(&path).is_dir())
    }

    fn file_exists(&self, path: &Path) -> Result<bool, Self::Error> {
        Ok(self.resolve(&path).is_file())
    }
}
