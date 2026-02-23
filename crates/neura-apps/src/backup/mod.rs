use std::any::Any;
use std::sync::Arc;
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui::style::{Style, Modifier};
use crossterm::event::{KeyCode, KeyEvent};
use serde_json::Value;
use chrono::Utc;
use neura_app_framework::app_trait::App;
use neura_app_framework::palette::*;
use neura_storage::vfs::Vfs;

#[derive(Debug, Clone)]
struct BackupEntry {
    name: String,
    created_at: String,
    size_kb: f32,
    path: String,
}

#[derive(Debug, Clone, PartialEq)]
enum View {
    Main,
    Confirm(ConfirmAction),
    Progress(String),
    Done(String),
    Error(String),
}

#[derive(Debug, Clone, PartialEq)]
enum ConfirmAction {
    CreateBackup,
    RestoreBackup(usize),
    DeleteBackup(usize),
}

pub struct BackupApp {
    vfs: Arc<Vfs>,
    username: String,
    backups: Vec<BackupEntry>,
    selected: usize,
    view: View,
    _scroll: usize,
    needs_scan: bool,
    pub status_msg: String,
}

impl BackupApp {
    pub fn new(vfs: Arc<Vfs>, username: &str) -> Self {
        Self {
            vfs,
            username: username.to_string(),
            backups: Vec::new(),
            selected: 0,
            view: View::Main,
            _scroll: 0,
            needs_scan: true,
            status_msg: "Press [n] to create a backup of your VFS data.".to_string(),
        }
    }

    pub fn needs_scan(&self) -> bool { self.needs_scan }

    pub async fn async_scan(&mut self) {
        self.needs_scan = false;
        let backup_dir = format!("/home/{}/backups", self.username);
        self.backups.clear();

        if let Ok(entries) = self.vfs.list_dir(&backup_dir).await {
            let mut entries = entries;
            entries.sort();
            entries.reverse(); // newest first
            for name in entries {
                let path = format!("{}/{}", backup_dir, name);
                if let Ok(info) = self.vfs.stat(&path).await {
                    self.backups.push(BackupEntry {
                        name: name.clone(),
                        created_at: info.modified_at.clone(),
                        size_kb: info.size as f32 / 1024.0,
                        path: path.clone(),
                    });
                }
            }
        }
    }

    pub async fn async_create_backup(&mut self) {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let backup_name = format!("backup_{}.json", timestamp);
        let backup_dir = format!("/home/{}/backups", self.username);
        let backup_path = format!("{}/{}", backup_dir, backup_name);

        // Snapshot the entire user home directory
        let home = format!("/home/{}", self.username);
        let mut snapshot: serde_json::Map<String, Value> = serde_json::Map::new();
        snapshot.insert("created_at".to_string(), Value::String(Utc::now().to_rfc3339()));
        snapshot.insert("user".to_string(), Value::String(self.username.clone()));

        // Walk the home directory and collect all files
        let mut files_snapshot: serde_json::Map<String, Value> = serde_json::Map::new();
        self.walk_and_snapshot(&home, &mut files_snapshot).await;
        snapshot.insert("files".to_string(), Value::Object(files_snapshot));

        let snapshot_json = serde_json::Value::Object(snapshot);

        let _ = self.vfs.mkdir(&backup_dir, &self.username).await;
        if let Ok(data) = serde_json::to_vec_pretty(&snapshot_json) {
            match self.vfs.write_file(&backup_path, data, &self.username).await {
                Ok(()) => {
                    self.status_msg = format!("Backup created: {}", backup_name);
                    self.needs_scan = true;
                    self.view = View::Done(format!("Backup '{}' created successfully!", backup_name));
                }
                Err(e) => {
                    self.view = View::Error(format!("Backup failed: {}", e));
                }
            }
        }
    }

    async fn walk_and_snapshot(&self, path: &str, out: &mut serde_json::Map<String, Value>) {
        if let Ok(entries) = self.vfs.list_dir(path).await {
            for name in entries {
                let full = format!("{}/{}", path, name);
                // Skip the backups directory itself
                if full.contains("/backups") { continue; }
                if let Ok(data) = self.vfs.read_file(&full).await {
                    let content = String::from_utf8_lossy(&data).to_string();
                    out.insert(full, Value::String(content));
                }
            }
        }
    }

