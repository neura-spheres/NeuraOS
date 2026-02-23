use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct VfsPermissions {
    pub owner_read: bool,
    pub owner_write: bool,
    pub owner_execute: bool,
    pub group_read: bool,
    pub group_write: bool,
    pub group_execute: bool,
    pub other_read: bool,
    pub other_write: bool,
    pub other_execute: bool,
}

impl VfsPermissions {
    pub fn default_file() -> Self {
        Self {
            owner_read: true,
            owner_write: true,
            owner_execute: false,
            group_read: true,
            group_write: false,
            group_execute: false,
            other_read: false,
            other_write: false,
            other_execute: false,
        }
    }

    pub fn default_dir() -> Self {
        Self {
            owner_read: true,
            owner_write: true,
            owner_execute: true,
            group_read: true,
            group_write: false,
            group_execute: true,
            other_read: false,
            other_write: false,
            other_execute: false,
        }
    }

    /// Check if a given role can read.
    pub fn can_read(&self, is_owner: bool, is_group: bool) -> bool {
        if is_owner { self.owner_read }
        else if is_group { self.group_read }
        else { self.other_read }
    }

    /// Check if a given role can write.
    pub fn can_write(&self, is_owner: bool, is_group: bool) -> bool {
        if is_owner { self.owner_write }
        else if is_group { self.group_write }
        else { self.other_write }
    }

    /// Check if a given role can execute.
    pub fn can_execute(&self, is_owner: bool, is_group: bool) -> bool {
        if is_owner { self.owner_execute }
        else if is_group { self.group_execute }
        else { self.other_execute }
    }

    /// Octal representation (e.g. 0o755).
    pub fn to_octal(&self) -> u16 {
        let mut mode = 0u16;
        if self.owner_read { mode |= 0o400; }
        if self.owner_write { mode |= 0o200; }
        if self.owner_execute { mode |= 0o100; }
        if self.group_read { mode |= 0o040; }
        if self.group_write { mode |= 0o020; }
        if self.group_execute { mode |= 0o010; }
        if self.other_read { mode |= 0o004; }
        if self.other_write { mode |= 0o002; }
        if self.other_execute { mode |= 0o001; }
        mode
    }
}
