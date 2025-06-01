use crate::{
    path_descriptor::{IdentitySource, PathDescriptor},
    traits::StoreDestination,
};
use async_trait::async_trait;
use ssh2::{self, ErrorCode, OpenFlags, Session};
use std::{
    io::{BufRead, BufReader, Read},
    net::TcpStream,
    path::{Path, PathBuf},
    sync::Arc,
};
use tracing::trace_span;

use super::SftpError;

pub struct BlockingSftpImpl {
    path_descriptor: Arc<PathDescriptor>,
    #[allow(dead_code)]
    session: ssh2::Session,
    sftp: ssh2::Sftp,
    base_remote_path: PathBuf,
}

impl BlockingSftpImpl {
    #[allow(clippy::unused_async)]
    pub fn new_with_public_key(
        path_descriptor: Arc<PathDescriptor>,
        host: &str,
        username: &str,
        priv_key: IdentitySource,
        base_remote_path: impl Into<PathBuf>,
    ) -> Result<Self, SftpError> {
        let mut session = Session::new().map_err(SftpError::SessionInitError)?;

        let tcp = TcpStream::connect(host).unwrap();
        session.set_tcp_stream(tcp);
        session.handshake().map_err(SftpError::HandshakeFailed)?;

        let priv_key = priv_key.into_key()?;

        session
            .userauth_pubkey_memory(username, None, &priv_key, None)
            .map_err(SftpError::PubKeyAuthError)?;

        let sftp = session.sftp().map_err(SftpError::SftpChannelOpenFailed)?;

        let base_remote_path = simplify_virtual_path(&base_remote_path.into());

        let result = BlockingSftpImpl {
            path_descriptor,
            session,
            sftp,
            base_remote_path,
        };

        Ok(result)
    }

    fn resolve(&self, path: impl AsRef<Path>) -> PathBuf {
        self.base_remote_path.join(path)
    }

    fn ls_inner<P: AsRef<Path>>(&self, path: P) -> Result<Vec<PathBuf>, SftpError> {
        let path = self.resolve(path.as_ref());
        let contents = self.sftp.readdir(path).map_err(SftpError::LsFailed)?;
        let names = contents.into_iter().map(|v| v.0).collect();
        Ok(names)
    }

    pub fn init(&self) -> Result<(), SftpError> {
        let span = trace_span!("make_frigate_client");
        let _enter = span.enter();

        tracing::trace!(
            "Initializing file sender: {}",
            self.path_descriptor.to_string()
        );

        if !self.dir_exists(&self.base_remote_path)? {
            tracing::trace!(
                "Path in descriptor does not exist. Creating it: {}",
                self.base_remote_path.display()
            );

            self.mkdir_p_low_level(&self.base_remote_path)
                .inspect_err(|e| {
                    tracing::trace!(
                        "Creating path failed: `{}`. Error: `{e}`",
                        self.base_remote_path.display()
                    );
                })
                .inspect(|()| {
                    tracing::trace!(
                        "Creating base path in init() success: `{}`",
                        self.base_remote_path.display()
                    );
                })?;
        }

        self.sftp
            .opendir(&self.base_remote_path)
            .map_err(|_e| SftpError::DestPathNotFound(self.base_remote_path.clone()))
            .inspect_err(|e| {
                tracing::trace!(
                    "Opening dir failed. Dir: `{}`. Error: `{e}`",
                    self.base_remote_path.display()
                );
            })
            .inspect(|_| {
                tracing::trace!(
                    "Opening dir in init success. Dir: `{}`.",
                    self.base_remote_path.display()
                );
            })?;

        Ok(())
    }

    pub fn ls(&self, path: &Path) -> Result<Vec<PathBuf>, SftpError> {
        let result = self.ls_inner(path)?;
        let result = result
            .into_iter()
            .map(|p| {
                simplify_virtual_path(&p)
                    .strip_prefix(&self.base_remote_path)
                    .map(std::borrow::ToOwned::to_owned)
                    .unwrap_or(p)
            })
            .collect::<Vec<_>>();
        Ok(result)
    }