    pub async fn async_restore_backup(&mut self, idx: usize) {
        let backup = match self.backups.get(idx) {
            Some(b) => b.clone(),
            None => return,
        };

        match self.vfs.read_file(&backup.path).await {
            Ok(data) => {
                if let Ok(snapshot) = serde_json::from_slice::<Value>(&data) {
                    if let Some(files) = snapshot.get("files").and_then(|v| v.as_object()) {
                        let mut restored = 0;
                        for (path, content) in files {
                            if let Some(text) = content.as_str() {
                                if let Some(parent_end) = path.rfind('/') {
                                    let parent = &path[..parent_end];
                                    let _ = self.vfs.mkdir(parent, &self.username).await;
                                }
                                if self.vfs.write_file(path, text.as_bytes().to_vec(), &self.username).await.is_ok() {
                                    restored += 1;
                                }
                            }
                        }
                        self.view = View::Done(format!("Restored {} files from '{}'.", restored, backup.name));
                        self.status_msg = format!("Restore complete: {} files", restored);
                    }
                } else {
                    self.view = View::Error("Backup file is corrupted.".to_string());
                }
            }
            Err(e) => {
                self.view = View::Error(format!("Restore failed: {}", e));
            }
        }
    }

    pub async fn async_delete_backup(&mut self, idx: usize) {
        let path = match self.backups.get(idx) {
            Some(b) => b.path.clone(),
            None => return,
        };
        match self.vfs.remove(&path).await {
            Ok(()) => {
                self.backups.remove(idx);
                if self.selected > 0 && self.selected >= self.backups.len() {
                    self.selected = self.backups.len().saturating_sub(1);
                }
                self.status_msg = "Backup deleted.".to_string();
                self.view = View::Main;
            }
            Err(e) => {
                self.view = View::Error(format!("Delete failed: {}", e));
            }
        }
    }
}

impl App for BackupApp {
    fn id(&self) -> &str { "backup" }
    fn name(&self) -> &str { "NeuraBackup" }

    fn init(&mut self) -> anyhow::Result<()> { Ok(()) }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match &self.view.clone() {
            View::Main => {
                match key.code {
                    KeyCode::Esc => return false,
                    KeyCode::Char('n') => {
                        self.view = View::Confirm(ConfirmAction::CreateBackup);
                    }
                    KeyCode::Char('r') if !self.backups.is_empty() => {
                        self.view = View::Confirm(ConfirmAction::RestoreBackup(self.selected));
                    }
                    KeyCode::Char('d') if !self.backups.is_empty() => {
                        self.view = View::Confirm(ConfirmAction::DeleteBackup(self.selected));
                    }
                    KeyCode::Char('R') => {
                        self.needs_scan = true;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if self.selected > 0 { self.selected -= 1; }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if self.selected + 1 < self.backups.len() { self.selected += 1; }
                    }
                    _ => {}
                }
                true
            }
            View::Confirm(_) => {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Enter => {
                        let action = match &self.view {
                            View::Confirm(a) => a.clone(),
                            _ => return true,
                        };
                        match action {
                            ConfirmAction::CreateBackup => {
                                self.view = View::Progress("Creating backup...".to_string());
                                // Signal async needed
                                self.status_msg = "__CREATE_BACKUP__".to_string();
                            }
                            ConfirmAction::RestoreBackup(idx) => {
                                self.view = View::Progress("Restoring...".to_string());
                                self.status_msg = format!("__RESTORE__:{}", idx);
                            }
                            ConfirmAction::DeleteBackup(idx) => {
                                self.view = View::Progress("Deleting...".to_string());
                                self.status_msg = format!("__DELETE__:{}", idx);
                            }
                        }
                        true
                    }
                    KeyCode::Char('n') | KeyCode::Esc => {
                        self.view = View::Main;
                        true
                    }
                    _ => true,
                }
            }
            View::Done(_) | View::Error(_) => {
                match key.code {
                    KeyCode::Enter | KeyCode::Esc | KeyCode::Char(' ') => {
                        self.view = View::Main;
                    }
                    _ => {}
                }
                true
            }
            View::Progress(_) => {
                // Block input while working
                if key.code == KeyCode::Esc { return false; }
                true
            }
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        match &self.view {
            View::Main => self.render_main(frame, area),
            View::Confirm(action) => self.render_confirm(frame, area, action),
            View::Progress(msg) => self.render_progress(frame, area, msg),
            View::Done(msg) => self.render_done(frame, area, msg),
            View::Error(msg) => self.render_error(frame, area, msg),
        }
    }

    fn on_resume(&mut self) { self.needs_scan = true; }
    fn on_pause(&mut self) {}
    fn on_close(&mut self) {}

    fn save_state(&self) -> Option<Value> { None }
    fn load_state(&mut self, _state: Value) {}

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

