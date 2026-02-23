use std::any::Any;
use std::sync::Arc;
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui::style::{Style, Modifier};
use crossterm::event::{KeyCode, KeyEvent};
use serde::{Serialize, Deserialize};
use serde_json::Value;
use chrono::Utc;
use neura_app_framework::app_trait::App;
use neura_app_framework::palette::*;
use neura_storage::vfs::Vfs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub title: String,
    pub content: String,
    pub created_at: String,
    pub modified_at: String,
}

#[derive(Debug, Clone, PartialEq)]
enum Mode {
    List,
    Editing,
    NewTitle,
}

pub struct NotesApp {
    vfs: Arc<Vfs>,
    username: String,
    notes: Vec<Note>,
    selected: usize,
    mode: Mode,
    edit_buffer: String,
    edit_cursor: usize,
    title_buffer: String,
    initialized: bool,
}

impl NotesApp {
    pub fn new(vfs: Arc<Vfs>, username: &str) -> Self {
        Self {
            vfs,
            username: username.to_string(),
            notes: Vec::new(),
            selected: 0,
            mode: Mode::List,
            edit_buffer: String::new(),
            edit_cursor: 0,
            title_buffer: String::new(),
            initialized: false,
        }
    }

    fn notes_dir(&self) -> String {
        format!("/home/{}/notes", self.username)
    }

    async fn _load_notes_async(vfs: &Vfs, dir: &str) -> Vec<Note> {
        let mut notes = Vec::new();
        if let Ok(entries) = vfs.list_dir(dir).await {
            let mut sorted = entries;
            sorted.sort();
            for name in sorted {
                if !name.ends_with(".notes") { continue; }
                let path = format!("{}/{}", dir, name);
                if let Ok(data) = vfs.read_file(&path).await {
                    if let Ok(note) = serde_json::from_slice::<Note>(&data) {
                        notes.push(note);
                    }
                }
            }
        }
        notes
    }

    async fn save_note_async(vfs: &Vfs, dir: &str, note: &Note, username: &str) {
        let filename = note.title.replace(' ', "_").to_lowercase();
        let path = format!("{}/{}.notes", dir, filename);
        if let Ok(data) = serde_json::to_vec_pretty(note) {
            let _ = vfs.write_file(&path, data, username).await;
        }
    }

    async fn delete_note_async(vfs: &Vfs, dir: &str, note: &Note) {
        let filename = note.title.replace(' ', "_").to_lowercase();
        let path = format!("{}/{}.notes", dir, filename);
        let _ = vfs.remove(&path).await;
    }
}

impl App for NotesApp {
    fn id(&self) -> &str { "notes" }
    fn name(&self) -> &str { "NeuraNotes" }

