use async_trait::async_trait;
use file_sender::path_descriptor::PathDescriptor;
use file_sender::traits::StoreDestination;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[must_use]
pub fn make_store_mock() -> Box<dyn StoreDestination<Error = anyhow::Error>> {
    Box::new(MockStoreDest::new())
}

mockall::mock! {
    pub StoreDest {}

    #[async_trait]
    impl StoreDestination for StoreDest {
        type Error = anyhow::Error;

        async fn init(&self) -> Result<(), anyhow::Error>;
        async fn ls(&self, path: &Path) -> Result<Vec<PathBuf>, anyhow::Error>;
        async fn del_file(&self, path: &Path) -> Result<(), anyhow::Error>;
        async fn mkdir_p(&self, path: &Path) -> Result<(), anyhow::Error>;
        async fn put(&self, from: &Path, to: &Path) -> Result<(), anyhow::Error>;
        async fn put_from_memory(&self, from: &[u8], to: &Path) -> Result<(), anyhow::Error>;
        async fn get_to_memory(&self, from: &Path) -> Result<Vec<u8>, anyhow::Error>;
        async fn dir_exists(&self, path: &Path) -> Result<bool, anyhow::Error>;
        async fn file_exists(&self, path: &Path) -> Result<bool, anyhow::Error>;
        fn path_descriptor(&self) -> &Arc<PathDescriptor>;
    }
}
