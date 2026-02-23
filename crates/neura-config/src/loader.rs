use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::{info, warn};

use crate::schema::SystemConfig;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("TOML serialize error: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("Config not found at: {0}")]
    NotFound(String),
}

pub type ConfigResult<T> = Result<T, ConfigError>;

pub struct ConfigManager {
    path: PathBuf,
    config: SystemConfig,
}

impl ConfigManager {
    /// Load config from path, or create default if not found.
    pub fn load(path: &Path) -> ConfigResult<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            let config: SystemConfig = toml::from_str(&content)?;
            info!("Loaded config from {}", path.display());
            Ok(Self {
                path: path.to_path_buf(),
                config,
            })
        } else {
            let config = SystemConfig::default();
            let manager = Self {
                path: path.to_path_buf(),
                config,
            };
            manager.save()?;
            info!("Created default config at {}", path.display());
            Ok(manager)
        }
    }

    /// Save the current config to disk.
    pub fn save(&self) -> ConfigResult<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(&self.config)?;
        std::fs::write(&self.path, content)?;
        Ok(())
    }

    /// Reload config from disk.
    pub fn reload(&mut self) -> ConfigResult<()> {
        if self.path.exists() {
            let content = std::fs::read_to_string(&self.path)?;
            self.config = toml::from_str(&content)?;
            info!("Config reloaded");
            Ok(())
        } else {
            warn!("Config file missing, keeping current config");
            Ok(())
        }
    }

    pub fn get(&self) -> &SystemConfig {
        &self.config
    }

    pub fn get_mut(&mut self) -> &mut SystemConfig {
        &mut self.config
    }
}
