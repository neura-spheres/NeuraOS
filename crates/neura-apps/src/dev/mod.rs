use std::any::Any;
use std::collections::HashSet;
use std::sync::Arc;
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui::style::{Style, Modifier};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::Value;
use chrono::Utc;
use neura_app_framework::app_trait::App;
use neura_app_framework::palette::*;
use neura_storage::vfs::{Vfs, NodeType};

#[derive(Debug, Clone, PartialEq)]
enum Mode {
    FileTree,
    Editor,
    NewFile,
    NewFolder,
    SaveAs,
    _Command,
}

#[derive(Debug, Clone)]
struct TreeEntry {
    name: String,
    path: String,
    depth: usize,
    is_dir: bool,
    expanded: bool,
}

#[derive(Debug, Clone)]
struct Buffer {
    path: Option<String>,
    name: String,
    lines: Vec<String>,
    cursor_line: usize,
    cursor_col: usize,
    scroll: usize,
    modified: bool,
}

impl Buffer {
    fn new_empty() -> Self {
        Self {
            path: None,
            name: "untitled".to_string(),
            lines: vec![String::new()],
            cursor_line: 0,
            cursor_col: 0,
            scroll: 0,
            modified: false,
        }
    }

    fn from_content(path: &str, content: &str) -> Self {
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        let lines: Vec<String> = if content.is_empty() {
            vec![String::new()]
        } else {
            content.split('\n').map(|l| l.to_string()).collect()
        };
        Self {
            path: Some(path.to_string()),
            name,
            lines,
            cursor_line: 0,
            cursor_col: 0,
            scroll: 0,
            modified: false,
        }
    }

    fn content(&self) -> String {
        self.lines.join("\n")
    }

    fn insert_char(&mut self, c: char) {
        if self.cursor_line >= self.lines.len() {
            self.lines.push(String::new());
        }
        let line = &mut self.lines[self.cursor_line];
        line.insert(self.cursor_col, c);
        self.cursor_col += 1;
        self.modified = true;
    }

    fn insert_newline(&mut self) {
        let rest = if self.cursor_col <= self.lines[self.cursor_line].len() {
            self.lines[self.cursor_line][self.cursor_col..].to_string()
        } else {
            String::new()
        };
        self.lines[self.cursor_line].truncate(self.cursor_col);
        self.cursor_line += 1;
        self.lines.insert(self.cursor_line, rest);
        self.cursor_col = 0;
        self.modified = true;
    }

    fn delete_char(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
            self.lines[self.cursor_line].remove(self.cursor_col);
            self.modified = true;
        } else if self.cursor_line > 0 {
            let current = self.lines.remove(self.cursor_line);
            self.cursor_line -= 1;
            self.cursor_col = self.lines[self.cursor_line].len();
            self.lines[self.cursor_line].push_str(&current);
            self.modified = true;
        }
    }

    fn delete_forward(&mut self) {
        let line_len = self.lines[self.cursor_line].len();
        if self.cursor_col < line_len {
            self.lines[self.cursor_line].remove(self.cursor_col);
            self.modified = true;
        } else if self.cursor_line + 1 < self.lines.len() {
            let next = self.lines.remove(self.cursor_line + 1);
            self.lines[self.cursor_line].push_str(&next);
            self.modified = true;
        }
    }

    fn move_cursor(&mut self, dir: KeyCode, editor_height: usize) {
        match dir {
            KeyCode::Left => {
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                } else if self.cursor_line > 0 {
                    self.cursor_line -= 1;
                    self.cursor_col = self.lines[self.cursor_line].len();
                }
            }
            KeyCode::Right => {
                let len = self.lines[self.cursor_line].len();
                if self.cursor_col < len {
                    self.cursor_col += 1;
                } else if self.cursor_line + 1 < self.lines.len() {
                    self.cursor_line += 1;
                    self.cursor_col = 0;
                }
            }
            KeyCode::Up => {
                if self.cursor_line > 0 {
                    self.cursor_line -= 1;
                    self.cursor_col = self.cursor_col.min(self.lines[self.cursor_line].len());
                }
            }
            KeyCode::Down => {
                if self.cursor_line + 1 < self.lines.len() {
                    self.cursor_line += 1;
                    self.cursor_col = self.cursor_col.min(self.lines[self.cursor_line].len());
                }
            }
            KeyCode::Home => { self.cursor_col = 0; }
            KeyCode::End => { self.cursor_col = self.lines[self.cursor_line].len(); }
            KeyCode::PageUp => {
                self.cursor_line = self.cursor_line.saturating_sub(editor_height);
                self.cursor_col = self.cursor_col.min(self.lines[self.cursor_line].len());
            }
            KeyCode::PageDown => {
                self.cursor_line = (self.cursor_line + editor_height).min(self.lines.len().saturating_sub(1));
                self.cursor_col = self.cursor_col.min(self.lines[self.cursor_line].len());
            }
            _ => {}
        }
        // Adjust scroll
        if self.cursor_line < self.scroll {
            self.scroll = self.cursor_line;
        } else if self.cursor_line >= self.scroll + editor_height {
            self.scroll = self.cursor_line + 1 - editor_height;
        }
    }
}

