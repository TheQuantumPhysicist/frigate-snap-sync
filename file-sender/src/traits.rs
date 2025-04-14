use std::path::{Path, PathBuf};

pub trait StoreDestination {
    type Error;

    fn ls(&self, path: &Path) -> Result<Vec<PathBuf>, Self::Error>;
    fn del_file(&self, path: &Path) -> Result<(), Self::Error>;
    fn mkdir_p(&self, path: &Path) -> Result<(), Self::Error>;
    fn put(&self, from: &Path, to: &Path) -> Result<(), Self::Error>;
    fn dir_exists(&self, path: &Path) -> Result<bool, Self::Error>;
    fn file_exists(&self, path: &Path) -> Result<bool, Self::Error>;
}
