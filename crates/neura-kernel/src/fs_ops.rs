use std::path::{Path, PathBuf};

use crate::syscall::{FsHost, SyscallResult};

/// Returns the NeuraOS root data directory (~/.neura/).
pub fn neura_home() -> PathBuf {
    dirs_next().join(".neura")
}

/// Returns the user's actual home directory from the OS.
fn dirs_next() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("C:\\Users\\Default"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"))
    }
}

/// Ensure the base NeuraOS directory structure exists on disk.
pub fn ensure_base_dirs() -> SyscallResult<()> {
    let home = neura_home();
    let dirs = [
        home.clone(),
        home.join("data"),
        home.join("data/db"),
        home.join("config"),
        home.join("cache"),
        home.join("tmp"),
        home.join("logs"),
        home.join("users"),
        home.join("apps"),
        home.join("plugins"),
        home.join("packages"),
    ];
    for dir in &dirs {
        FsHost::create_dir(dir)?;
    }
    Ok(())
}

/// Resolve a virtual path to a real host filesystem path.
pub fn resolve_path(virtual_path: &Path) -> PathBuf {
    neura_home().join("data").join(virtual_path)
}