pub struct DevApp {
    vfs: Arc<Vfs>,
    username: String,
    mode: Mode,
    buffers: Vec<Buffer>,
    active_buffer: usize,
    tree_entries: Vec<TreeEntry>,
    tree_selected: usize,
    tree_scroll: usize,
    current_dir: String,
    expanded_dirs: HashSet<String>,
    pub new_file_input: String,
    new_folder_input: String,
    save_as_input: String,
    pub command_input: String,
    status_msg: String,
    editor_height: usize,
    needs_refresh: bool,
}

impl DevApp {
    pub fn new(vfs: Arc<Vfs>, username: &str) -> Self {
        let home = format!("/home/{}", username);
        Self {
            vfs,
            username: username.to_string(),
            mode: Mode::FileTree,
            buffers: vec![Buffer::new_empty()],
            active_buffer: 0,
            tree_entries: Vec::new(),
            tree_selected: 0,
            tree_scroll: 0,
            current_dir: home,
            expanded_dirs: HashSet::new(),
            new_file_input: String::new(),
            new_folder_input: String::new(),
            save_as_input: String::new(),
            command_input: String::new(),
            status_msg: "Welcome to NeuraDev. [n] New file  [d] New folder  [↵] Open/Expand".to_string(),
            editor_height: 20,
            needs_refresh: true,
        }
    }

    pub fn needs_refresh(&self) -> bool { self.needs_refresh }

    /// Update the file tree root to the given VFS path (called when opened from the shell).
    pub fn set_cwd(&mut self, cwd: &str) {
        if self.current_dir != cwd {
            self.current_dir = cwd.to_string();
            self.tree_selected = 0;
            self.tree_scroll = 0;
            self.expanded_dirs.clear();
            self.needs_refresh = true;
        }
    }

    pub async fn async_refresh(&mut self) {
        self.needs_refresh = false;
        let mut entries = Vec::new();
        Self::build_tree_entries(&self.vfs, &self.current_dir, &self.expanded_dirs, 0, &mut entries).await;
        self.tree_entries = entries;
        // Clamp selection
        if !self.tree_entries.is_empty() && self.tree_selected >= self.tree_entries.len() {
            self.tree_selected = self.tree_entries.len() - 1;
        }
    }

    /// Recursively build the visible tree entries respecting expanded state.
    async fn build_tree_entries(
        vfs: &Arc<Vfs>,
        dir: &str,
        expanded_dirs: &HashSet<String>,
        depth: usize,
        entries: &mut Vec<TreeEntry>,
    ) {
        if let Ok(mut names) = vfs.list_dir(dir).await {
            names.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
            for name in names {
                let path = if dir == "/" {
                    format!("/{}", name)
                } else {
                    format!("{}/{}", dir, name)
                };
                let is_dir = matches!(vfs.stat(&path).await, Ok(ref info) if matches!(info.node_type, NodeType::Directory));
                let expanded = is_dir && expanded_dirs.contains(&path);
                entries.push(TreeEntry { name, path: path.clone(), depth, is_dir, expanded });
                if expanded {
                    Box::pin(Self::build_tree_entries(vfs, &path, expanded_dirs, depth + 1, entries)).await;
                }
            }
        }
    }

