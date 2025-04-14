use std::{
    io::{BufRead, BufReader},
    net::TcpStream,
    path::{Path, PathBuf},
};

use crate::traits::StoreDestination;

use ssh2::{self, ErrorCode, OpenFlags, Session};

pub struct SftpImpl {
    #[allow(dead_code)]
    session: ssh2::Session,
    sftp: ssh2::Sftp,
}

impl SftpImpl {
    pub fn new_with_public_key<P: AsRef<Path>>(
        host: &str,
        username: &str,
        priv_key_path: &P,
    ) -> Result<Self, SftpError> {
        let mut session = Session::new().map_err(SftpError::SessionInitError)?;

        let tcp = TcpStream::connect(host).unwrap();
        session.set_tcp_stream(tcp);
        session.handshake().map_err(SftpError::HandshakeFailed)?;

        if !priv_key_path.as_ref().exists() {
            return Err(SftpError::PrivKeyNotFoundInPath(
                priv_key_path.as_ref().to_owned(),
            ));
        }

        session
            .userauth_pubkey_file(username, None, priv_key_path.as_ref(), None)
            .map_err(SftpError::PubKeyAuthError)?;

        let sftp = session.sftp().map_err(SftpError::SftpChannelOpenFailed)?;

        let result = SftpImpl { session, sftp };
        Ok(result)
    }

    pub fn ls<P: AsRef<Path>>(&self, path: &P) -> Result<Vec<PathBuf>, SftpError> {
        let contents = self
            .sftp
            .readdir(path.as_ref())
            .map_err(SftpError::LsFailed)?;
        let names = contents.into_iter().map(|v| v.0).collect();
        Ok(names)
    }

    pub fn del<P: AsRef<Path>>(&self, path: &P) -> Result<(), SftpError> {
        self.sftp
            .unlink(path.as_ref())
            .map_err(SftpError::DelFileFailed)
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

    pub fn put<P: AsRef<Path>, Q: AsRef<Path>>(&self, from: &P, to: &Q) -> Result<(), SftpError> {
        if !from.as_ref().exists() {
            return Err(SftpError::SourceFileNotFound(from.as_ref().to_owned()));
        }
        let src_file = std::fs::File::open(from)
            .map_err(|e| SftpError::SourceFileOpenFailed(from.as_ref().to_owned(), e))?;
        let dest_file = self
            .sftp
            .open_mode(
                to.as_ref(),
                OpenFlags::WRITE | OpenFlags::CREATE,
                0o600,
                ssh2::OpenType::File,
            )
            .map_err(SftpError::OpenDestinationFileToWriteFailed)?;

        // We don't use std::io::buffer because this is more efficient with buffering
        Self::copy_buffers(src_file, dest_file)?;

        Ok(())
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

    pub fn dir_exists<P: AsRef<Path>>(&self, path: &P) -> Result<bool, SftpError> {
        match self.sftp.readdir(path.as_ref()) {
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

    pub fn file_exists<P: AsRef<Path>>(&self, path: &P) -> Result<bool, SftpError> {
        match self.sftp.open(path.as_ref()) {
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

    pub fn mkdir<P: AsRef<Path>>(&self, path: &P) -> Result<(), SftpError> {
        self.sftp
            .mkdir(path.as_ref(), 0o700)
            .map_err(SftpError::MkdirFailed)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum SftpError {
    #[error("Initialization failed {0}")]
    SessionInitError(ssh2::Error),
    #[error("Handshake failed {0}")]
    HandshakeFailed(ssh2::Error),
    #[error("Public key isn't found in path {0}")]
    PrivKeyNotFoundInPath(PathBuf),
    #[error("Public key auth failed {0}")]
    PubKeyAuthError(ssh2::Error),
    #[error("Opening sftp channel {0}")]
    SftpChannelOpenFailed(ssh2::Error),
    #[error("List dir contents failed {0}")]
    LsFailed(ssh2::Error),
    #[error("Del file failed {0}")]
    DelFileFailed(ssh2::Error),
    #[error("Mkdir failed {0}")]
    MkdirFailed(ssh2::Error),
    #[error("Open file to write failed {0}")]
    OpenDestinationFileToWriteFailed(ssh2::Error),
    #[error("Could not find source file for put {0}")]
    SourceFileNotFound(PathBuf),
    #[error("Could not open source file for put {0}")]
    SourceFileOpenFailed(PathBuf, std::io::Error),
    #[error("Copy file for put failed {0}")]
    FileCopyForPutFailed(std::io::Error),
    #[error("Dir exists check error {0}")]
    DirExistsCheckError(ssh2::Error),
    #[error("Read source file buffer error {0}")]
    ReadBufferError(std::io::Error),
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

impl StoreDestination for SftpImpl {
    type Error = SftpError;

    fn ls(&self, path: &Path) -> Result<Vec<PathBuf>, Self::Error> {
        self.ls(&path)
    }

    fn del_file(&self, path: &Path) -> Result<(), Self::Error> {
        self.del(&path)
    }

    fn put(&self, from: &Path, to: &Path) -> Result<(), Self::Error> {
        self.put(&from, &to)
    }

    fn mkdir_p(&self, path: &Path) -> Result<(), Self::Error> {
        if self.dir_exists(&path)? {
            return Ok(());
        }

        let parents = get_all_parents_for_mkdir_p(path);
        for p in parents {
            if !self.dir_exists(&p)? {
                self.mkdir(&p)?;
            }
        }

        self.mkdir(&path)
    }

    fn dir_exists(&self, path: &Path) -> Result<bool, Self::Error> {
        self.dir_exists(&path)
    }

    fn file_exists(&self, path: &Path) -> Result<bool, Self::Error> {
        self.file_exists(&path)
    }
}
