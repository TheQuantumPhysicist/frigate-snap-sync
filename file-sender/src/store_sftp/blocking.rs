use crate::{
    path_descriptor::{PathDescriptor, StringFileData},
    traits::StoreDestination,
};
use async_trait::async_trait;
use ssh2::{self, ErrorCode, OpenFlags, Session};
use std::{
    io::{BufRead, BufReader, Read},
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tracing::trace_span;

use super::SftpError;

// Upper bound on how long the initial TCP connection may block before the
// attempt is abandoned.
const TCP_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

// - Upper bound on every blocking libssh2 call on the session.
// - Covers the handshake, the public-key auth, and all later SFTP operations.
// - Without it libssh2 waits forever: a server that accepts the TCP connection
//   then stalls (for example its SSH daemon is still starting and has not sent
//   its banner) wedges the calling task indefinitely.
// - A bounded failure surfaces as an error the daemon's retry loop can recover from.
const SSH_OPERATION_TIMEOUT: Duration = Duration::from_secs(45);

pub struct BlockingSftpImpl {
    path_descriptor: Arc<PathDescriptor>,
    #[allow(dead_code)]
    session: ssh2::Session,
    sftp: ssh2::Sftp,
    base_remote_path: PathBuf,
}

impl BlockingSftpImpl {
    pub fn new_with_public_key(
        path_descriptor: Arc<PathDescriptor>,
        host: &str,
        username: &str,
        priv_key: StringFileData,
        base_remote_path: impl Into<PathBuf>,
    ) -> Result<Self, SftpError> {
        let session = connect_session(host, TCP_CONNECT_TIMEOUT, SSH_OPERATION_TIMEOUT)?;

        let priv_key = priv_key.into_file_data()?;

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
}

// - Opens a TCP connection to the host, performs the SSH handshake, returns the session.
// - Both timeouts are passed explicitly so the behavior can be tested in isolation.
// - connect_timeout bounds the TCP connect.
// - operation_timeout bounds the handshake and every later blocking call on the session.
fn connect_session(
    host: &str,
    connect_timeout: Duration,
    operation_timeout: Duration,
) -> Result<Session, SftpError> {
    let mut session = Session::new().map_err(SftpError::SessionInitError)?;

    let address = host
        .to_socket_addrs()
        .map_err(SftpError::TcpConnectionFailed)?
        .next()
        .ok_or_else(|| {
            SftpError::TcpConnectionFailed(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("No socket address resolved for host `{host}`"),
            ))
        })?;

    let tcp = TcpStream::connect_timeout(&address, connect_timeout)
        .map_err(SftpError::TcpConnectionFailed)?;
    session.set_tcp_stream(tcp);

    // - libssh2 takes the timeout in milliseconds as a u32.
    // - The source is a u128 millisecond count, always non-negative.
    // - The production constant fits comfortably in u32.
    // - A caller passing more than ~49 days saturates to the longest timeout, a safe degradation.
    let operation_timeout_ms = u32::try_from(operation_timeout.as_millis()).unwrap_or(u32::MAX);
    session.set_timeout(operation_timeout_ms);

    session.handshake().map_err(SftpError::HandshakeFailed)?;

    Ok(session)
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

    fn path_descriptor(&self) -> &Arc<PathDescriptor> {
        &self.path_descriptor
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::net::TcpListener;
    use std::time::Instant;

    // - A fake server accepts the TCP connection but never speaks the SSH protocol.
    // - This reproduces the hang that wedged the SFTP store: the handshake waits
    //   for a banner that never arrives.
    // - With the session timeout, the attempt must return an error within a bound.
    // - Without the timeout this test would hang forever.
    #[test]
    fn connect_session_does_not_hang_on_a_stalled_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let accept_thread = std::thread::spawn(move || {
            // - Accept the connection and keep it open while sending nothing.
            // - The client blocks waiting for an SSH banner that never comes.
            // - Draining without replying holds the socket open until the client gives up.
            // - When the client drops its socket, this read returns 0 and the thread exits.
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0u8; 64];
                while matches!(stream.read(&mut buffer), Ok(size) if size > 0) {}
            }
        });

        let operation_timeout = Duration::from_secs(1);
        let started = Instant::now();
        let result = connect_session(
            &address.to_string(),
            Duration::from_secs(5),
            operation_timeout,
        );
        let elapsed = started.elapsed();

        assert!(
            result.is_err(),
            "handshake against a stalled server must fail, not succeed"
        );
        // - The handshake must wait roughly the operation timeout before failing.
        // - That proves the timeout breaks the stall, not some unrelated early error.
        // - It must still return within a bound rather than hanging.
        assert!(
            elapsed >= operation_timeout / 2,
            "handshake returned before the timeout could fire; took {elapsed:?}"
        );
        assert!(
            elapsed < operation_timeout * 10,
            "connect must return within a bound on a stalled server; took {elapsed:?}"
        );

        accept_thread.join().unwrap();
    }

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