impl BackupApp {
    fn render_main(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(5), Constraint::Length(1)])
            .split(area);

        // Header
        let header = Paragraph::new(format!("  NeuraBackup  |  {} backups on record  |  {}", self.backups.len(), &self.status_msg))
            .style(Style::default().fg(PRIMARY))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(BORDER)).title_style(Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)));
        frame.render_widget(header, chunks[0]);

        // Backup list
        let mut items: Vec<ListItem> = Vec::new();
        if self.backups.is_empty() {
            items.push(ListItem::new("  No backups yet. Press [n] to create one.").style(Style::default().fg(DIM)));
            items.push(ListItem::new("").style(Style::default().fg(DIM)));
            items.push(ListItem::new("  Backups capture all your notes, tasks, contacts,").style(Style::default().fg(MUTED)));
            items.push(ListItem::new("  settings, and files stored in the VFS.").style(Style::default().fg(MUTED)));
        } else {
            for (i, b) in self.backups.iter().enumerate() {
                let is_sel = i == self.selected;
                let prefix = if is_sel { "▸ " } else { "  " };
                let style = if is_sel {
                    Style::default().fg(GREEN).bg(SEL_BG).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(TEXT)
                };
                items.push(ListItem::new(format!(
                    "{}  {:<30}  {:.1} KB  {}",
                    prefix, b.name, b.size_kb, &b.created_at[..19]
                )).style(style));
            }
        }

        let list = List::new(items)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BORDER))
                .title(" Backup History ")
                .title_style(Style::default().fg(PRIMARY)));
        frame.render_widget(list, chunks[1]);

        let help = Paragraph::new("  [n] create  [r] restore  [d] delete  [R] refresh  [Esc] back")
            .style(Style::default().fg(MUTED));
        frame.render_widget(help, chunks[2]);
    }

    fn render_confirm(&self, frame: &mut Frame, area: Rect, action: &ConfirmAction) {
        let msg = match action {
            ConfirmAction::CreateBackup => "Create a new backup of all your VFS data?".to_string(),
            ConfirmAction::RestoreBackup(idx) => {
                let name = self.backups.get(*idx).map(|b| b.name.as_str()).unwrap_or("selected backup");
                format!("Restore from '{}'? This will overwrite current files.", name)
            }
            ConfirmAction::DeleteBackup(idx) => {
                let name = self.backups.get(*idx).map(|b| b.name.as_str()).unwrap_or("selected backup");
                format!("Delete backup '{}'? This cannot be undone.", name)
            }
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(ORANGE))
            .title(" Confirm ")
            .title_style(Style::default().fg(ORANGE).add_modifier(Modifier::BOLD));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let text = format!("\n\n\n  {}\n\n  [y/Enter] Yes   [n/Esc] No", msg);
        frame.render_widget(Paragraph::new(text).style(Style::default().fg(TEXT)).alignment(Alignment::Center), inner);
    }

    fn render_progress(&self, frame: &mut Frame, area: Rect, msg: &str) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(CYAN))
            .title(" Working... ")
            .title_style(Style::default().fg(PRIMARY));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let text = format!("\n\n\n  ⟳ {}\n\n  Please wait...", msg);
        frame.render_widget(Paragraph::new(text).style(Style::default().fg(ORANGE)).alignment(Alignment::Center), inner);
    }

    fn render_done(&self, frame: &mut Frame, area: Rect, msg: &str) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(GREEN))
            .title(" Done ")
            .title_style(Style::default().fg(GREEN).add_modifier(Modifier::BOLD));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let text = format!("\n\n\n  ✓ {}\n\n  [Enter/Esc] back", msg);
        frame.render_widget(Paragraph::new(text).style(Style::default().fg(GREEN)).alignment(Alignment::Center), inner);
    }

    fn render_error(&self, frame: &mut Frame, area: Rect, msg: &str) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(RED))
            .title(" Error ")
            .title_style(Style::default().fg(RED).add_modifier(Modifier::BOLD));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let text = format!("\n\n\n  ✗ {}\n\n  [Enter/Esc] back", msg);
        frame.render_widget(Paragraph::new(text).style(Style::default().fg(RED)).alignment(Alignment::Center), inner);
    }
}

