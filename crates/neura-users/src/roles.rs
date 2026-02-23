use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Root,
    Admin,
    User,
    Guest,
}

impl Role {
    pub fn can_manage_users(&self) -> bool {
        matches!(self, Role::Root | Role::Admin)
    }

    pub fn can_install_packages(&self) -> bool {
        matches!(self, Role::Root | Role::Admin | Role::User)
    }

    pub fn can_access_ai(&self) -> bool {
        matches!(self, Role::Root | Role::Admin | Role::User)
    }

    pub fn can_modify_system_config(&self) -> bool {
        matches!(self, Role::Root | Role::Admin)
    }

    pub fn can_view_all_processes(&self) -> bool {
        matches!(self, Role::Root | Role::Admin)
    }

    pub fn is_privileged(&self) -> bool {
        matches!(self, Role::Root | Role::Admin)
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::Root => write!(f, "root"),
            Role::Admin => write!(f, "admin"),
            Role::User => write!(f, "user"),
            Role::Guest => write!(f, "guest"),
        }
    }
}

impl std::str::FromStr for Role {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "root" => Ok(Role::Root),
            "admin" => Ok(Role::Admin),
            "user" => Ok(Role::User),
            "guest" => Ok(Role::Guest),
            _ => Err(format!("Unknown role: {}", s)),
        }
    }
}