    /// Open a specific VFS file path directly (called from shell).
    pub async fn open_path(&mut self, path: &str) {
        match self.vfs.read_file(path).await {
            Ok(data) => {
                let content = String::from_utf8_lossy(&data).to_string();
                let buf = Buffer::from_content(path, &content);
                if let Some(i) = self.buffers.iter().position(|b| b.path.as_deref() == Some(path)) {
                    self.active_buffer = i;
                } else {
                    self.buffers.push(buf);
                    self.active_buffer = self.buffers.len() - 1;
                }
                // Navigate the file tree to the parent directory
                if let Some(parent) = path.rfind('/') {
                    self.current_dir = path[..parent].to_string();
                    self.expanded_dirs.clear();
                    self.needs_refresh = true;
                }
                self.mode = Mode::Editor;
                self.status_msg = format!("Opened: {}", path);
            }
            Err(e) => {
                self.status_msg = format!("Cannot open '{}': {}", path, e);
            }
        }
    }

    /// Open the currently selected file (called from main loop for __OPEN__ signal).
    pub async fn open_selected(&mut self) {
        let entry = match self.tree_entries.get(self.tree_selected) {
            Some(e) => e.clone(),
            None => return,
        };
        if entry.is_dir { return; } // dirs handled via expand/collapse in handle_key
        match self.vfs.read_file(&entry.path).await {
            Ok(data) => {
                let content = String::from_utf8_lossy(&data).to_string();
                let buf = Buffer::from_content(&entry.path, &content);
                if let Some(i) = self.buffers.iter().position(|b| b.path.as_deref() == Some(&entry.path)) {
                    self.active_buffer = i;
                } else {
                    self.buffers.push(buf);
                    self.active_buffer = self.buffers.len() - 1;
                }
                self.mode = Mode::Editor;
                self.status_msg = format!("Opened: {}", entry.path);
            }
            Err(e) => {
                self.status_msg = format!("Cannot open: {}", e);
            }
        }
    }

    pub async fn save_current(&mut self) {
        if self.active_buffer >= self.buffers.len() { return; }
        let buf = &self.buffers[self.active_buffer];
        let path = match &buf.path {
            Some(p) => p.clone(),
            None => {
                self.mode = Mode::SaveAs;
                self.save_as_input = format!("{}/", self.current_dir);
                return;
            }
        };
        let content = buf.content();
        match self.vfs.write_file(&path, content.into_bytes(), &self.username).await {
            Ok(()) => {
                self.buffers[self.active_buffer].modified = false;
                self.status_msg = format!("Saved: {} at {}", path, Utc::now().format("%H:%M:%S"));
            }
            Err(e) => {
                self.status_msg = format!("Save failed: {}", e);
            }
        }
    }

    pub async fn save_as_confirm(&mut self) {
        let path = self.save_as_input.trim().to_string();
        if path.is_empty() {
            self.mode = Mode::Editor;
            return;
        }
        let content = self.buffers[self.active_buffer].content();
        if let Some(last) = path.rfind('/') {
            let dir = &path[..last];
            let _ = self.vfs.mkdir(dir, &self.username).await;
        }
        match self.vfs.write_file(&path, content.into_bytes(), &self.username).await {
            Ok(()) => {
                let name = path.rsplit('/').next().unwrap_or(&path).to_string();
                self.buffers[self.active_buffer].path = Some(path.clone());
                self.buffers[self.active_buffer].name = name;
                self.buffers[self.active_buffer].modified = false;
                self.status_msg = format!("Saved as: {}", path);
                self.needs_refresh = true;
            }
            Err(e) => {
                self.status_msg = format!("Save failed: {}", e);
            }
        }
        self.save_as_input.clear();
        self.mode = Mode::Editor;
    }

