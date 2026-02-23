use std::sync::Arc;
use neura_storage::vfs::Vfs;
use neura_storage::db::Database;
use neura_ai_core::provider::AiProvider;
use neura_ai_core::provider::types::ChatMessage;
use neura_users::roles::Role;
use neura_users::account::UserStore;
use serde_json;

/// Shared system context that shell commands can access.
pub struct ShellContext {
    pub vfs:          Arc<Vfs>,
    pub username:     String,
    pub hostname:     String,
    pub cwd:          String,

    /// The role of the currently-logged-in user.
    pub role:         Role,

    /// Access to the user database (for useradd/userlist/passwd).
    /// None only during early boot before auth completes.
    pub user_store:   Option<Arc<UserStore>>,
    
    /// System database for persistent memory
    pub db:           Option<Database>,

    pub ai_client:       Option<Arc<dyn AiProvider>>,
    pub ai_temperature:  f32,
    pub ai_max_tokens:   u32,
    pub ai_history:      Vec<ChatMessage>,

    pub command_history: Vec<String>,
    pub history_cursor:  usize,
    pub max_history:     usize,

    /// Timezone offset in minutes from UTC (from desktop.timezone setting).
    pub timezone_offset_mins: i32,
    /// Human-readable timezone label (e.g. "Jakarta / Medan (WIB)").
    pub timezone_label: String,
}

impl ShellContext {
    pub fn new(vfs: Arc<Vfs>, username: &str, hostname: &str) -> Self {
        let home = format!("/home/{}", username);
        Self {
            vfs,
            username:        username.to_string(),
            hostname:        hostname.to_string(),
            cwd:             home,
            role:            Role::Guest,   // overridden after login
            user_store:      None,
            db:              None,
            ai_client:       None,
            ai_temperature:  0.7,
            ai_max_tokens:   4096,
            ai_history:      Vec::new(),
            command_history:      Vec::new(),
            history_cursor:       0,
            max_history:          1000,
            timezone_offset_mins: 0,
            timezone_label:       "UTC".to_string(),
        }
    }

    // ── Permission helpers ────────────────────────────────────────────────────

    /// Returns Ok(()) if the current user can access the given VFS path.
    /// Admin / Root users can access any path.
    /// All other roles are restricted to their own home directory.
    pub fn check_path_access(&self, path: &str) -> Result<(), String> {
        if self.role.is_privileged() {
            return Ok(());
        }
        let home = format!("/home/{}", self.username);
        if path == home || path.starts_with(&format!("{}/", home)) {
            Ok(())
        } else {
            Err(format!(
                "Permission denied: '{}' is outside your home directory. \
                 Only administrators can access other paths.",
                path
            ))
        }
    }

    // ── AI client ─────────────────────────────────────────────────────────────

    pub fn set_ai_client(&mut self, client: Arc<dyn AiProvider>) {
        self.ai_client = Some(client);
    }

    // ── History ───────────────────────────────────────────────────────────────

    pub fn push_history(&mut self, cmd: &str) {
        if !cmd.is_empty() {
            self.command_history.push(cmd.to_string());
            if self.command_history.len() > self.max_history {
                let excess = self.command_history.len() - self.max_history;
                self.command_history.drain(..excess);
            }
        }
        self.history_cursor = self.command_history.len();
    }

    pub fn history_prev(&mut self) -> Option<&str> {
        if self.history_cursor > 0 {
            self.history_cursor -= 1;
            self.command_history.get(self.history_cursor).map(|s| s.as_str())
        } else {
            None
        }
    }

    pub fn history_next(&mut self) -> Option<&str> {
        if self.history_cursor < self.command_history.len() {
            self.history_cursor += 1;
            if self.history_cursor < self.command_history.len() {
                Some(&self.command_history[self.history_cursor])
            } else {
                Some("")
            }
        } else {
            None
        }
    }

    // ── Path resolution ───────────────────────────────────────────────────────

    pub fn resolve_path(&self, path: &str) -> String {
        if path.starts_with('/') {
            path.to_string()
        } else if path == "~" {
            format!("/home/{}", self.username)
        } else if path.starts_with("~/") {
            format!("/home/{}/{}", self.username, &path[2..])
        } else if path == ".." {
            let mut parts: Vec<&str> = self.cwd.split('/').filter(|s| !s.is_empty()).collect();
            parts.pop();
            if parts.is_empty() { "/".to_string() } else { format!("/{}", parts.join("/")) }
        } else if path == "." {
            self.cwd.clone()
        } else if self.cwd == "/" {
            format!("/{}", path)
        } else {
            format!("{}/{}", self.cwd, path)
        }
    }

    // ── Prompt ────────────────────────────────────────────────────────────────

    pub fn prompt(&self) -> String {
        let display_cwd = if self.cwd == format!("/home/{}", self.username) {
            "~".to_string()
        } else {
            self.cwd.clone()
        };
        // '#' for admins/root, '$' for everyone else — classic Unix convention
        let sigil = if self.role.is_privileged() { "#" } else { "$" };
        format!("{}@{} {} {}", self.username, self.hostname, display_cwd, sigil)
    }

    // ── AI History Persistence ───────────────────────────────────────────────

    pub async fn load_ai_history(&mut self) {
        let path = format!("/home/{}/.ai_history.json", self.username);
        if let Ok(data) = self.vfs.read_file(&path).await {
            if let Ok(history) = serde_json::from_slice::<Vec<ChatMessage>>(&data) {
                self.ai_history = history;
            }
        }
    }

    pub async fn save_ai_history(&self) {
        let path = format!("/home/{}/.ai_history.json", self.username);
        if let Ok(data) = serde_json::to_vec_pretty(&self.ai_history) {
            let _ = self.vfs.write_file(&path, data, &self.username).await;
        }
    }
}
