use chrono::Utc;

pub struct StatusBar {
    pub hostname: String,
    pub username: String,
    pub workspace: String,
}

impl StatusBar {
    pub fn new(hostname: &str, username: &str) -> Self {
        Self {
            hostname: hostname.to_string(),
            username: username.to_string(),
            workspace: "main".to_string(),
        }
    }

    pub fn render_text(&self) -> String {
        let time = Utc::now().format("%H:%M:%S");
        format!(
            " {} | {} @ {} | {} ",
            self.workspace,
            self.username,
            self.hostname,
            time,
        )
    }
}