    /// Create a new folder at the given full path (called from main loop for __MKDIR__ signal).
    pub async fn new_folder_confirm(&mut self, path: &str) {
        match self.vfs.mkdir(path, &self.username).await {
            Ok(()) => {
                let name = path.rsplit('/').next().unwrap_or(path);
                self.status_msg = format!("Created folder: {}/", name);
                self.needs_refresh = true;
            }
            Err(e) => {
                self.status_msg = format!("mkdir failed: {}", e);
            }
        }
    }

    fn new_file_confirm(&mut self) {
        let name = self.new_file_input.trim().to_string();
        if !name.is_empty() {
            // Determine target directory from the current selection:
            // - If a directory is selected → create inside it
            // - If a file is selected → create in its parent directory
            // - Fallback → use the root of the file tree
            let target_dir = if let Some(entry) = self.tree_entries.get(self.tree_selected) {
                if entry.is_dir {
                    entry.path.clone()
                } else {
                    entry.path.rsplit_once('/').map(|(p, _)| p.to_string())
                        .unwrap_or_else(|| self.current_dir.clone())
                }
            } else {
                self.current_dir.clone()
            };

            let path = format!("{}/{}", target_dir, name);
            let buf = Buffer::from_content(&path, "");
            self.buffers.push(buf);
            self.active_buffer = self.buffers.len() - 1;
            self.mode = Mode::Editor;
            self.status_msg = format!("New file: {} (not saved)", path);
        } else {
            self.mode = Mode::FileTree;
        }
        self.new_file_input.clear();
    }

    fn detect_syntax_color(line: &str, _ext: &str) -> Color {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with("--") {
            DIM
        } else if trimmed.starts_with("fn ") || trimmed.starts_with("pub ") || trimmed.starts_with("use ")
            || trimmed.starts_with("impl ") || trimmed.starts_with("struct ") || trimmed.starts_with("enum ")
            || trimmed.starts_with("let ") || trimmed.starts_with("const ") || trimmed.starts_with("mod ") {
            PRIMARY
        } else if trimmed.starts_with("def ") || trimmed.starts_with("class ") || trimmed.starts_with("import ") {
            PRIMARY
        } else if trimmed.starts_with('"') || trimmed.starts_with('\'') {
            GREEN
        } else {
            TEXT
        }
    }
}

impl App for DevApp {
    fn id(&self) -> &str { "dev" }
    fn name(&self) -> &str { "NeuraDev" }

