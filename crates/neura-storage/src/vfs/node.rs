use std::collections::HashMap;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use uuid::Uuid;

use super::permissions::VfsPermissions;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeType {
    File,
    Directory,
    Symlink(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VfsNode {
    pub id: String,
    pub name: String,
    pub node_type: NodeType,
    pub owner: String,
    pub group: String,
    pub permissions: VfsPermissions,
    pub data: Vec<u8>,
    pub children: HashMap<String, VfsNode>,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub size: u64,
    pub locked_by: Option<String>,
}

impl VfsNode {
    pub fn new_dir(name: String, owner: String, permissions: VfsPermissions) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            node_type: NodeType::Directory,
            owner: owner.clone(),
            group: owner,
            permissions,
            data: Vec::new(),
            children: HashMap::new(),
            created_at: now,
            modified_at: now,
            size: 0,
            locked_by: None,
        }
    }

    pub fn new_file(name: String, owner: String, data: Vec<u8>, permissions: VfsPermissions) -> Self {
        let now = Utc::now();
        let size = data.len() as u64;
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            node_type: NodeType::File,
            owner: owner.clone(),
            group: owner,
            permissions,
            data,
            children: HashMap::new(),
            created_at: now,
            modified_at: now,
            size,
            locked_by: None,
        }
    }

    pub fn is_locked(&self) -> bool {
        self.locked_by.is_some()
    }
}
