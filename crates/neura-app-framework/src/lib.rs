pub mod app_trait;
pub mod lifecycle;
pub mod sandbox;
pub mod config;

/// Centralized Tokyo Night colour palette — import with `use neura_app_framework::palette::*`.
pub mod palette;

/// OS-wide constants: identity, defaults, app IDs, VFS paths, timezone helpers.
pub mod consts;

pub use app_trait::{App, AppId, AppManifest};
pub use lifecycle::AppLifecycleManager;
