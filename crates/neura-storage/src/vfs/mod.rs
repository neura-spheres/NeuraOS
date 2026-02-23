pub mod node;
pub mod permissions;
pub mod journal;
pub mod transaction;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
use thiserror::Error;
use tracing::info;

pub use node::{VfsNode, NodeType};
pub use permissions::VfsPermissions;
pub use journal::{JournalEntry, Journal};
pub use transaction::VfsTransaction;

#[derive(Error, Debug)]
pub enum VfsError {
    #[error("Path not found: {0}")]
    NotFound(String),
    #[error("Path already exists: {0}")]
    AlreadyExists(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Not a directory: {0}")]
    NotADirectory(String),
    #[error("Not a file: {0}")]
    NotAFile(String),
    #[error("Directory not empty: {0}")]
    NotEmpty(String),
    #[error("File is locked: {0}")]
    Locked(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Storage error: {0}")]
    Storage(String),
}

pub type VfsResult<T> = Result<T, VfsError>;

/// The virtual filesystem tree. Thread-safe via RwLock.
/// Supports disk persistence via JSON serialization.
pub struct Vfs {
    root: Arc<RwLock<VfsNode>>,
    journal: Arc<RwLock<Journal>>,
    /// Path to the on-disk snapshot file. None = in-memory only.
    persist_path: Option<PathBuf>,
    /// Dirty flag — true when the tree has been modified since last save.
    dirty: Arc<std::sync::atomic::AtomicBool>,
}

impl Vfs {
    /// Create a new in-memory-only VFS.
    pub fn new() -> Self {
        let root = VfsNode::new_dir("/".to_string(), "root".to_string(), VfsPermissions::default_dir());
        Self {
            root: Arc::new(RwLock::new(root)),
            journal: Arc::new(RwLock::new(Journal::new())),
            persist_path: None,
            dirty: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Create a VFS backed by a disk file. Loads existing data if present.
    pub fn with_persistence(path: &Path) -> VfsResult<Self> {
        let root = if path.exists() {
            let data = std::fs::read(path).map_err(VfsError::Io)?;
            serde_json::from_slice::<VfsNode>(&data)
                .map_err(|e| VfsError::Storage(format!("Failed to deserialize VFS: {}", e)))?
        } else {
            VfsNode::new_dir("/".to_string(), "root".to_string(), VfsPermissions::default_dir())
        };

        info!("VFS loaded from {}", path.display());
        Ok(Self {
            root: Arc::new(RwLock::new(root)),
            journal: Arc::new(RwLock::new(Journal::new())),
            persist_path: Some(path.to_path_buf()),
            dirty: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }

    /// Save the VFS tree to disk (if persistence is configured).
    pub async fn save(&self) -> VfsResult<()> {
        let Some(ref path) = self.persist_path else {
            return Ok(());
        };
        let root = self.root.read().await;
        let data = serde_json::to_vec(&*root)
            .map_err(|e| VfsError::Storage(format!("Failed to serialize VFS: {}", e)))?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(VfsError::Io)?;
        }

        // Atomic write: write to tmp then rename
        let tmp_path = path.with_extension("tmp");
        std::fs::write(&tmp_path, &data).map_err(VfsError::Io)?;
        std::fs::rename(&tmp_path, path).map_err(VfsError::Io)?;

        self.dirty.store(false, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// Returns true if the VFS has unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn mark_dirty(&self) {
        self.dirty.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    fn split_path(path: &str) -> Vec<&str> {
        path.split('/')
            .filter(|s| !s.is_empty())
            .collect()
    }

    pub async fn mkdir(&self, path: &str, owner: &str) -> VfsResult<()> {
        let segments = Self::split_path(path);
        if segments.is_empty() {
            return Err(VfsError::AlreadyExists("/".to_string()));
        }

        let mut root = self.root.write().await;
        let mut current = &mut *root;

        for (i, seg) in segments.iter().enumerate() {
            if i == segments.len() - 1 {
                if current.children.contains_key(*seg) {
                    return Err(VfsError::AlreadyExists(path.to_string()));
                }
                let node = VfsNode::new_dir(seg.to_string(), owner.to_string(), VfsPermissions::default_dir());
                current.children.insert(seg.to_string(), node);

                let mut journal = self.journal.write().await;
                journal.record(JournalEntry::mkdir(path.to_string(), owner.to_string()));
                self.mark_dirty();

                return Ok(());
            }

            if !current.children.contains_key(*seg) {
                let node = VfsNode::new_dir(seg.to_string(), owner.to_string(), VfsPermissions::default_dir());
                current.children.insert(seg.to_string(), node);
            }

            current = current.children.get_mut(*seg)
                .ok_or_else(|| VfsError::NotFound(seg.to_string()))?;

            if current.node_type != NodeType::Directory {
                return Err(VfsError::NotADirectory(seg.to_string()));
            }
        }

        Ok(())
    }

    pub async fn write_file(&self, path: &str, data: Vec<u8>, owner: &str) -> VfsResult<()> {
        let segments = Self::split_path(path);
        if segments.is_empty() {
            return Err(VfsError::NotAFile("/".to_string()));
        }

        let mut root = self.root.write().await;
        let mut current = &mut *root;

        for (i, seg) in segments.iter().enumerate() {
            if i == segments.len() - 1 {
                let node = VfsNode::new_file(
                    seg.to_string(),
                    owner.to_string(),
                    data.clone(),
                    VfsPermissions::default_file(),
                );
                current.children.insert(seg.to_string(), node);

                let mut journal = self.journal.write().await;
                journal.record(JournalEntry::write_file(path.to_string(), data.len(), owner.to_string()));
                self.mark_dirty();

                return Ok(());
            }

            if !current.children.contains_key(*seg) {
                let node = VfsNode::new_dir(seg.to_string(), owner.to_string(), VfsPermissions::default_dir());
                current.children.insert(seg.to_string(), node);
            }

            current = current.children.get_mut(*seg)
                .ok_or_else(|| VfsError::NotFound(seg.to_string()))?;

            if current.node_type != NodeType::Directory {
                return Err(VfsError::NotADirectory(seg.to_string()));
            }
        }

        Ok(())
    }

    pub async fn read_file(&self, path: &str) -> VfsResult<Vec<u8>> {
        let segments = Self::split_path(path);
        let root = self.root.read().await;
        let mut current = &*root;

        for (i, seg) in segments.iter().enumerate() {
            current = current.children.get(*seg)
                .ok_or_else(|| VfsError::NotFound(path.to_string()))?;

            if i == segments.len() - 1 {
                if current.node_type != NodeType::File {
                    return Err(VfsError::NotAFile(path.to_string()));
                }
                return Ok(current.data.clone());
            }

            if current.node_type != NodeType::Directory {
                return Err(VfsError::NotADirectory(seg.to_string()));
            }
        }

        Err(VfsError::NotFound(path.to_string()))
    }

    pub async fn list_dir(&self, path: &str) -> VfsResult<Vec<String>> {
        let segments = Self::split_path(path);
        let root = self.root.read().await;
        let mut current = &*root;

        if segments.is_empty() {
            return Ok(current.children.keys().cloned().collect());
        }

        for seg in &segments {
            current = current.children.get(*seg)
                .ok_or_else(|| VfsError::NotFound(path.to_string()))?;
            if current.node_type != NodeType::Directory {
                return Err(VfsError::NotADirectory(seg.to_string()));
            }
        }

        Ok(current.children.keys().cloned().collect())
    }

    /// Get metadata for a node at the given path.
    pub async fn stat(&self, path: &str) -> VfsResult<NodeInfo> {
        let segments = Self::split_path(path);
        let root = self.root.read().await;
        let mut current = &*root;

        if segments.is_empty() {
            return Ok(NodeInfo::from_node(current, "/"));
        }

        for seg in &segments {
            current = current.children.get(*seg)
                .ok_or_else(|| VfsError::NotFound(path.to_string()))?;
        }

        Ok(NodeInfo::from_node(current, path))
    }

    pub async fn remove(&self, path: &str) -> VfsResult<()> {
        let segments = Self::split_path(path);
        if segments.is_empty() {
            return Err(VfsError::PermissionDenied("Cannot remove root".to_string()));
        }

        let mut root = self.root.write().await;
        let mut current = &mut *root;

        for (i, seg) in segments.iter().enumerate() {
            if i == segments.len() - 1 {
                let node = current.children.get(*seg)
                    .ok_or_else(|| VfsError::NotFound(path.to_string()))?;

                if node.node_type == NodeType::Directory && !node.children.is_empty() {
                    return Err(VfsError::NotEmpty(path.to_string()));
                }

                current.children.remove(*seg);

                let mut journal = self.journal.write().await;
                journal.record(JournalEntry::remove(path.to_string()));
                self.mark_dirty();

                return Ok(());
            }

            current = current.children.get_mut(*seg)
                .ok_or_else(|| VfsError::NotFound(seg.to_string()))?;
        }

        Ok(())
    }

    pub async fn exists(&self, path: &str) -> bool {
        let segments = Self::split_path(path);
        let root = self.root.read().await;
        let mut current = &*root;

        for seg in &segments {
            match current.children.get(*seg) {
                Some(node) => current = node,
                None => return false,
            }
        }
        true
    }

    /// Bootstrap the default directory structure for a new system.
    pub async fn bootstrap_defaults(&self) -> VfsResult<()> {
        let dirs = [
            "/home",
            "/system",
            "/system/config",
            "/system/logs",
            "/tmp",
            "/apps",
        ];
        for d in &dirs {
            if !self.exists(d).await {
                self.mkdir(d, "root").await?;
            }
        }
        Ok(())
    }
}

impl Default for Vfs {
    fn default() -> Self {
        Self::new()
    }
}

/// Lightweight node metadata returned by stat().
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub path: String,
    pub node_type: NodeType,
    pub owner: String,
    pub size: u64,
    pub permissions: VfsPermissions,
    pub created_at: String,
    pub modified_at: String,
    pub children_count: usize,
}

impl NodeInfo {
    fn from_node(node: &VfsNode, path: &str) -> Self {
        Self {
            path: path.to_string(),
            node_type: node.node_type.clone(),
            owner: node.owner.clone(),
            size: node.size,
            permissions: node.permissions,
            created_at: node.created_at.to_rfc3339(),
            modified_at: node.modified_at.to_rfc3339(),
            children_count: node.children.len(),
        }
    }
}
