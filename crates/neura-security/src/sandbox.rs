use std::collections::HashSet;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SandboxError {
    #[error("Path access denied: {0}")]
    PathDenied(String),
    #[error("Command not allowed: {0}")]
    CommandDenied(String),
    #[error("Network access denied")]
    NetworkDenied,
}

pub type SandboxResult<T> = Result<T, SandboxError>;

#[derive(Debug, Clone)]
pub struct Sandbox {
    pub allowed_read_paths: HashSet<PathBuf>,
    pub allowed_write_paths: HashSet<PathBuf>,
    pub allowed_commands: HashSet<String>,
    pub allow_network: bool,
    pub max_execution_secs: u64,
    pub max_memory_bytes: u64,
}

impl Sandbox {
    pub fn restrictive() -> Self {
        Self {
            allowed_read_paths: HashSet::new(),
            allowed_write_paths: HashSet::new(),
            allowed_commands: HashSet::new(),
            allow_network: false,
            max_execution_secs: 30,
            max_memory_bytes: 256 * 1024 * 1024, // 256 MB
        }
    }

    pub fn permissive() -> Self {
        Self {
            allowed_read_paths: HashSet::new(),
            allowed_write_paths: HashSet::new(),
            allowed_commands: HashSet::new(),
            allow_network: true,
            max_execution_secs: 0,
            max_memory_bytes: 0,
        }
    }

    pub fn allow_read(&mut self, path: PathBuf) -> &mut Self {
        self.allowed_read_paths.insert(path);
        self
    }

    pub fn allow_write(&mut self, path: PathBuf) -> &mut Self {
        self.allowed_write_paths.insert(path);
        self
    }

    pub fn allow_command(&mut self, cmd: impl Into<String>) -> &mut Self {
        self.allowed_commands.insert(cmd.into());
        self
    }

    pub fn check_read(&self, path: &PathBuf) -> SandboxResult<()> {
        if self.allowed_read_paths.is_empty() {
            return Ok(());
        }
        for allowed in &self.allowed_read_paths {
            if path.starts_with(allowed) {
                return Ok(());
            }
        }
        Err(SandboxError::PathDenied(path.display().to_string()))
    }

    pub fn check_write(&self, path: &PathBuf) -> SandboxResult<()> {
        if self.allowed_write_paths.is_empty() {
            return Ok(());
        }
        for allowed in &self.allowed_write_paths {
            if path.starts_with(allowed) {
                return Ok(());
            }
        }
        Err(SandboxError::PathDenied(path.display().to_string()))
    }

    pub fn check_command(&self, cmd: &str) -> SandboxResult<()> {
        if self.allowed_commands.is_empty() {
            return Ok(());
        }
        if self.allowed_commands.contains(cmd) {
            Ok(())
        } else {
            Err(SandboxError::CommandDenied(cmd.to_string()))
        }
    }

    pub fn check_network(&self) -> SandboxResult<()> {
        if self.allow_network {
            Ok(())
        } else {
            Err(SandboxError::NetworkDenied)
        }
    }
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::restrictive()
    }
}