    pub fn del<P: AsRef<Path>>(&self, path: P) -> Result<(), SftpError> {
        let path = self.resolve(path.as_ref());
        self.sftp.unlink(&path).map_err(SftpError::DelFileFailed)
    }

    fn copy_buffers(
        src: impl std::io::Read,
        mut dst: impl std::io::Write,
    ) -> Result<(), SftpError> {
        let mut buffer_queue = Vec::<u8>::new();
        let max_buffer_size = 1 << 24;
        let mut src_file_reader = BufReader::new(src);
        loop {
            let size = Self::fill_buffer(&mut buffer_queue, &mut src_file_reader, max_buffer_size)?;
            if size == 0 {
                break;
            }

            dst.write_all(&buffer_queue)
                .map_err(SftpError::FileCopyForPutFailed)?;
            buffer_queue.clear();
        }

        Ok(())
    }

    pub fn put<P: AsRef<Path>, Q: AsRef<Path>>(&self, from: P, to: Q) -> Result<(), SftpError> {
        let to = self.resolve(to.as_ref());
        if !from.as_ref().exists() {
            return Err(SftpError::SourceFileNotFound(from.as_ref().to_owned()));
        }
        let from = from.as_ref();
        let src_file = std::fs::File::open(from)
            .map_err(|e| SftpError::SourceFileOpenFailed(from.to_owned(), e))?;
        let dest_file = self
            .sftp
            .open_mode(
                to,
                OpenFlags::WRITE | OpenFlags::CREATE,
                0o600,
                ssh2::OpenType::File,
            )
            .map_err(SftpError::OpenDestinationFileToWriteFailed)?;

        // We don't use std::io::buffer because this is more efficient with buffering
        Self::copy_buffers(src_file, dest_file)?;

        Ok(())
    }

    pub fn put_from_memory<P: AsRef<[u8]>, Q: AsRef<Path>>(
        &self,
        from: P,
        to: Q,
    ) -> Result<(), SftpError> {
        let to = self.resolve(to.as_ref());

        let dest_file = self
            .sftp
            .open_mode(
                to,
                OpenFlags::WRITE | OpenFlags::CREATE,
                0o600,
                ssh2::OpenType::File,
            )
            .map_err(SftpError::OpenDestinationFileToWriteFailed)?;

        let from_buffer = from.as_ref();

        // We don't use std::io::buffer because this is more efficient with buffering
        Self::copy_buffers(from_buffer, dest_file)?;

        Ok(())
    }

    pub fn get_to_memory<Q: AsRef<Path>>(&self, from: Q) -> Result<Vec<u8>, SftpError> {
        let from = self.resolve(from.as_ref());

        let mut dest_file = self
            .sftp
            .open(from)
            .map_err(SftpError::OpenDestinationFileToReadFailed)?;

        let mut result = Vec::new();
        dest_file
            .read_to_end(&mut result)
            .map_err(SftpError::ReadRemoteFileError)?;

        Ok(result)
    }

    fn fill_buffer<S: std::io::Read>(
        buffer_queue: &mut Vec<u8>,
        reader: &mut std::io::BufReader<S>,
        max_buffer_size: usize,
    ) -> Result<usize, SftpError> {
        let mut total_read = 0;
        while buffer_queue.len() < max_buffer_size {
            let buf_len = {
                let data = reader.fill_buf().map_err(SftpError::ReadBufferError)?;
                if data.is_empty() {
                    break;
                }
                buffer_queue.extend(data.iter());
                data.len()
            };
            total_read += buf_len;
            reader.consume(buf_len);
        }

        Ok(total_read)
    }

    pub fn dir_exists<P: AsRef<Path>>(&self, path: P) -> Result<bool, SftpError> {
        let path = self.resolve(path.as_ref());
        self.dir_exists_low_level(path)
    }