    fn init(&mut self) -> anyhow::Result<()> { Ok(()) }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match &self.mode {
            Mode::FileTree => {
                match key.code {
                    KeyCode::Esc => return false,
                    KeyCode::Up | KeyCode::Char('k') => {
                        if self.tree_selected > 0 {
                            self.tree_selected -= 1;
                            // Adjust scroll
                            if self.tree_selected < self.tree_scroll {
                                self.tree_scroll = self.tree_selected;
                            }
                        }
                        true
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if self.tree_selected + 1 < self.tree_entries.len() {
                            self.tree_selected += 1;
                        }
                        true
                    }
                    KeyCode::Enter => {
                        if let Some(entry) = self.tree_entries.get(self.tree_selected) {
                            let path = entry.path.clone();
                            let is_dir = entry.is_dir;
                            if is_dir {
                                // Toggle expand/collapse
                                if self.expanded_dirs.contains(&path) {
                                    self.expanded_dirs.remove(&path);
                                } else {
                                    self.expanded_dirs.insert(path);
                                }
                                self.needs_refresh = true;
                            } else {
                                // Open file via async signal
                                self.status_msg = format!("Opening {}...", entry.name);
                                self.new_file_input = format!("__OPEN__:{}", path);
                            }
                        }
                        true
                    }
                    KeyCode::Backspace => {
                        // Go up one directory
                        if let Some(parent) = self.current_dir.rfind('/') {
                            if parent > 0 {
                                self.current_dir = self.current_dir[..parent].to_string();
                            }
                        }
                        self.tree_selected = 0;
                        self.tree_scroll = 0;
                        self.expanded_dirs.clear();
                        self.needs_refresh = true;
                        true
                    }
                    KeyCode::Char('n') | KeyCode::F(1) => {
                        self.mode = Mode::NewFile;
                        self.new_file_input.clear();
                        true
                    }
                    KeyCode::Char('d') | KeyCode::F(3) => {
                        self.mode = Mode::NewFolder;
                        self.new_folder_input.clear();
                        true
                    }
                    KeyCode::Tab => {
                        self.mode = Mode::Editor;
                        true
                    }
                    _ => true,
                }
            }
            Mode::Editor => {
                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                match key.code {
                    KeyCode::Esc => {
                        self.mode = Mode::FileTree;
                        true
                    }
                    KeyCode::F(2) | KeyCode::Char('s') if ctrl => {
                        self.status_msg = "Saving...".to_string();
                        self.command_input = "__SAVE__".to_string();
                        true
                    }
                    KeyCode::Char('w') if ctrl => {
                        if self.buffers.len() > 1 {
                            self.buffers.remove(self.active_buffer);
                            if self.active_buffer >= self.buffers.len() {
                                self.active_buffer = self.buffers.len() - 1;
                            }
                        }
                        true
                    }
                    KeyCode::Tab if ctrl => {
                        if self.buffers.len() > 1 {
                            self.active_buffer = (self.active_buffer + 1) % self.buffers.len();
                        }
                        true
                    }
                    KeyCode::Tab => {
                        if self.active_buffer < self.buffers.len() {
                            for _ in 0..4 {
                                self.buffers[self.active_buffer].insert_char(' ');
                            }
                        }
                        true
                    }
                    KeyCode::Char(c) if !ctrl => {
                        if self.active_buffer < self.buffers.len() {
                            self.buffers[self.active_buffer].insert_char(c);
                        }
                        true
                    }
                    KeyCode::Enter => {
                        if self.active_buffer < self.buffers.len() {
                            self.buffers[self.active_buffer].insert_newline();
                        }
                        true
                    }
                    KeyCode::Backspace => {
                        if self.active_buffer < self.buffers.len() {
                            self.buffers[self.active_buffer].delete_char();
                        }
                        true
                    }
                    KeyCode::Delete => {
                        if self.active_buffer < self.buffers.len() {
                            self.buffers[self.active_buffer].delete_forward();
                        }
                        true
                    }
                    KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down
                    | KeyCode::Home | KeyCode::End | KeyCode::PageUp | KeyCode::PageDown => {
                        if self.active_buffer < self.buffers.len() {
                            self.buffers[self.active_buffer].move_cursor(key.code, self.editor_height.saturating_sub(2));
                        }
                        true
                    }
                    _ => true,
                }
            }
            Mode::NewFile => {
                match key.code {
                    KeyCode::Esc => { self.mode = Mode::FileTree; self.new_file_input.clear(); true }
                    KeyCode::Enter => { self.new_file_confirm(); true }
                    KeyCode::Char(c) => { self.new_file_input.push(c); true }
                    KeyCode::Backspace => { self.new_file_input.pop(); true }
                    _ => true,
                }
            }
            Mode::NewFolder => {
                match key.code {
                    KeyCode::Esc => { self.mode = Mode::FileTree; self.new_folder_input.clear(); true }
                    KeyCode::Enter => {
                        let name = self.new_folder_input.trim().to_string();
                        if !name.is_empty() {
                            // Same logic as new_file_confirm: resolve target dir from selection
                            let target_dir = if let Some(entry) = self.tree_entries.get(self.tree_selected) {
                                if entry.is_dir {
                                    entry.path.clone()
                                } else {
                                    entry.path.rsplit_once('/').map(|(p, _)| p.to_string())
                                        .unwrap_or_else(|| self.current_dir.clone())
                                }
                            } else {
                                self.current_dir.clone()
                            };
                            let path = format!("{}/{}", target_dir, name);
                            self.command_input = format!("__MKDIR__:{}", path);
                        }
                        self.new_folder_input.clear();
                        self.mode = Mode::FileTree;
                        true
                    }
                    KeyCode::Char(c) => { self.new_folder_input.push(c); true }
                    KeyCode::Backspace => { self.new_folder_input.pop(); true }
                    _ => true,
                }
            }
            Mode::SaveAs => {
                match key.code {
                    KeyCode::Esc => { self.mode = Mode::Editor; self.save_as_input.clear(); true }
                    KeyCode::Enter => {
                        let input = self.save_as_input.clone();
                        self.command_input = format!("__SAVE_AS__:{}", input);
                        true
                    }
                    KeyCode::Char(c) => { self.save_as_input.push(c); true }
                    KeyCode::Backspace => { self.save_as_input.pop(); true }
                    _ => true,
                }
            }
            Mode::_Command => {
                match key.code {
                    KeyCode::Esc => { self.mode = Mode::Editor; true }
                    _ => true,
                }
            }
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(28), Constraint::Min(10)])
            .split(area);

        let left_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(5), Constraint::Length(1)])
            .split(main_chunks[0]);

        let right_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(5), Constraint::Length(1), Constraint::Length(1)])
            .split(main_chunks[1]);

        // ── File Tree ──
        let root_name = self.current_dir.rsplit('/').next().unwrap_or("/");
        let tree_title = format!(" \u{1f4c1} {} ", root_name);

        let visible_height = left_chunks[0].height.saturating_sub(2) as usize;
        let tree_scroll = if self.tree_selected >= visible_height {
            self.tree_selected.saturating_sub(visible_height - 1)
        } else {
            0
        };

        let tree_items: Vec<ListItem> = self.tree_entries.iter().enumerate()
            .skip(tree_scroll)
            .take(visible_height)
            .map(|(i, entry)| {
                let is_selected = i == self.tree_selected && self.mode == Mode::FileTree;
                let indent = "  ".repeat(entry.depth);
                let (icon, fg) = if entry.is_dir {
                    let exp_icon = if entry.expanded { "\u{25be} " } else { "\u{25b8} " };
                    (exp_icon, if is_selected { GREEN } else { PRIMARY })
                } else {
                    ("  ", if is_selected { GREEN } else { TEXT })
                };
                let suffix = if entry.is_dir { "/" } else { "" };
                let label = format!("{}{}{}{}", indent, icon, entry.name, suffix);
                let style = if is_selected {
                    Style::default().fg(fg).bg(SEL_BG).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(fg)
                };
                ListItem::new(label).style(style)
            }).collect();

        let tree_border_color = if self.mode == Mode::FileTree { PRIMARY } else { BORDER };
        let tree_list = List::new(tree_items)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(tree_border_color))
                .title(Span::styled(tree_title, Style::default().fg(PRIMARY))));
        frame.render_widget(tree_list, left_chunks[0]);

        // Tree help line
        let tree_help = Paragraph::new("[n] file  [d] folder  [\u{23ce}] open/expand  [\u{232b}] up")
            .style(Style::default().fg(DIM));
        frame.render_widget(tree_help, left_chunks[1]);

        // ── Buffer Tabs ──
        let tab_spans: Vec<Span> = self.buffers.iter().enumerate().map(|(i, b)| {
            let modified = if b.modified { " *" } else { "" };
            let label = format!(" {}{} ", b.name, modified);
            if i == self.active_buffer {
                Span::styled(label, Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD).bg(SEL_BG))
            } else {
                Span::styled(label, Style::default().fg(DIM))
            }
        }).collect();
        let tabs_line = Line::from(tab_spans);
        let tabs_para = Paragraph::new(tabs_line)
            .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(BORDER)));
        frame.render_widget(tabs_para, right_chunks[0]);

        // ── Editor ──
        let is_editing = self.mode == Mode::Editor || self.mode == Mode::SaveAs;
        let editor_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if is_editing { PRIMARY } else { BORDER }))
            .title(" Editor ")
            .title_style(Style::default().fg(PRIMARY));

        if self.active_buffer < self.buffers.len() {
            let buf = &self.buffers[self.active_buffer];
            let inner = editor_block.inner(right_chunks[1]);
            frame.render_widget(editor_block, right_chunks[1]);

            let ext = buf.name.rsplit('.').next().unwrap_or("").to_string();
            let visible_height = inner.height as usize;
            let line_num_width = 5u16;
            let text_width = inner.width.saturating_sub(line_num_width) as usize;

            let lines_to_show = &buf.lines[buf.scroll..buf.lines.len().min(buf.scroll + visible_height)];

            for (i, line) in lines_to_show.iter().enumerate() {
                let abs_line = buf.scroll + i;
                let is_cursor_line = abs_line == buf.cursor_line;
                let y = inner.y + i as u16;

                // Line number
                let num_str = format!("{:>4} ", abs_line + 1);
                let num_span = Paragraph::new(num_str)
                    .style(Style::default().fg(if is_cursor_line { ORANGE } else { LINE_NUM }));
                frame.render_widget(num_span, Rect { x: inner.x, y, width: line_num_width, height: 1 });

                // Line content
                let display = if line.len() > text_width { &line[..text_width] } else { line.as_str() };
                let color = if is_cursor_line {
                    TEXT
                } else {
                    Self::detect_syntax_color(line, &ext)
                };
                let bg = if is_cursor_line { SEL_BG } else { Color::Reset };
                let line_para = Paragraph::new(display)
                    .style(Style::default().fg(color).bg(bg));
                frame.render_widget(line_para, Rect { x: inner.x + line_num_width, y, width: inner.width.saturating_sub(line_num_width), height: 1 });
            }

            // Cursor position
            if is_editing {
                let cursor_y = inner.y + (buf.cursor_line.saturating_sub(buf.scroll)) as u16;
                let cursor_x = inner.x + line_num_width + buf.cursor_col as u16;
                if cursor_y < inner.y + inner.height && cursor_x < inner.x + inner.width {
                    frame.set_cursor_position((cursor_x, cursor_y));
                }
            }
        } else {
            frame.render_widget(editor_block, right_chunks[1]);
        }

        // ── Status Bar ──
        let buf_info = if self.active_buffer < self.buffers.len() {
            let buf = &self.buffers[self.active_buffer];
            format!(" {}:{} | {} lines | {} ",
                buf.cursor_line + 1, buf.cursor_col + 1, buf.lines.len(),
                if buf.modified { "modified" } else { "saved" })
        } else {
            String::new()
        };
        let status_line = format!("{}  {}", self.status_msg, buf_info);
        let status = Paragraph::new(status_line)
            .style(Style::default().fg(TEXT).bg(SEL_BG));
        frame.render_widget(status, right_chunks[2]);

        // ── Help / Input Bar ──
        let bottom_content = match &self.mode {
            Mode::SaveAs    => format!(" Save as: {}", self.save_as_input),
            Mode::NewFile   => format!(" New file name: {}", self.new_file_input),
            Mode::NewFolder => format!(" New folder name: {}", self.new_folder_input),
            _ => " [Esc] file tree  [Ctrl+S] save  [Ctrl+W] close  [Ctrl+Tab] next buffer  [Tab] indent".to_string(),
        };
        let bottom_style = match &self.mode {
            Mode::SaveAs | Mode::NewFile | Mode::NewFolder => Style::default().fg(ORANGE),
            _ => Style::default().fg(MUTED),
        };
        frame.render_widget(Paragraph::new(bottom_content).style(bottom_style), right_chunks[3]);
    }

    fn on_pause(&mut self) {}
    fn on_resume(&mut self) {
        self.needs_refresh = true;
    }
    fn on_close(&mut self) {}

    fn save_state(&self) -> Option<Value> { None }
    fn load_state(&mut self, _state: Value) {}

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
