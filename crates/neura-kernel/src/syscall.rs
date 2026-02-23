use std::io;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SyscallError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Not found: {0}")]
    NotFound(String),
}

pub type SyscallResult<T> = Result<T, SyscallError>;

/// Abstraction over host filesystem operations.
/// All real I/O goes through here so we can audit/sandbox it.
pub struct FsHost;

impl FsHost {
    pub fn read_file(path: &Path) -> SyscallResult<Vec<u8>> {
        std::fs::read(path).map_err(SyscallError::Io)
    }

    pub fn write_file(path: &Path, data: &[u8]) -> SyscallResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(SyscallError::Io)?;
        }
        std::fs::write(path, data).map_err(SyscallError::Io)
    }

    pub fn create_dir(path: &Path) -> SyscallResult<()> {
        std::fs::create_dir_all(path).map_err(SyscallError::Io)
    }

    pub fn remove_file(path: &Path) -> SyscallResult<()> {
        std::fs::remove_file(path).map_err(SyscallError::Io)
    }

    pub fn remove_dir(path: &Path) -> SyscallResult<()> {
        std::fs::remove_dir_all(path).map_err(SyscallError::Io)
    }

    pub fn exists(path: &Path) -> bool {
        path.exists()
    }

    pub fn metadata(path: &Path) -> SyscallResult<std::fs::Metadata> {
        std::fs::metadata(path).map_err(SyscallError::Io)
    }

    pub fn list_dir(path: &Path) -> SyscallResult<Vec<std::fs::DirEntry>> {
        let entries = std::fs::read_dir(path)
            .map_err(SyscallError::Io)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(SyscallError::Io)?;
        Ok(entries)
    }
}
