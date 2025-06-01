mod blocking;

use crate::{
    path_descriptor::{IdentitySource, PathDescriptor},
    traits::StoreDestination,
};
use blocking::BlockingSftpImpl;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub struct AsyncSftpImpl {
    sftp: Arc<tokio::sync::Mutex<blocking::BlockingSftpImpl>>,
    path_descriptor: Arc<PathDescriptor>,
}

impl AsyncSftpImpl {
    pub fn new_with_public_key(
        path_descriptor: Arc<PathDescriptor>,
        host: &str,
        username: &str,
        priv_key: IdentitySource,
        base_remote_path: impl Into<PathBuf>,
    ) -> Result<Self, SftpError> {
        let sftp = BlockingSftpImpl::new_with_public_key(
            path_descriptor.clone(),
            host,
            username,
            priv_key,
            base_remote_path,
        )?;

        let result = Self {
            sftp: Arc::new(tokio::sync::Mutex::new(sftp)),
            path_descriptor,
        };

        Ok(result)
    }
}

// libssh2 doesn't provide an async implementation, so we use blocking tasks to substitute for it
#[async_trait::async_trait]
impl StoreDestination for AsyncSftpImpl {
    type Error = anyhow::Error;

    async fn init(&self) -> Result<(), Self::Error> {
        let session = self.sftp.clone();
        tokio::task::spawn_blocking(async move || session.lock().await.init())
            .await?
            .await?;
        Ok(())
    }

    async fn ls(&self, path: &Path) -> Result<Vec<PathBuf>, Self::Error> {
        let session = self.sftp.clone();
        let path = path.to_owned();
        let result = tokio::task::spawn_blocking(async move || session.lock().await.ls(&path))
            .await?
            .await?;
        Ok(result)
    }

    async fn del_file(&self, path: &Path) -> Result<(), Self::Error> {
        let session = self.sftp.clone();
        let path = path.to_owned();
        tokio::task::spawn_blocking(async move || session.lock().await.del(&path))
            .await?
            .await?;
        Ok(())
    }

    async fn put(&self, from: &Path, to: &Path) -> Result<(), Self::Error> {
        let session = self.sftp.clone();
        let from = from.to_owned();
        let to = to.to_owned();
        tokio::task::spawn_blocking(async move || session.lock().await.put(&from, &to))
            .await?
            .await?;
        Ok(())
    }

    async fn put_from_memory(&self, from: &[u8], to: &Path) -> Result<(), Self::Error> {
        let session = self.sftp.clone();
        let from = from.to_owned();
        let to = to.to_owned();
        tokio::task::spawn_blocking(async move || session.lock().await.put_from_memory(&from, &to))
            .await?
            .await?;
        Ok(())
    }

    async fn get_to_memory(&self, from: &Path) -> Result<Vec<u8>, Self::Error> {
        let session = self.sftp.clone();
        let from = from.to_owned();
        let result =
            tokio::task::spawn_blocking(async move || session.lock().await.get_to_memory(&from))
                .await?
                .await?;
        Ok(result)
    }

    async fn mkdir_p(&self, path: &Path) -> Result<(), Self::Error> {
        let session = self.sftp.clone();
        let path = path.to_owned();
        tokio::task::spawn_blocking(async move || session.lock().await.mkdir_p(&path))
            .await?
            .await?;
        Ok(())
    }

    async fn dir_exists(&self, path: &Path) -> Result<bool, Self::Error> {
        let session = self.sftp.clone();
        let path = path.to_owned();
        let result =
            tokio::task::spawn_blocking(async move || session.lock().await.dir_exists(&path))
                .await?
                .await?;
        Ok(result)
    }

    async fn file_exists(&self, path: &Path) -> Result<bool, Self::Error> {
        let session = self.sftp.clone();
        let path = path.to_owned();
        let result =
            tokio::task::spawn_blocking(async move || session.lock().await.file_exists(&path))
                .await?
                .await?;
        Ok(result)
    }

    fn path_descriptor(&self) -> &Arc<PathDescriptor> {
        &self.path_descriptor
    }
}

#[derive(thiserror::Error, Debug)]
pub enum SftpError {
    #[error("Initialization failed: {0}")]
    SessionInitError(ssh2::Error),
    #[error("Handshake failed: {0}")]
    HandshakeFailed(ssh2::Error),
    #[error("Public key isn't readable in path. Error: {0}")]
    PrivKeyNotFoundInPath(PathBuf),
    #[error("Private key isn't readable in path. Error: {0}")]
    PrivKeyReadError(std::io::Error),
    #[error("Public key auth failed: {0}")]
    PubKeyAuthError(ssh2::Error),
    #[error("Opening sftp channel: {0}")]
    SftpChannelOpenFailed(ssh2::Error),
    #[error("List dir contents failed: {0}")]
    LsFailed(ssh2::Error),
    #[error("Del file failed: {0}")]
    DelFileFailed(ssh2::Error),
    #[error("Mkdir failed: {0}")]
    MkdirFailed(ssh2::Error),
    #[error("Open file to write failed: {0}")]
    OpenDestinationFileToWriteFailed(ssh2::Error),
    #[error("Open file to read failed: {0}")]
    OpenDestinationFileToReadFailed(ssh2::Error),
    #[error("Could not find source file for put: {0}")]
    SourceFileNotFound(PathBuf),
    #[error("Destination path not found: {0}")]
    DestPathNotFound(PathBuf),
    #[error("Could not open source file for put: {0}")]
    SourceFileOpenFailed(PathBuf, std::io::Error),
    #[error("Copy file for put failed: {0}")]
    FileCopyForPutFailed(std::io::Error),
    #[error("Dir exists check error: {0}")]
    DirExistsCheckError(ssh2::Error),
    #[error("Read source file buffer error: {0}")]
    ReadBufferError(std::io::Error),
    #[error("Read remote file error: {0}")]
    ReadRemoteFileError(std::io::Error),
}