    // Same as dir_exists, but without resolving
    fn dir_exists_low_level<P: AsRef<Path>>(&self, path: P) -> Result<bool, SftpError> {
        match self.sftp.readdir(path) {
            Ok(_) => Ok(true),
            Err(e) => {
                if e.code() == ErrorCode::SFTP(libssh2_sys::LIBSSH2_FX_NO_SUCH_FILE) {
                    Ok(false)
                } else {
                    Err(SftpError::DirExistsCheckError(e))
                }
            }
        }
    }

    pub fn file_exists<P: AsRef<Path>>(&self, path: P) -> Result<bool, SftpError> {
        let path = self.resolve(path.as_ref());
        match self.sftp.open(path) {
            Ok(_) => Ok(true),
            Err(e) => {
                if e.code() == ErrorCode::SFTP(libssh2_sys::LIBSSH2_FX_NO_SUCH_FILE) {
                    Ok(false)
                } else {
                    Err(SftpError::DirExistsCheckError(e))
                }
            }
        }
    }

    /// Functionality of mkdir, but without resolving
    fn mkdir_low_level<P: AsRef<Path>>(&self, path: P) -> Result<(), SftpError> {
        if self.dir_exists_low_level(path.as_ref())? {
            return Ok(());
        }
        self.sftp
            .mkdir(path.as_ref(), 0o700)
            .map_err(SftpError::MkdirFailed)
    }

    /// Functionality of `mkdir_p`, but without resolving
    fn mkdir_p_low_level(&self, path: &Path) -> Result<(), SftpError> {
        if self.dir_exists(path)? {
            return Ok(());
        }

        let parents = get_all_parents_for_mkdir_p(path);
        for p in parents {
            if !self.dir_exists(&p)? {
                self.mkdir_low_level(&p)?;
            }
        }

        self.mkdir_low_level(path)
    }

    pub fn mkdir_p(&self, path: &Path) -> Result<(), SftpError> {
        let path_resolved = self.resolve(path);
        if self.dir_exists(&path_resolved)? {
            return Ok(());
        }

        let parents = get_all_parents_for_mkdir_p(path);
        for p in parents {
            if !self.dir_exists(&p)? {
                self.mkdir_low_level(self.resolve(p))?;
            }
        }

        self.mkdir_low_level(path_resolved)
    }

    pub fn path_descriptor(&self) -> &Arc<PathDescriptor> {
        &self.path_descriptor
    }
}

fn get_all_parents_for_mkdir_p<P: AsRef<Path>>(path: P) -> Vec<PathBuf> {
    let mut result = Vec::new();
    let mut path = path.as_ref().to_owned();
    while let Some(p) = path.parent() {
        if p.to_string_lossy() != "" {
            result.push(p.to_owned());
        }
        path = p.to_owned();
    }

    result.into_iter().rev().collect()
}

/// Simplifies cases of `abc/./xyz` to `abc/xyz`... and similar.
fn simplify_virtual_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    let mut stack = Vec::new();
    let is_absolute = path.is_absolute();

    for comp in path.components() {
        match comp {
            std::path::Component::Prefix(_) => result.push(comp),
            std::path::Component::RootDir => {
                result.push(comp);
                stack.clear(); // root resets the stack
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if let Some(last) = stack.pop() {
                    if matches!(last, std::path::Component::Normal(_)) {
                        // dropped
                    } else {
                        stack.push(last);
                        if !is_absolute {
                            stack.push(comp);
                        }
                    }
                } else if !is_absolute {
                    stack.push(comp);
                }
            }
            std::path::Component::Normal(_) => stack.push(comp),
        }
    }

    for comp in stack {
        result.push(comp);
    }

    result
}

#[async_trait]
impl StoreDestination for BlockingSftpImpl {
    type Error = anyhow::Error;

    async fn init(&self) -> Result<(), Self::Error> {
        self.init().map_err(Into::into)
    }

    async fn ls(&self, path: &Path) -> Result<Vec<PathBuf>, Self::Error> {
        self.ls(path).map_err(Into::into)
    }

    async fn del_file(&self, path: &Path) -> Result<(), Self::Error> {
        self.del(path).map_err(Into::into)
    }

