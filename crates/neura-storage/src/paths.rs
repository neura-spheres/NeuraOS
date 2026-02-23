use std::path::PathBuf;
use neura_kernel::fs_ops::neura_home;

// ── VFS bootstrap directories ───────────────────────────────────────────────
// These are the top-level directories created during first-boot VFS setup.
// Keep them in sync with neura_app_framework::consts::vfs_paths.
pub const VFS_HOME:        &str = "/home";
pub const VFS_SYSTEM:      &str = "/system";
pub const VFS_SYSTEM_CFG:  &str = "/system/config";
pub const VFS_SYSTEM_LOGS: &str = "/system/logs";
pub const VFS_TMP:         &str = "/tmp";
pub const VFS_APPS:        &str = "/apps";

/// Default directories created during VFS bootstrap, in creation order.
pub const VFS_BOOTSTRAP_DIRS: &[&str] = &[
    VFS_HOME, VFS_SYSTEM, VFS_SYSTEM_CFG, VFS_SYSTEM_LOGS, VFS_TMP, VFS_APPS,
];

pub fn db_path() -> PathBuf {
    neura_home().join("data/db/neura.db")
}

pub fn user_home(username: &str) -> PathBuf {
    neura_home().join("users").join(username)
}

pub fn app_data_dir(app_id: &str) -> PathBuf {
    neura_home().join("apps").join(app_id).join("data")
}

pub fn cache_dir() -> PathBuf {
    neura_home().join("cache")
}

pub fn tmp_dir() -> PathBuf {
    neura_home().join("tmp")
}

pub fn config_dir() -> PathBuf {
    neura_home().join("config")
}

pub fn logs_dir() -> PathBuf {
    neura_home().join("logs")
}

pub fn plugins_dir() -> PathBuf {
    neura_home().join("plugins")
}

pub fn packages_dir() -> PathBuf {
    neura_home().join("packages")
}