    fn init(&mut self) -> anyhow::Result<()> {
        if !self.initialized {
            // We can't do async here, so we'll load on first render via a sync check
            self.initialized = true;
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match self.mode {
            Mode::List => {
                match key.code {
                    KeyCode::Char('n') => {
                        self.mode = Mode::NewTitle;
                        self.title_buffer.clear();
                        true
                    }
                    KeyCode::Char('d') if !self.notes.is_empty() => {
                        let vfs = self.vfs.clone();
                        let dir = self.notes_dir();
                        let note = self.notes.remove(self.selected);
                        if self.selected > 0 && self.selected >= self.notes.len() {
                            self.selected = self.notes.len().saturating_sub(1);
                        }
                        tokio::spawn(async move {
                            Self::delete_note_async(&vfs, &dir, &note).await;
                        });
                        true
                    }
                    KeyCode::Enter if !self.notes.is_empty() => {
                        self.edit_buffer = self.notes[self.selected].content.clone();
                        self.edit_cursor = self.edit_buffer.len();
                        self.mode = Mode::Editing;
                        true
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if self.selected > 0 {
                            self.selected -= 1;
                        }
                        true
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if self.selected + 1 < self.notes.len() {
                            self.selected += 1;
                        }
                        true
                    }
                    KeyCode::Esc => false, // Let desktop handle Esc
                    _ => true,
                }
            }
            Mode::NewTitle => {
                match key.code {
                    KeyCode::Enter => {
                        if !self.title_buffer.is_empty() {
                            let now = Utc::now().to_rfc3339();
                            let note = Note {
                                title: self.title_buffer.clone(),
                                content: String::new(),
                                created_at: now.clone(),
                                modified_at: now,
                            };
                            self.notes.push(note);
                            self.selected = self.notes.len() - 1;
                            self.edit_buffer.clear();
                            self.edit_cursor = 0;
                            self.mode = Mode::Editing;

                            // Save
                            let vfs = self.vfs.clone();
                            let dir = self.notes_dir();
                            let note = self.notes[self.selected].clone();
                            let username = self.username.clone();
                            tokio::spawn(async move {
                                let _ = vfs.mkdir(&dir, &username).await;
                                Self::save_note_async(&vfs, &dir, &note, &username).await;
                            });
                        }
                        true
                    }
                    KeyCode::Esc => {
                        self.mode = Mode::List;
                        true
                    }
                    KeyCode::Char(c) => {
                        self.title_buffer.push(c);
                        true
                    }
                    KeyCode::Backspace => {
                        self.title_buffer.pop();
                        true
                    }
                    _ => true,
                }
            }
            Mode::Editing => {
                match key.code {
                    KeyCode::Esc => {
                        // Save and return to list
                        let dir = self.notes_dir();
                        let vfs = self.vfs.clone();
                        let username = self.username.clone();
                        if let Some(note) = self.notes.get_mut(self.selected) {
                            note.content = self.edit_buffer.clone();
                            note.modified_at = Utc::now().to_rfc3339();
                            let note = note.clone();
                            tokio::spawn(async move {
                                Self::save_note_async(&vfs, &dir, &note, &username).await;
                            });
                        }
                        self.mode = Mode::List;
                        true
                    }
                    KeyCode::Char(c) => {
                        self.edit_buffer.insert(self.edit_cursor, c);
                        self.edit_cursor += 1;
                        true
                    }
                    KeyCode::Enter => {
                        self.edit_buffer.insert(self.edit_cursor, '\n');
                        self.edit_cursor += 1;
                        true
                    }
                    KeyCode::Backspace => {
                        if self.edit_cursor > 0 {
                            self.edit_cursor -= 1;
                            self.edit_buffer.remove(self.edit_cursor);
                        }
                        true
                    }
                    KeyCode::Left => {
                        if self.edit_cursor > 0 { self.edit_cursor -= 1; }
                        true
                    }
                    KeyCode::Right => {
                        if self.edit_cursor < self.edit_buffer.len() { self.edit_cursor += 1; }
                        true
                    }
                    _ => true,
                }
            }
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        match self.mode {
            Mode::List => self.render_list(frame, area),
            Mode::NewTitle => self.render_new_title(frame, area),
            Mode::Editing => self.render_editor(frame, area),
        }
    }

    fn on_close(&mut self) {}

    fn save_state(&self) -> Option<Value> {
        serde_json::to_value(&self.notes).ok()
    }

    fn load_state(&mut self, state: Value) {
        if let Ok(notes) = serde_json::from_value(state) {
            self.notes = notes;
        }
    }

    fn ai_tools(&self) -> Vec<Value> {
        vec![serde_json::json!({
            "name": "create_note",
            "description": "Create a new note in NeuraNotes",
            "parameters": {
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Note title" },
                    "content": { "type": "string", "description": "Note content" }
                },
                "required": ["title", "content"]
            }
        })]
    }

    fn handle_ai_tool(&mut self, tool_name: &str, args: Value) -> Option<Value> {
        match tool_name {
            "create_note" => {
                let title = args.get("title")?.as_str()?.to_string();
                let content = args.get("content")?.as_str()?.to_string();
                let now = Utc::now().to_rfc3339();
                self.notes.push(Note {
                    title: title.clone(),
                    content,
                    created_at: now.clone(),
                    modified_at: now,
                });
                Some(serde_json::json!({"status": "created", "title": title}))
            }
            _ => None,
        }
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

impl NotesApp {
    fn render_list(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(2)])
            .split(area);

        let items: Vec<ListItem> = self.notes.iter().enumerate().map(|(i, note)| {
            let style = if i == self.selected {
                Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT)
            };
            let prefix = if i == self.selected { "▸ " } else { "  " };
            ListItem::new(format!("{}{}", prefix, note.title)).style(style)
        }).collect();

        let list = List::new(items)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BORDER))
                .title(" NeuraNotes ")
                .title_style(Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)));
        frame.render_widget(list, chunks[0]);

        let help = Paragraph::new(" [n]ew  [Enter]edit  [d]elete  [Esc]back")
            .style(Style::default().fg(MUTED));
        frame.render_widget(help, chunks[1]);
    }

    fn render_new_title(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(PRIMARY))
            .title(" New Note - Enter Title ")
            .title_style(Style::default().fg(ORANGE).add_modifier(Modifier::BOLD));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let text = Paragraph::new(format!("> {}", self.title_buffer))
            .style(Style::default().fg(GREEN));
        frame.render_widget(text, inner);
    }

    fn render_editor(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(3), Constraint::Length(2)])
            .split(area);

        // Title bar
        let title = if let Some(note) = self.notes.get(self.selected) {
            format!(" Editing: {} ", note.title)
        } else {
            " Editing ".to_string()
        };
        let title_bar = Paragraph::new(title)
            .style(Style::default().fg(ORANGE).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(BORDER)));
        frame.render_widget(title_bar, chunks[0]);

        // Editor area
        let editor = Paragraph::new(self.edit_buffer.as_str())
            .style(Style::default().fg(TEXT))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(BORDER)).title(" Content "));
        frame.render_widget(editor, chunks[1]);

        // Help
        let help = Paragraph::new(" [Esc] save & back  Type to edit")
            .style(Style::default().fg(MUTED));
        frame.render_widget(help, chunks[2]);
    }
}