    async fn put(&self, from: &Path, to: &Path) -> Result<(), Self::Error> {
        self.put(from, to).map_err(Into::into)
    }

    async fn put_from_memory(&self, from: &[u8], to: &Path) -> Result<(), Self::Error> {
        self.put_from_memory(from, to).map_err(Into::into)
    }

    async fn get_to_memory(&self, from: &Path) -> Result<Vec<u8>, Self::Error> {
        self.get_to_memory(from).map_err(Into::into)
    }

    async fn mkdir_p(&self, path: &Path) -> Result<(), Self::Error> {
        self.mkdir_p(path).map_err(Into::into)
    }

    async fn dir_exists(&self, path: &Path) -> Result<bool, Self::Error> {
        self.dir_exists(path).map_err(Into::into)
    }

    async fn file_exists(&self, path: &Path) -> Result<bool, Self::Error> {
        self.file_exists(path).map_err(Into::into)
    }

    fn path_descriptor(&self) -> Arc<PathDescriptor> {
        self.path_descriptor.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simplify_virtual_path() {
        use std::path::{Path, PathBuf};

        let s = |p| simplify_virtual_path(Path::new(p));

        // Basic . and ..
        assert_eq!(s("a/./.."), PathBuf::from(""));
        assert_eq!(s("a/./b/../c"), PathBuf::from("a/c"));
        assert_eq!(s("a/./b"), PathBuf::from("a/b"));
        assert_eq!(s("./a/b"), PathBuf::from("a/b"));
        assert_eq!(s("a/b/."), PathBuf::from("a/b"));
        assert_eq!(s("."), PathBuf::from(""));
        assert_eq!(s("./."), PathBuf::from(""));
        assert_eq!(s("a/././b"), PathBuf::from("a/b"));
        assert_eq!(s("a//b"), PathBuf::from("a/b"));
        assert_eq!(s("a///b"), PathBuf::from("a/b"));
        assert_eq!(s("a/./b/./c"), PathBuf::from("a/b/c"));
        assert_eq!(s("a/./b/."), PathBuf::from("a/b"));
        assert_eq!(s("a/./b/./"), PathBuf::from("a/b"));
        assert_eq!(s(""), PathBuf::from(""));

        // Absolute paths
        assert_eq!(s("/a/./b"), PathBuf::from("/a/b"));
        assert_eq!(s("/./a/b"), PathBuf::from("/a/b"));
        assert_eq!(s("/a/b/."), PathBuf::from("/a/b"));
        assert_eq!(s("/./"), PathBuf::from("/"));
        assert_eq!(s("/"), PathBuf::from("/"));

        // Parent resolution
        assert_eq!(s("a/.."), PathBuf::from(""));
        assert_eq!(s("a/b/.."), PathBuf::from("a"));
        assert_eq!(s("a/b/../.."), PathBuf::from(""));
        assert_eq!(s("a/b/../../.."), PathBuf::from(".."));
        assert_eq!(s("a/./b/../c"), PathBuf::from("a/c"));
        assert_eq!(s("./a/../b/."), PathBuf::from("b"));
        assert_eq!(s("/a/b/../c"), PathBuf::from("/a/c"));
        assert_eq!(s("/a/../../b"), PathBuf::from("/b"));

        // Relative paths with leading ..
        assert_eq!(s("../../a/b"), PathBuf::from("../../a/b"));
        assert_eq!(s("../../../a"), PathBuf::from("../../../a"));
        assert_eq!(s("../a"), PathBuf::from("../a"));
        assert_eq!(s("../.."), PathBuf::from("../.."));
        assert_eq!(s(".."), PathBuf::from(".."));
        assert_eq!(s("./../a"), PathBuf::from("../a"));

        // Absolute paths trying to go above root
        assert_eq!(s("/.."), PathBuf::from("/"));
        assert_eq!(s("/../.."), PathBuf::from("/"));

        // Redundant parent dirs
        assert_eq!(s("a/b/../../c"), PathBuf::from("c"));
    }
}
