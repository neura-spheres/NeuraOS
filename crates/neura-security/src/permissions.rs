use neura_users::roles::Role;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PermissionError {
    #[error("Access denied: {0}")]
    AccessDenied(String),
    #[error("Insufficient role: required {required}, has {actual}")]
    InsufficientRole { required: String, actual: String },
}

pub type PermResult<T> = Result<T, PermissionError>;
pub struct PermissionCheck;

impl PermissionCheck {
    pub fn can_access_user_resource(actor_role: &Role, actor_username: &str, resource_owner: &str) -> PermResult<()> {
        if actor_username == resource_owner || actor_role.is_privileged() {
            Ok(())
        } else {
            Err(PermissionError::AccessDenied(
                format!("User '{}' cannot access resources of '{}'", actor_username, resource_owner)
            ))
        }
    }
    pub fn require_privileged(role: &Role) -> PermResult<()> {
        if role.is_privileged() {
            Ok(())
        } else {
            Err(PermissionError::InsufficientRole {
                required: "admin".to_string(),
                actual: role.to_string(),
            })
        }
    }
    pub fn can_install_packages(role: &Role) -> PermResult<()> {
        if role.can_install_packages() {
            Ok(())
        } else {
            Err(PermissionError::InsufficientRole {
                required: "user".to_string(),
                actual: role.to_string(),
            })
        }
    }
    pub fn can_access_ai(role: &Role) -> PermResult<()> {
        if role.can_access_ai() {
            Ok(())
        } else {
            Err(PermissionError::InsufficientRole {
                required: "user".to_string(),
                actual: role.to_string(),
            })
        }
    }
}
