use std::any::Any;
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::prelude::*;
use ratatui::widgets::*;
use serde_json::Value;
use neura_app_framework::app_trait::App;
use neura_storage::vfs::{Vfs, NodeType};

// ── Tokyo Night palette (imported from neura_app_framework) ──────────────────
use neura_app_framework::palette::{self, *};
// files app uses a slightly darker MUTED variant
const MUTED: Color = palette::STATUSBAR_MUTED;
// files app uses a slightly different SEL_BG
const SEL_BG: Color = Color::Rgb(40, 46, 74);

// ── File entry ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct FileEntry {
    name:       String,
    is_dir:     bool,
    size:       u64,
    owner:      String,
    modified:   String,       // "YYYY-MM-DD" from RFC3339 prefix
    created:    String,
    type_label: &'static str, // e.g. "DIR", "TXT", "COD"
    perms:      u16,          // octal e.g. 0o644
}

impl FileEntry {
    fn type_label(name: &str, is_dir: bool) -> &'static str {
        if is_dir { return "DIR"; }
        match name.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
            "txt" | "md" | "log" | "rst" | "rtf"                         => "TXT",
            "rs"  | "py" | "js"  | "ts"  | "c" | "cpp" | "h" | "go"
            | "java" | "kt" | "swift" | "rb" | "php" | "lua" | "zig"    => "COD",
            "json" | "toml" | "yaml" | "yml" | "ini" | "cfg"
            | "conf" | "env" | "lock"                                     => "CFG",
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg"
            | "webp" | "ico" | "tiff" | "raw"                            => "IMG",
            "mp4" | "avi" | "mkv" | "mov" | "webm" | "flv" | "wmv"     => "VID",
            "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" | "opus"    => "AUD",
            "zip" | "tar" | "gz"  | "bz2" | "xz" | "7z" | "rar"        => "ZIP",
            "pdf"                                                         => "PDF",
            "html" | "htm" | "css" | "xml" | "xhtml"                    => "WEB",
            "exe" | "sh" | "bat" | "cmd" | "msi"                        => "EXE",
            "db" | "sql" | "sqlite" | "sqlite3"                          => "DB ",
            _                                                             => "   ",
        }
    }

    fn type_color(label: &str) -> Color {
        match label {
            "DIR" => DIR_C,
            "COD" => OK,
            "CFG" => WARN,
            "IMG" => CYAN,
            "VID" | "AUD" => PURPLE,
            "ZIP" => Color::Rgb(255, 158, 100),
            "PDF" => ERR,
            "WEB" => Color::Rgb(42, 195, 222),
            "EXE" => Color::Rgb(255, 100, 100),
            "DB " => Color::Rgb(224, 175, 104),
            _     => FG,
        }
    }

    fn format_size(size: u64, is_dir: bool) -> String {
        if is_dir { return "       -".to_string(); }
        if size < 1_024 { format!("{:>6} B", size) }
        else if size < 1_048_576 { format!("{:>5.1}KB", size as f64 / 1_024.0) }
        else if size < 1_073_741_824 { format!("{:>5.1}MB", size as f64 / 1_048_576.0) }
        else { format!("{:>5.1}GB", size as f64 / 1_073_741_824.0) }
    }
}

// ── Enums ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Mode {
    Browse,
    ViewFile,
    NewDir,
    NewFile,
    Rename,
    DeleteConfirm,
    Search,
    Properties,
    JumpTo,
}

#[derive(Debug, Clone, PartialEq)]
enum SortBy { Name, Size, Date, Type }

#[derive(Debug, Clone, PartialEq)]
enum ClipOp { Copy, Cut }

#[derive(Debug, Clone)]
struct Clipboard {
    op:      ClipOp,
    src_dir: String,
    names:   Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum ActivePanel { Sidebar, Content }

// ── Sidebar bookmarks ─────────────────────────────────────────────────────────

struct Bookmark { label: &'static str, rel_path: &'static str }

static USER_BOOKMARKS: &[Bookmark] = &[
    Bookmark { label: "Home",      rel_path: "" },
    Bookmark { label: "Documents", rel_path: "documents" },
    Bookmark { label: "Notes",     rel_path: "notes" },
    Bookmark { label: "Downloads", rel_path: "downloads" },
    Bookmark { label: "Dev",       rel_path: "dev" },
];

static ADMIN_BOOKMARKS: &[Bookmark] = &[
    Bookmark { label: "/home",   rel_path: "/home" },
    Bookmark { label: "/system", rel_path: "/system" },
    Bookmark { label: "/apps",   rel_path: "/apps" },
    Bookmark { label: "/tmp",    rel_path: "/tmp" },
];

// ── FilesApp ──────────────────────────────────────────────────────────────────

pub struct FilesApp {
    vfs:          Arc<Vfs>,
    username:     String,
    home_dir:     String,
    is_admin:     bool,

    // Navigation
    cwd:          String,
    nav_back:     VecDeque<String>,
    nav_fwd:      VecDeque<String>,

    // File list
    entries:      Vec<FileEntry>,
    selected:     usize,
    selected_set: HashSet<usize>,
    scroll_offset: usize,

    // UI state
    mode:         Mode,
    active_panel: ActivePanel,
    sidebar_sel:  usize,

    // Input / messages
    input_buffer: String,
    status_msg:   String,
    status_ok:    bool,

    // File viewer
    file_content: String,
    file_scroll:  usize,
    view_path:    String,

    // Clipboard
    clipboard:    Option<Clipboard>,

    // Sort / filter
    sort_by:      SortBy,
    sort_rev:     bool,
    show_hidden:  bool,

    // Search (sync on current dir entries)
    search_results: Vec<String>,
    search_sel:     usize,

    // Properties
    prop_entry:   Option<FileEntry>,
    prop_path:    String,

    // Async signals (read by main.rs)
    needs_refresh_flag:   bool,
    needs_file_load_path: Option<String>,
    pub needs_paste:      bool,
    pub pending_rename:   Option<(String, String)>, // (old_full_path, new_name)
    pub pending_mkdir:    Option<String>,            // full path to create
    pub pending_new_file: Option<String>,            // full path to create
    pub pending_delete:   Vec<String>,               // full paths to delete
}

impl FilesApp {
    pub fn new(vfs: Arc<Vfs>, username: &str, is_admin: bool) -> Self {
        let home = format!("/home/{}", username);
        Self {
            vfs,
            username:     username.to_string(),
            home_dir:     home.clone(),
            is_admin,
            cwd:          home,
            nav_back:     VecDeque::new(),
            nav_fwd:      VecDeque::new(),
            entries:      Vec::new(),
            selected:     0,
            selected_set: HashSet::new(),
            scroll_offset: 0,
            mode:         Mode::Browse,
            active_panel: ActivePanel::Content,
            sidebar_sel:  0,
            input_buffer: String::new(),
            status_msg:   String::new(),
            status_ok:    true,
            file_content: String::new(),
            file_scroll:  0,
            view_path:    String::new(),
            clipboard:    None,
            sort_by:      SortBy::Name,
            sort_rev:     false,
            show_hidden:  false,
            search_results: Vec::new(),
            search_sel:   0,
            prop_entry:   None,
            prop_path:    String::new(),
            needs_refresh_flag:   true,
            needs_file_load_path: None,
            needs_paste:  false,
            pending_rename: None,
            pending_mkdir:    None,
            pending_new_file: None,
            pending_delete:   Vec::new(),
        }
    }

    // ── Security ──────────────────────────────────────────────────────────────

    /// Returns true if the current user may access the given VFS path.
    fn can_access(&self, path: &str) -> bool {
        if self.is_admin { return true; }
        // Non-admin: only their own home subtree
        path == self.home_dir
            || path.starts_with(&format!("{}/", self.home_dir))
    }

    // ── Navigation ────────────────────────────────────────────────────────────

    /// Open this app at a specific directory path (called from shell).
    pub fn open_at(&mut self, path: &str) {
        self.navigate_to(path.to_string());
    }

    fn navigate_to(&mut self, new_cwd: String) {
        if !self.can_access(&new_cwd) {
            self.set_err("Permission denied: restricted to your home directory.");
            return;
        }
        if new_cwd == self.cwd { return; }
        let old = std::mem::replace(&mut self.cwd, new_cwd);
        self.nav_back.push_back(old);
        if self.nav_back.len() > 50 { self.nav_back.pop_front(); }
        self.nav_fwd.clear();
        self.selected = 0;
        self.scroll_offset = 0;
        self.selected_set.clear();
        self.needs_refresh_flag = true;
    }

    fn go_back(&mut self) {
        if let Some(prev) = self.nav_back.pop_back() {
            self.nav_fwd.push_front(self.cwd.clone());
            self.cwd = prev;
            self.selected = 0;
            self.scroll_offset = 0;
            self.selected_set.clear();
            self.needs_refresh_flag = true;
        }
    }

    fn go_fwd(&mut self) {
        if let Some(next) = self.nav_fwd.pop_front() {
            self.nav_back.push_back(self.cwd.clone());
            self.cwd = next;
            self.selected = 0;
            self.scroll_offset = 0;
            self.selected_set.clear();
            self.needs_refresh_flag = true;
        }
    }

    fn go_up(&mut self) {
        if self.cwd == "/" { return; }
        let parts: Vec<&str> = self.cwd.split('/').filter(|s| !s.is_empty()).collect();
        let parent = if parts.len() <= 1 {
            "/".to_string()
        } else {
            format!("/{}", parts[..parts.len() - 1].join("/"))
        };
        if !self.can_access(&parent) {
            self.set_err("Permission denied: cannot navigate above your home directory.");
            return;
        }
        self.navigate_to(parent);
    }

    // ── Status ────────────────────────────────────────────────────────────────

    fn set_ok(&mut self, msg: &str) { self.status_msg = msg.to_string(); self.status_ok = true; }
    fn set_err(&mut self, msg: &str) { self.status_msg = msg.to_string(); self.status_ok = false; }

    // ── Entry helpers ─────────────────────────────────────────────────────────

    fn entry_full_path(&self, e: &FileEntry) -> String {
        if self.cwd == "/" { format!("/{}", e.name) } else { format!("{}/{}", self.cwd, e.name) }
    }

    fn selected_entry_path(&self) -> Option<String> {
        self.entries.get(self.selected).map(|e| self.entry_full_path(e))
    }

    fn effective_names(&self) -> Vec<String> {
        if self.selected_set.is_empty() {
            self.entries.get(self.selected)
                .filter(|e| e.name != "..")
                .map(|e| e.name.clone())
                .into_iter().collect()
        } else {
            self.selected_set.iter()
                .filter_map(|&i| self.entries.get(i))
                .filter(|e| e.name != "..")
                .map(|e| e.name.clone())
                .collect()
        }
    }

    fn effective_paths(&self) -> Vec<String> {
        if self.selected_set.is_empty() {
            self.entries.get(self.selected)
                .filter(|e| e.name != "..")
                .map(|e| self.entry_full_path(e))
                .into_iter().collect()
        } else {
            self.selected_set.iter()
                .filter_map(|&i| self.entries.get(i))
                .filter(|e| e.name != "..")
                .map(|e| self.entry_full_path(e))
                .collect()
        }
    }

    // ── Sort ──────────────────────────────────────────────────────────────────

    fn apply_sort(&mut self) {
        let rev = self.sort_rev;
        // Keep ".." always at the top
        let (dotdot, mut rest): (Vec<_>, Vec<_>) = self.entries.drain(..)
            .partition(|e| e.name == "..");
        match self.sort_by {
            SortBy::Name => rest.sort_by(|a, b| {
                let c = b.is_dir.cmp(&a.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                if rev { c.reverse() } else { c }
            }),
            SortBy::Size => rest.sort_by(|a, b| {
                let c = b.is_dir.cmp(&a.is_dir).then(a.size.cmp(&b.size));
                if rev { c.reverse() } else { c }
            }),
            SortBy::Date => rest.sort_by(|a, b| {
                let c = b.is_dir.cmp(&a.is_dir).then(a.modified.cmp(&b.modified));
                if rev { c.reverse() } else { c }
            }),
            SortBy::Type => rest.sort_by(|a, b| {
                let c = b.is_dir.cmp(&a.is_dir)
                    .then(a.type_label.cmp(b.type_label))
                    .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                if rev { c.reverse() } else { c }
            }),
        }
        self.entries.extend(dotdot);
        self.entries.extend(rest);
        // Move ".." back to front (it was pushed, but we need it first)
        if let Some(pos) = self.entries.iter().position(|e| e.name == "..") {
            let e = self.entries.remove(pos);
            self.entries.insert(0, e);
        }
    }

    // ── Public async interface (called by main.rs) ─────────────────────────────

    pub fn needs_refresh(&self) -> bool { self.needs_refresh_flag }
    pub fn needs_file_load(&self) -> Option<&str> { self.needs_file_load_path.as_deref() }

    pub async fn async_refresh(&mut self) {
        self.entries.clear();

        // ".." entry — only if we can navigate up
        let show_parent = if self.is_admin {
            self.cwd != "/"
        } else {
            self.cwd != self.home_dir
        };
        if show_parent {
            self.entries.push(FileEntry {
                name: "..".to_string(), is_dir: true,
                size: 0, owner: String::new(), modified: String::new(),
                created: String::new(), type_label: "DIR",
                perms: 0o755,
            });
        }

        match self.vfs.list_dir(&self.cwd).await {
            Ok(names) => {
                for name in names {
                    if !self.show_hidden && name.starts_with('.') { continue; }
                    let full = if self.cwd == "/" {
                        format!("/{}", name)
                    } else {
                        format!("{}/{}", self.cwd, name)
                    };
                    let (is_dir, size, owner, modified, created, perms) =
                        if let Ok(info) = self.vfs.stat(&full).await {
                            let mod_d = info.modified_at.get(..10).unwrap_or("").to_string();
                            let cre_d = info.created_at.get(..10).unwrap_or("").to_string();
                            (
                                info.node_type == NodeType::Directory,
                                info.size, info.owner, mod_d, cre_d,
                                info.permissions.to_octal(),
                            )
                        } else {
                            (false, 0, String::new(), String::new(), String::new(), 0o644)
                        };
                    self.entries.push(FileEntry {
                        name: name.clone(),
                        is_dir,
                        size,
                        owner,
                        modified,
                        created,
                        type_label: FileEntry::type_label(&name, is_dir),
                        perms,
                    });
                }
            }
            Err(e) => self.set_err(&format!("Cannot read directory: {}", e)),
        }

        self.apply_sort();
        if self.selected >= self.entries.len() {
            self.selected = self.entries.len().saturating_sub(1);
        }
        self.needs_refresh_flag = false;
    }

    pub async fn async_load_file(&mut self) {
        if let Some(path) = self.needs_file_load_path.take() {
            match self.vfs.read_file(&path).await {
                Ok(data) => {
                    self.file_content = String::from_utf8_lossy(&data).to_string();
                    if self.file_content.is_empty() {
                        self.file_content = "(empty file)".to_string();
                    }
                }
                Err(e) => self.file_content = format!("Error reading file: {}", e),
            }
        }
    }

    /// Copy/move files from clipboard into current directory.
    pub async fn async_paste(&mut self) {
        self.needs_paste = false;
        let clip = match self.clipboard.take() {
            Some(c) => c,
            None => return,
        };
        let mut ok = 0usize;
        let mut fail = 0usize;

        for name in &clip.names {
            let src = if clip.src_dir == "/" {
                format!("/{}", name)
            } else {
                format!("{}/{}", clip.src_dir, name)
            };
            let mut dst_name = name.clone();
            let mut dst = if self.cwd == "/" {
                format!("/{}", dst_name)
            } else {
                format!("{}/{}", self.cwd, dst_name)
            };
            // Avoid overwriting: append _copy suffix if same path
            if src == dst {
                dst_name = format!("{}_copy", name);
                dst = if self.cwd == "/" {
                    format!("/{}", dst_name)
                } else {
                    format!("{}/{}", self.cwd, dst_name)
                };
            }
            match copy_vfs_recursive(&self.vfs, &src, &dst, &self.username).await {
                Ok(()) => {
                    ok += 1;
                    if clip.op == ClipOp::Cut {
                        remove_vfs_recursive(&self.vfs, &src).await;
                    }
                }
                Err(e) => {
                    tracing::warn!("Paste '{}' → '{}' failed: {}", src, dst, e);
                    fail += 1;
                }
            }
        }
        // Restore clipboard for Copy (not Cut)
        if clip.op == ClipOp::Copy {
            self.clipboard = Some(clip);
        }
        if fail == 0 {
            self.set_ok(&format!("{} item(s) pasted.", ok));
        } else {
            self.set_err(&format!("{} pasted, {} failed.", ok, fail));
        }
        self.needs_refresh_flag = true;
    }

    pub async fn async_mkdir(&mut self) {
        if let Some(path) = self.pending_mkdir.take() {
            let name = path.rsplit('/').next().unwrap_or(&path).to_string();
            match self.vfs.mkdir(&path, &self.username).await {
                Ok(()) => self.set_ok(&format!("Directory '{}' created.", name)),
                Err(e) => self.set_err(&format!("mkdir failed: {}", e)),
            }
            self.needs_refresh_flag = true;
        }
    }

    pub async fn async_new_file(&mut self) {
        if let Some(path) = self.pending_new_file.take() {
            let name = path.rsplit('/').next().unwrap_or(&path).to_string();
            match self.vfs.write_file(&path, Vec::new(), &self.username).await {
                Ok(()) => self.set_ok(&format!("File '{}' created.", name)),
                Err(e) => self.set_err(&format!("touch failed: {}", e)),
            }
            self.needs_refresh_flag = true;
        }
    }

    pub async fn async_delete(&mut self) {
        if self.pending_delete.is_empty() { return; }
        let paths = std::mem::take(&mut self.pending_delete);
        let count = paths.len();
        let mut failed = 0usize;
        for path in &paths {
            let vfs = self.vfs.clone();
            remove_vfs_recursive(&*vfs, path).await;
            // Verify it's gone
            if self.vfs.exists(path).await {
                failed += 1;
            }
        }
        if failed == 0 {
            self.set_ok(&format!("{} item(s) deleted.", count));
        } else {
            self.set_err(&format!("{} deleted, {} failed.", count - failed, failed));
        }
        self.needs_refresh_flag = true;
    }

    /// Rename selected file to a new name in the same directory.
    pub async fn async_rename(&mut self) {
        if let Some((old_path, new_name)) = self.pending_rename.take() {
            if new_name.is_empty() { return; }
            let parent = parent_of(&old_path);
            let new_path = if parent == "/" {
                format!("/{}", new_name)
            } else {
                format!("{}/{}", parent, new_name)
            };
            match copy_vfs_recursive(&self.vfs, &old_path, &new_path, &self.username).await {
                Ok(()) => {
                    remove_vfs_recursive(&self.vfs, &old_path).await;
                    self.set_ok(&format!("Renamed to '{}'.", new_name));
                }
                Err(e) => self.set_err(&format!("Rename failed: {}", e)),
            }
            self.needs_refresh_flag = true;
        }
    }

    // ── Sidebar helpers ───────────────────────────────────────────────────────

    fn bookmarks(&self) -> Vec<(String, String)> {
        let mut bms: Vec<(String, String)> = USER_BOOKMARKS.iter().map(|b| {
            let path = if b.rel_path.is_empty() {
                self.home_dir.clone()
            } else {
                format!("{}/{}", self.home_dir, b.rel_path)
            };
            (b.label.to_string(), path)
        }).collect();
        if self.is_admin {
            for b in ADMIN_BOOKMARKS {
                bms.push((b.label.to_string(), b.rel_path.to_string()));
            }
        }
        bms
    }

    // ── Breadcrumb display ────────────────────────────────────────────────────

    fn breadcrumb(&self) -> String {
        if self.cwd == "/" { return "/".to_string(); }
        self.cwd.split('/').filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" › ")
    }

    // ── Search helper (sync, searches loaded entries) ─────────────────────────

    fn update_search(&mut self) {
        let q = self.input_buffer.to_lowercase();
        if q.is_empty() {
            self.search_results.clear();
            return;
        }
        self.search_results = self.entries.iter()
            .filter(|e| e.name.to_lowercase().contains(&q) && e.name != "..")
            .map(|e| e.name.clone())
            .collect();
        self.search_sel = 0;
    }

    // ── Key handlers ──────────────────────────────────────────────────────────

    fn handle_browse(&mut self, key: KeyEvent) -> bool {
        let ctrl  = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);

        match key.code {
            KeyCode::Esc => return false,

            // ── Panel switch ──────────────────────────────────────────────────
            KeyCode::Tab => {
                self.active_panel = if self.active_panel == ActivePanel::Sidebar {
                    ActivePanel::Content
                } else {
                    ActivePanel::Sidebar
                };
            }

            // ── Navigation ────────────────────────────────────────────────────
            KeyCode::Up | KeyCode::Char('k') => {
                if self.active_panel == ActivePanel::Sidebar {
                    if self.sidebar_sel > 0 { self.sidebar_sel -= 1; }
                } else {
                    if self.selected > 0 {
                        if shift { self.selected_set.insert(self.selected); }
                        self.selected -= 1;
                        if shift { self.selected_set.insert(self.selected); }
                    }
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.active_panel == ActivePanel::Sidebar {
                    let bm_len = self.bookmarks().len();
                    if self.sidebar_sel + 1 < bm_len {
                        self.sidebar_sel += 1;
                        // scroll will be adjusted in render
                    }
                } else {
                    if self.selected + 1 < self.entries.len() {
                        if shift { self.selected_set.insert(self.selected); }
                        self.selected += 1;
                        if shift { self.selected_set.insert(self.selected); }
                    }
                }
            }
            KeyCode::PageUp => {
                self.selected = self.selected.saturating_sub(10);
                self.scroll_offset = self.scroll_offset.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.selected = (self.selected + 10).min(self.entries.len().saturating_sub(1));
            }
            KeyCode::Home => { self.selected = 0; self.scroll_offset = 0; }
            KeyCode::End  => { self.selected = self.entries.len().saturating_sub(1); }

            // ── Open / Forward ────────────────────────────────────────────────
            KeyCode::Enter | KeyCode::Char('l') => {
                // Ctrl+L: jump to path (same as 'g')
                if ctrl && key.code == KeyCode::Char('l') {
                    self.input_buffer = self.cwd.clone();
                    self.mode = Mode::JumpTo;
                } else if self.active_panel == ActivePanel::Sidebar {
                    if let Some((_, path)) = self.bookmarks().get(self.sidebar_sel).cloned() {
                        self.navigate_to(path);
                        self.active_panel = ActivePanel::Content;
                    }
                } else if let Some(entry) = self.entries.get(self.selected).cloned() {
                    if entry.is_dir {
                        if entry.name == ".." { self.go_up(); }
                        else { let path = self.entry_full_path(&entry); self.navigate_to(path); }
                    } else {
                        let path = self.entry_full_path(&entry);
                        self.view_path = path.clone();
                        self.file_content = "(loading...)".to_string();
                        self.file_scroll  = 0;
                        self.needs_file_load_path = Some(path);
                        self.mode = Mode::ViewFile;
                    }
                }
            }
            KeyCode::Right => {
                if ctrl {
                    self.go_fwd();
                } else if self.active_panel == ActivePanel::Sidebar {
                    if let Some((_, path)) = self.bookmarks().get(self.sidebar_sel).cloned() {
                        self.navigate_to(path);
                        self.active_panel = ActivePanel::Content;
                    }
                } else if let Some(entry) = self.entries.get(self.selected).cloned() {
                    if entry.is_dir {
                        if entry.name == ".." { self.go_up(); }
                        else { let path = self.entry_full_path(&entry); self.navigate_to(path); }
                    } else {
                        let path = self.entry_full_path(&entry);
                        self.view_path = path.clone();
                        self.file_content = "(loading...)".to_string();
                        self.file_scroll  = 0;
                        self.needs_file_load_path = Some(path);
                        self.mode = Mode::ViewFile;
                    }
                }
            }

            // ── Go up / Back ──────────────────────────────────────────────────
            KeyCode::Left => {
                if ctrl {
                    self.go_back();
                } else if self.active_panel == ActivePanel::Content {
                    self.go_up();
                }
            }
            KeyCode::Backspace if self.active_panel == ActivePanel::Content && !ctrl => {
                self.go_up();
            }

            // ── History shortcuts ─────────────────────────────────────────────
            KeyCode::Char('[') => { self.go_back(); }
            KeyCode::Char(']') => { self.go_fwd(); }

            // ── Go home ───────────────────────────────────────────────────────
            KeyCode::Char('~') | KeyCode::Char('H') if shift => {
                self.navigate_to(self.home_dir.clone());
            }

            // ── Multi-select ──────────────────────────────────────────────────
            KeyCode::Char(' ') if self.active_panel == ActivePanel::Content => {
                let i = self.selected;
                if let Some(e) = self.entries.get(i) {
                    if e.name != ".." {
                        if self.selected_set.contains(&i) {
                            self.selected_set.remove(&i);
                        } else {
                            self.selected_set.insert(i);
                        }
                    }
                }
                if self.selected + 1 < self.entries.len() { self.selected += 1; }
            }
            KeyCode::Char('a') if ctrl => {
                if self.selected_set.len() == self.entries.len() {
                    self.selected_set.clear();
                } else {
                    self.selected_set = (0..self.entries.len()).collect();
                    self.selected_set.remove(&usize::MAX); // remove ".." if somehow present
                    if let Some(pos) = self.entries.iter().position(|e| e.name == "..") {
                        self.selected_set.remove(&pos);
                    }
                }
            }

            // ── Clipboard ─────────────────────────────────────────────────────
            KeyCode::Char('c') if ctrl => {
                let names = self.effective_names();
                if !names.is_empty() {
                    let n = names.len();
                    self.clipboard = Some(Clipboard { op: ClipOp::Copy, src_dir: self.cwd.clone(), names });
                    self.set_ok(&format!("{} item(s) copied to clipboard.", n));
                }
            }
            KeyCode::Char('x') if ctrl => {
                let names = self.effective_names();
                if !names.is_empty() {
                    let n = names.len();
                    self.clipboard = Some(Clipboard { op: ClipOp::Cut, src_dir: self.cwd.clone(), names });
                    self.set_ok(&format!("{} item(s) cut to clipboard.", n));
                }
            }
            KeyCode::Char('v') if ctrl => {
                if self.clipboard.is_some() {
                    if !self.can_access(&self.cwd) {
                        self.set_err("Permission denied: cannot paste here.");
                    } else {
                        self.needs_paste = true;
                    }
                } else {
                    self.set_err("Clipboard is empty.");
                }
            }

            // ── New directory ─────────────────────────────────────────────────
            KeyCode::Char('n') if !ctrl => {
                if !self.can_access(&self.cwd) {
                    self.set_err("Permission denied.");
                } else {
                    self.input_buffer.clear();
                    self.mode = Mode::NewDir;
                }
            }

            // ── New file ──────────────────────────────────────────────────────
            KeyCode::Char('t') if !ctrl => {
                if !self.can_access(&self.cwd) {
                    self.set_err("Permission denied.");
                } else {
                    self.input_buffer.clear();
                    self.mode = Mode::NewFile;
                }
            }

            // ── Rename (F2) ───────────────────────────────────────────────────
            KeyCode::F(2) => {
                if let Some(e) = self.entries.get(self.selected) {
                    if e.name != ".." {
                        self.input_buffer = e.name.clone();
                        self.mode = Mode::Rename;
                    }
                }
            }

            // ── Delete ────────────────────────────────────────────────────────
            KeyCode::Delete | KeyCode::Char('d') if !ctrl => {
                if self.active_panel == ActivePanel::Content {
                    let names = self.effective_names();
                    if !names.is_empty() {
                        self.mode = Mode::DeleteConfirm;
                    }
                }
            }

            // ── Properties ────────────────────────────────────────────────────
            KeyCode::Char('i') | KeyCode::F(4) => {
                if let Some(e) = self.entries.get(self.selected).cloned() {
                    if e.name != ".." {
                        self.prop_path  = self.entry_full_path(&e);
                        self.prop_entry = Some(e);
                        self.mode = Mode::Properties;
                    }
                }
            }

            // ── Search ────────────────────────────────────────────────────────
            KeyCode::Char('/') | KeyCode::Char('f') if ctrl => {
                self.input_buffer.clear();
                self.search_results.clear();
                self.search_sel = 0;
                self.mode = Mode::Search;
            }
            KeyCode::Char('f') if !ctrl => {
                self.input_buffer.clear();
                self.search_results.clear();
                self.search_sel = 0;
                self.mode = Mode::Search;
            }

            // ── Jump to path ──────────────────────────────────────────────────
            KeyCode::Char('g') if !ctrl => {
                self.input_buffer = self.cwd.clone();
                self.mode = Mode::JumpTo;
            }

            // ── Toggle hidden files ───────────────────────────────────────────
            KeyCode::Char('.') => {
                self.show_hidden = !self.show_hidden;
                self.needs_refresh_flag = true;
                self.set_ok(if self.show_hidden { "Showing hidden files." } else { "Hiding hidden files." });
            }

            // ── Sort ──────────────────────────────────────────────────────────
            KeyCode::F(3) => {
                self.sort_by = match self.sort_by {
                    SortBy::Name => SortBy::Size,
                    SortBy::Size => SortBy::Date,
                    SortBy::Date => SortBy::Type,
                    SortBy::Type => SortBy::Name,
                };
                self.apply_sort();
                self.set_ok(&format!("Sorted by {:?}.", self.sort_by));
            }
            KeyCode::Char('r') if !ctrl => {
                self.sort_rev = !self.sort_rev;
                self.apply_sort();
                self.set_ok(if self.sort_rev { "Reversed sort." } else { "Normal sort." });
            }

            // ── Refresh ───────────────────────────────────────────────────────
            KeyCode::F(5) | KeyCode::Char('r') if ctrl => {
                self.selected_set.clear();
                self.needs_refresh_flag = true;
                self.set_ok("Refreshed.");
            }

            _ => {}
        }
        true
    }

    fn handle_view_file(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Left => {
                self.mode = Mode::Browse;
                self.file_scroll = 0;
            }
            KeyCode::Up   | KeyCode::Char('k') => { self.file_scroll = self.file_scroll.saturating_sub(1); }
            KeyCode::Down | KeyCode::Char('j') => { self.file_scroll = self.file_scroll.saturating_add(1); }
            KeyCode::PageUp   => { self.file_scroll = self.file_scroll.saturating_sub(20); }
            KeyCode::PageDown => { self.file_scroll = self.file_scroll.saturating_add(20); }
            KeyCode::Home => { self.file_scroll = 0; }
            KeyCode::End  => { self.file_scroll = usize::MAX; }
            _ => {}
        }
        true
    }

    fn handle_input_mode(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Browse;
                self.status_msg.clear();
            }
            KeyCode::Enter => {
                let input = self.input_buffer.trim().to_string();
                match self.mode.clone() {
                    Mode::NewDir => {
                        if !input.is_empty() {
                            let path = join_path(&self.cwd, &input);
                            self.set_ok(&format!("Creating '{}'...", input));
                            self.pending_mkdir = Some(path);
                        }
                        self.mode = Mode::Browse;
                    }
                    Mode::NewFile => {
                        if !input.is_empty() {
                            let path = join_path(&self.cwd, &input);
                            self.set_ok(&format!("Creating '{}'...", input));
                            self.pending_new_file = Some(path);
                        }
                        self.mode = Mode::Browse;
                    }
                    Mode::Rename => {
                        if !input.is_empty() {
                            if let Some(path) = self.selected_entry_path() {
                                self.pending_rename = Some((path, input));
                            }
                        }
                        self.mode = Mode::Browse;
                    }
                    Mode::JumpTo => {
                        if !input.is_empty() {
                            self.navigate_to(input);
                        }
                        self.mode = Mode::Browse;
                    }
                    Mode::Search => {
                        // Navigate to first match
                        if let Some(name) = self.search_results.get(self.search_sel).cloned() {
                            // Find the entry with this name and select it
                            if let Some(idx) = self.entries.iter().position(|e| e.name == name) {
                                self.selected = idx;
                                self.selected_set.clear();
                            }
                        }
                        self.mode = Mode::Browse;
                    }
                    _ => { self.mode = Mode::Browse; }
                }
            }
            KeyCode::Char(c) => {
                self.input_buffer.push(c);
                if self.mode == Mode::Search { self.update_search(); }
            }
            KeyCode::Backspace => {
                self.input_buffer.pop();
                if self.mode == Mode::Search { self.update_search(); }
            }
            KeyCode::Up => {
                if self.mode == Mode::Search && self.search_sel > 0 {
                    self.search_sel -= 1;
                }
            }
            KeyCode::Down => {
                if self.mode == Mode::Search && self.search_sel + 1 < self.search_results.len() {
                    self.search_sel += 1;
                }
            }
            _ => {}
        }
        true
    }

    fn handle_delete_confirm(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let paths = self.effective_paths();
                let count = paths.len();
                self.selected_set.clear();
                self.set_ok(&format!("Deleting {} item(s)...", count));
                self.pending_delete = paths;
                self.mode = Mode::Browse;
            }
            _ => {
                self.set_ok("Delete cancelled.");
                self.mode = Mode::Browse;
            }
        }
        true
    }

    // ── Render ────────────────────────────────────────────────────────────────

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let can_back = !self.nav_back.is_empty();
        let can_fwd  = !self.nav_fwd.is_empty();
        let back_sym = if can_back { "◂" } else { " " };
        let fwd_sym  = if can_fwd  { "▸" } else { " " };
        let sort_indicator = match self.sort_by {
            SortBy::Name => "⇅Name",
            SortBy::Size => "⇅Size",
            SortBy::Date => "⇅Date",
            SortBy::Type => "⇅Type",
        };
        let rev_indicator = if self.sort_rev { "↓" } else { "↑" };
        let hidden_ind = if self.show_hidden { " [H]" } else { "" };
        let title = format!(" NeuraFiles{}  {}{}", hidden_ind, sort_indicator, rev_indicator);

        let inner_w = area.width.saturating_sub(4) as usize;
        let crumb = self.breadcrumb();
        let nav_part = format!(" {} {} ", back_sym, fwd_sym);
        let available = inner_w.saturating_sub(nav_part.len() + 2);
        let crumb_display = if crumb.len() > available {
            format!("…{}", &crumb[crumb.len().saturating_sub(available)..])
        } else {
            crumb
        };

        let bar = Paragraph::new(format!("{}{}", nav_part, crumb_display))
            .style(Style::default().fg(WARN))
            .block(Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BORDER))
                .title(Span::styled(title, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)))
                .style(Style::default().bg(PANEL)));
        frame.render_widget(bar, area);
    }

    fn render_sidebar(&self, frame: &mut Frame, area: Rect) {
        let is_active = self.active_panel == ActivePanel::Sidebar;
        let border_col = if is_active { ACCENT } else { BORDER };
        let title_col = if is_active { ACCENT } else { MUTED };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_col))
            .title(Span::styled(" Places ", Style::default().fg(title_col)))
            .style(Style::default().bg(PANEL));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let bms = self.bookmarks();
        let max_w = inner.width as usize;

        // Compute scroll offset to keep sidebar_sel in view
        let visible_height = inner.height.saturating_sub(2) as usize; // subtract header + blank line
        let scroll = if visible_height > 0 && self.sidebar_sel >= visible_height {
            self.sidebar_sel + 1 - visible_height
        } else {
            0
        };

        // Section header (always shown, not scrolled)
        let header_style = Style::default().fg(MUTED).add_modifier(Modifier::BOLD);
        frame.render_widget(
            Paragraph::new(vec![
                Line::styled("  PLACES", header_style),
                Line::styled("", Style::default()),
            ]),
            Rect { x: inner.x, y: inner.y, width: inner.width, height: 2.min(inner.height) },
        );

        // Bookmarks (scrollable)
        let item_area_y = inner.y + 2;
        let item_area_h = inner.height.saturating_sub(2);
        if item_area_h == 0 { return; }

        let visible_bms: Vec<Line> = bms.iter().enumerate()
            .skip(scroll)
            .take(item_area_h as usize)
            .map(|(i, (label, path))| {
                // Show selection when active (bright highlight) OR always show a ghost cursor when inactive
                let is_sel = i == self.sidebar_sel;
                let is_cwd = *path == self.cwd;
                let prefix = if is_sel && is_active { "▶ " }
                    else if is_sel { "› " }   // ghost cursor when panel not focused
                    else if is_cwd { "• " }
                    else { "  " };
                let style = if is_sel && is_active {
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD).bg(SEL_BG)
                } else if is_sel {
                    // Dim selection indicator when sidebar not focused
                    Style::default().fg(MUTED)
                } else if is_cwd {
                    Style::default().fg(OK)
                } else {
                    Style::default().fg(FG)
                };
                let label_str = format!("{}{}", prefix, label);
                let display = if label_str.len() > max_w {
                    label_str[..max_w].to_string()
                } else {
                    label_str
                };
                Line::styled(display, style)
            }).collect();

        // Scroll indicator
        let scroll_hint = if scroll > 0 || scroll + (item_area_h as usize) < bms.len() {
            let pct = if bms.is_empty() { 0 } else {
                (self.sidebar_sel * 100) / bms.len()
            };
            format!("{:>3}%", pct)
        } else {
            String::new()
        };

        frame.render_widget(
            Paragraph::new(visible_bms),
            Rect { x: inner.x, y: item_area_y, width: inner.width.saturating_sub(if scroll_hint.is_empty() { 0 } else { 4 }), height: item_area_h },
        );

        // Draw scroll % indicator
        if !scroll_hint.is_empty() && item_area_h > 0 {
            frame.render_widget(
                Paragraph::new(scroll_hint).style(Style::default().fg(DIM)),
                Rect { x: inner.x + inner.width.saturating_sub(4), y: item_area_y, width: 4, height: 1 },
            );
        }

        // Clipboard indicator at the bottom
        if let Some(ref clip) = self.clipboard {
            let clip_y = inner.y + inner.height.saturating_sub(1);
            if clip_y >= item_area_y {
                let clip_label = match clip.op {
                    ClipOp::Copy => format!(" ✂ {} cp", clip.names.len()),
                    ClipOp::Cut  => format!(" ✂ {} cut", clip.names.len()),
                };
                let display = if clip_label.len() > max_w { clip_label[..max_w].to_string() } else { clip_label };
                frame.render_widget(
                    Paragraph::new(display).style(Style::default().fg(WARN)),
                    Rect { x: inner.x, y: clip_y, width: inner.width, height: 1 },
                );
            }
        }
    }

    fn render_content(&self, frame: &mut Frame, area: Rect) {
        let is_active = self.active_panel == ActivePanel::Content;
        let border_col = if is_active { ACCENT } else { BORDER };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_col))
            .style(Style::default().bg(PANEL));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height < 2 { return; }

        // Column widths — responsive
        let w = inner.width as usize;
        let size_w   = 8usize;
        let type_w   = 5usize;
        let date_w   = 10usize;
        let owner_w  = 10usize;
        let has_size = w >= 40;
        let has_type = w >= 55;
        let has_date = w >= 70;
        let has_owner= w >= 90;
        let fixed_w  = 2   // marker + prefix arrow
            + 5 + 1        // [TAG] + space
            + if has_size  { size_w + 1 } else { 0 }
            + if has_type  { type_w + 1 } else { 0 }
            + if has_date  { date_w + 1 } else { 0 }
            + if has_owner { owner_w + 1 } else { 0 };
        let name_w = w.saturating_sub(fixed_w);

        // Column headers (8-char prefix: 2 marker/prefix + 5 [TAG] + 1 space)
        let mut header = format!("        {:<name_w$}", "Name", name_w = name_w.min(w.saturating_sub(8)));
        if has_size  { header.push_str(&format!(" {:>size_w$}", "Size",  size_w = size_w));  }
        if has_type  { header.push_str(&format!(" {:<type_w$}", "Type",  type_w = type_w));  }
        if has_date  { header.push_str(&format!(" {:<date_w$}", "Modified", date_w = date_w)); }
        if has_owner { header.push_str(&format!(" {:<owner_w$}", "Owner", owner_w = owner_w)); }
        frame.render_widget(
            Paragraph::new(header).style(Style::default().fg(MUTED).add_modifier(Modifier::BOLD)),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );

        // Divider
        let sep = "─".repeat(inner.width as usize);
        frame.render_widget(
            Paragraph::new(sep).style(Style::default().fg(BORDER)),
            Rect::new(inner.x, inner.y + 1, inner.width, 1),
        );

        // File list
        let list_area = Rect::new(inner.x, inner.y + 2, inner.width, inner.height.saturating_sub(2));
        let visible = list_area.height as usize;
        let mut scroll = self.scroll_offset;
        if self.selected < scroll { scroll = self.selected; }
        else if self.selected >= scroll + visible { scroll = self.selected.saturating_sub(visible.saturating_sub(1)); }

        for (row, (i, entry)) in self.entries.iter().enumerate().skip(scroll).take(visible).enumerate() {
            let y = list_area.y + row as u16;
            let is_selected   = i == self.selected && is_active;
            let is_in_set     = self.selected_set.contains(&i);
            let is_marked     = is_selected || is_in_set;

            let type_col = FileEntry::type_color(entry.type_label);
            let name_col = if entry.name == ".." {
                MUTED
            } else if entry.is_dir {
                DIR_C
            } else {
                FG
            };

            let sel_marker = if is_in_set { "●" } else { " " };
            let prefix = if is_selected { "▶" } else { " " };
            let type_tag = if entry.name == ".." {
                "   ".to_string()
            } else {
                format!("[{}]", entry.type_label)
            };

            let raw_name = if entry.name == ".." {
                "..  (parent directory)".to_string()
            } else {
                format!("{}{}", entry.name, if entry.is_dir { "/" } else { "" })
            };
            let display_name = if raw_name.len() > name_w {
                format!("{}…", &raw_name[..name_w.saturating_sub(1)])
            } else {
                format!("{:<width$}", raw_name, width = name_w)
            };

            // Build the line spans
            let bg = if is_marked { SEL_BG } else { PANEL };
            let row_rect = Rect::new(list_area.x, y, list_area.width, 1);
            // Background fill
            frame.render_widget(
                Paragraph::new(" ".repeat(list_area.width as usize))
                    .style(Style::default().bg(bg)),
                row_rect,
            );

            let mut x = list_area.x;
            // marker
            let m_style = if is_in_set {
                Style::default().fg(WARN).bg(bg)
            } else {
                Style::default().fg(MUTED).bg(bg)
            };
            frame.render_widget(Paragraph::new(sel_marker).style(m_style), Rect::new(x, y, 1, 1));
            x += 1;
            // prefix arrow
            let p_style = if is_selected {
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD).bg(bg)
            } else {
                Style::default().fg(MUTED).bg(bg)
            };
            frame.render_widget(Paragraph::new(prefix).style(p_style), Rect::new(x, y, 1, 1));
            x += 1;
            // type tag
            let t_style = Style::default().fg(type_col).bg(bg);
            frame.render_widget(Paragraph::new(type_tag.as_str()).style(t_style), Rect::new(x, y, 5, 1));
            x += 5;
            // space
            x += 1;
            // name
            let n_style = if is_selected {
                Style::default().fg(if entry.is_dir { DIR_C } else { FG })
                    .add_modifier(Modifier::BOLD).bg(bg)
            } else {
                Style::default().fg(name_col).bg(bg)
            };
            let nw = name_w.min((row_rect.x + row_rect.width).saturating_sub(x) as usize);
            if nw > 0 {
                frame.render_widget(Paragraph::new(display_name.as_str()).style(n_style), Rect::new(x, y, nw as u16, 1));
            }
            x += nw as u16;
            // size
            if has_size && x + 1 < row_rect.x + row_rect.width {
                x += 1;
                let avail = (row_rect.x + row_rect.width).saturating_sub(x) as usize;
                let sw = size_w.min(avail);
                if sw > 0 {
                    let size_str = FileEntry::format_size(entry.size, entry.is_dir);
                    frame.render_widget(
                        Paragraph::new(size_str.as_str()).style(Style::default().fg(MUTED).bg(bg)),
                        Rect::new(x, y, sw as u16, 1),
                    );
                }
                x += sw as u16;
            }
            // type label (verbose)
            if has_type && x + 1 < row_rect.x + row_rect.width {
                x += 1;
                let avail = (row_rect.x + row_rect.width).saturating_sub(x) as usize;
                let tw = type_w.min(avail);
                if tw > 0 {
                    let tl = format!("{:<type_w$}", entry.type_label, type_w = tw);
                    frame.render_widget(
                        Paragraph::new(tl.as_str()).style(Style::default().fg(type_col).bg(bg)),
                        Rect::new(x, y, tw as u16, 1),
                    );
                }
                x += tw as u16;
            }
            // date
            if has_date && x + 1 < row_rect.x + row_rect.width {
                x += 1;
                let avail = (row_rect.x + row_rect.width).saturating_sub(x) as usize;
                let dw = date_w.min(avail);
                if dw > 0 {
                    let d = if entry.modified.is_empty() { "-".to_string() } else { entry.modified.clone() };
                    frame.render_widget(
                        Paragraph::new(d.as_str()).style(Style::default().fg(MUTED).bg(bg)),
                        Rect::new(x, y, dw as u16, 1),
                    );
                }
                x += dw as u16;
            }
            // owner
            if has_owner && x + 1 < row_rect.x + row_rect.width {
                x += 1;
                let avail = (row_rect.x + row_rect.width).saturating_sub(x) as usize;
                let ow = owner_w.min(avail);
                if ow > 0 {
                    let own = if entry.owner.len() > ow {
                        entry.owner[..ow].to_string()
                    } else {
                        format!("{:<width$}", entry.owner, width = ow)
                    };
                    frame.render_widget(
                        Paragraph::new(own.as_str()).style(Style::default().fg(MUTED).bg(bg)),
                        Rect::new(x, y, ow as u16, 1),
                    );
                }
            }
        }

        // Empty directory message
        if self.entries.is_empty() || (self.entries.len() == 1 && self.entries[0].name == "..") {
            let empty_y = list_area.y + list_area.height / 2;
            frame.render_widget(
                Paragraph::new("(empty directory)")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(MUTED)),
                Rect::new(list_area.x, empty_y, list_area.width, 1),
            );
        }
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(area);

        // Status bar
        let total_size: u64 = if self.selected_set.is_empty() {
            self.entries.get(self.selected).map(|e| e.size).unwrap_or(0)
        } else {
            self.selected_set.iter().filter_map(|&i| self.entries.get(i)).map(|e| e.size).sum()
        };
        let sel_count = if self.selected_set.is_empty() {
            if self.entries.is_empty() { 0 } else { 1 }
        } else {
            self.selected_set.len()
        };
        let clip_info = match &self.clipboard {
            Some(c) => {
                let op = match c.op { ClipOp::Copy => "copy", ClipOp::Cut => "cut" };
                format!("  │  {} {} item(s) in clipboard", op, c.names.len())
            }
            None => String::new(),
        };
        let status_text = if !self.status_msg.is_empty() {
            let col = if self.status_ok { OK } else { ERR };
            let pfx = if self.status_ok { "✓ " } else { "✗ " };
            frame.render_widget(
                Paragraph::new(format!("{}{}", pfx, &self.status_msg))
                    .style(Style::default().fg(col)),
                chunks[0],
            );
        } else {
            let item_word = if self.entries.len() == 1 { "item" } else { "items" };
            let entry_count = self.entries.iter().filter(|e| e.name != "..").count();
            let info = format!(
                "  {} {}  │  {} selected ({}){}",
                entry_count, item_word,
                sel_count,
                FileEntry::format_size(total_size, false).trim(),
                clip_info,
            );
            frame.render_widget(
                Paragraph::new(info).style(Style::default().fg(MUTED)),
                chunks[0],
            );
        };
        let _ = status_text;

        // Key hints bar (context-sensitive, changes when sidebar is focused)
        let hints: &str = match self.mode {
            Mode::Browse if self.active_panel == ActivePanel::Sidebar => {
                " [↑↓/j/k]move  [Enter/→]open  [Tab]→ files  [Esc]exit"
            }
            Mode::Browse => {
                " [↑↓]nav  [Enter]open  [Tab]← places  [n]Dir  [t]File  [F2]rename  [Del]delete  [C-c/x/v]clip  [f]search  [g]goto  [[/]]back/fwd"
            }
            Mode::ViewFile => " [↑↓/PgUp/Dn]scroll  [Home/End]jump  [Esc/q]back",
            Mode::NewDir   => " [Enter]create  [Esc]cancel",
            Mode::NewFile  => " [Enter]create  [Esc]cancel",
            Mode::Rename   => " [Enter]rename  [Esc]cancel",
            Mode::JumpTo   => " [Enter]go  [Esc]cancel",
            Mode::Search   => " [↑↓]results  [Enter]jump to  [Esc]cancel",
            Mode::DeleteConfirm => " [Y]confirm delete  [any other key]cancel",
            Mode::Properties => " [any key]close",
        };
        frame.render_widget(
            Paragraph::new(hints).style(Style::default().fg(MUTED)),
            chunks[1],
        );
    }

    fn render_input_overlay(&self, frame: &mut Frame, area: Rect) {
        let (title, prompt) = match self.mode {
            Mode::NewDir  => ("Create Directory", "Directory name:"),
            Mode::NewFile => ("Create File",      "File name:"),
            Mode::Rename  => ("Rename",           "New name:"),
            Mode::JumpTo  => ("Jump to Path",     "Path:"),
            Mode::Search  => ("Search",           "Search:"),
            _             => return,
        };

        let w = (area.width as usize).min(60) as u16;
        let h = if self.mode == Mode::Search && !self.search_results.is_empty() {
            5u16 + self.search_results.len().min(8) as u16
        } else {
            5
        };
        let x = area.x + area.width.saturating_sub(w) / 2;
        let y = area.y + area.height.saturating_sub(h) / 2;
        let popup = Rect::new(x, y, w, h);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(ACCENT))
            .title(Span::styled(format!(" {} ", title), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)))
            .style(Style::default().bg(PANEL));
        let inner = block.inner(popup);
        frame.render_widget(Clear, popup);
        frame.render_widget(block, popup);

        // Input line
        frame.render_widget(
            Paragraph::new(format!("{} {}▌", prompt, self.input_buffer))
                .style(Style::default().fg(FG)),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );

        // Search results
        if self.mode == Mode::Search && !self.search_results.is_empty() {
            let result_area = Rect::new(inner.x, inner.y + 2, inner.width, inner.height.saturating_sub(2));
            for (i, name) in self.search_results.iter().enumerate().take(result_area.height as usize) {
                let is_sel = i == self.search_sel;
                let style = if is_sel {
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(FG)
                };
                let prefix = if is_sel { "▶ " } else { "  " };
                frame.render_widget(
                    Paragraph::new(format!("{}{}", prefix, name)).style(style),
                    Rect::new(result_area.x, result_area.y + i as u16, result_area.width, 1),
                );
            }
        }
    }

    fn render_delete_confirm_overlay(&self, frame: &mut Frame, area: Rect) {
        let names = self.effective_names();
        let count = names.len();
        let preview = if count == 1 {
            format!("'{}'", names[0])
        } else if count <= 3 {
            names.iter().map(|n| format!("'{}'", n)).collect::<Vec<_>>().join(", ")
        } else {
            format!("{} items", count)
        };

        let w = 50u16.min(area.width);
        let h = 7u16;
        let x = area.x + area.width.saturating_sub(w) / 2;
        let y = area.y + area.height.saturating_sub(h) / 2;
        let popup = Rect::new(x, y, w, h);

        frame.render_widget(Clear, popup);
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ERR))
                .title(Span::styled(" Delete Confirmation ", Style::default().fg(ERR).add_modifier(Modifier::BOLD)))
                .style(Style::default().bg(PANEL)),
            popup,
        );
        let inner = Rect::new(popup.x + 2, popup.y + 1, popup.width.saturating_sub(4), popup.height.saturating_sub(2));
        frame.render_widget(
            Paragraph::new(format!("Delete {}?", preview))
                .style(Style::default().fg(WARN))
                .wrap(Wrap { trim: false }),
            Rect::new(inner.x, inner.y + 1, inner.width, 2),
        );
        frame.render_widget(
            Paragraph::new("  [Y] Yes — permanently delete   [Any key] Cancel")
                .style(Style::default().fg(FG)),
            Rect::new(inner.x, inner.y + 4, inner.width, 1),
        );
    }

    fn render_properties_overlay(&self, frame: &mut Frame, area: Rect) {
        let entry = match &self.prop_entry { Some(e) => e, None => return };

        let w = 54u16.min(area.width);
        let h = 16u16.min(area.height);
        let x = area.x + area.width.saturating_sub(w) / 2;
        let y = area.y + area.height.saturating_sub(h) / 2;
        let popup = Rect::new(x, y, w, h);

        frame.render_widget(Clear, popup);
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ACCENT))
                .title(Span::styled(" Properties ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)))
                .style(Style::default().bg(PANEL)),
            popup,
        );
        let inner = Rect::new(popup.x + 2, popup.y + 1, popup.width.saturating_sub(4), popup.height.saturating_sub(2));

        let kind  = if entry.is_dir { "Directory" } else { "File" };
        let size  = if entry.is_dir { "-".to_string() } else { FileEntry::format_size(entry.size, false).trim().to_string() };
        let perms_octal = format!("{:04o}", entry.perms);
        let perms_str   = perms_to_rwx(entry.perms);

        let rows: &[(&str, String)] = &[
            ("Name",       entry.name.clone()),
            ("Path",       self.prop_path.clone()),
            ("Type",       format!("{} [{}]", kind, entry.type_label)),
            ("Size",       size),
            ("Owner",      entry.owner.clone()),
            ("Permissions",format!("{} ({})", perms_str, perms_octal)),
            ("Modified",   entry.modified.clone()),
            ("Created",    entry.created.clone()),
        ];

        for (i, (label, value)) in rows.iter().enumerate() {
            if i as u16 >= inner.height { break; }
            let y_row = inner.y + i as u16;
            let label_w = 12u16;
            frame.render_widget(
                Paragraph::new(format!("{:<12}", label))
                    .style(Style::default().fg(MUTED)),
                Rect::new(inner.x, y_row, label_w, 1),
            );
            let val_w = inner.width.saturating_sub(label_w + 1);
            let val_display = if value.len() > val_w as usize {
                format!("…{}", &value[value.len().saturating_sub(val_w as usize - 1)..])
            } else {
                value.clone()
            };
            frame.render_widget(
                Paragraph::new(val_display.as_str())
                    .style(Style::default().fg(FG)),
                Rect::new(inner.x + label_w + 1, y_row, val_w, 1),
            );
        }

        // Hint
        let hint_y = popup.y + popup.height.saturating_sub(2);
        frame.render_widget(
            Paragraph::new("  any key to close")
                .style(Style::default().fg(MUTED)),
            Rect::new(popup.x + 1, hint_y, popup.width.saturating_sub(2), 1),
        );
    }

    fn render_file_viewer(&self, frame: &mut Frame, area: Rect) {
        let filename = self.view_path.rsplit('/').next().unwrap_or(&self.view_path);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .title(Span::styled(
                format!(" {} ", filename),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(PANEL));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let lines: Vec<&str> = self.file_content.lines().collect();
        let total  = lines.len();
        let visible= inner.height as usize;
        let max_sc = total.saturating_sub(visible);
        let scroll = self.file_scroll.min(max_sc);

        for (i, line) in lines.iter().skip(scroll).take(visible).enumerate() {
            let y = inner.y + i as u16;
            let max_w = inner.width as usize;
            let display = if line.len() > max_w { &line[..max_w] } else { line };
            frame.render_widget(
                Paragraph::new(display).style(Style::default().fg(FG)),
                Rect::new(inner.x, y, inner.width, 1),
            );
        }

        // Scrollbar indicator (top-right)
        if total > 0 {
            let pct = (scroll * 100) / total.max(1);
            let info = format!("{}/{} ({}%)", scroll + 1, total, pct);
            let info_x = inner.x + inner.width.saturating_sub(info.len() as u16 + 1);
            frame.render_widget(
                Paragraph::new(info.as_str()).style(Style::default().fg(MUTED)),
                Rect::new(info_x, inner.y, info.len() as u16 + 1, 1),
            );
        }
    }
}

// ── App trait ─────────────────────────────────────────────────────────────────

impl App for FilesApp {
    fn id(&self)   -> &str { "files" }
    fn name(&self) -> &str { "NeuraFiles" }

    fn init(&mut self) -> anyhow::Result<()> {
        self.needs_refresh_flag = true;
        Ok(())
    }

    fn on_resume(&mut self) {
        self.needs_refresh_flag = true;
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match self.mode {
            Mode::Browse          => self.handle_browse(key),
            Mode::ViewFile        => self.handle_view_file(key),
            Mode::DeleteConfirm   => self.handle_delete_confirm(key),
            Mode::Properties      => { self.mode = Mode::Browse; true }
            Mode::NewDir  | Mode::NewFile | Mode::Rename |
            Mode::JumpTo  | Mode::Search  => self.handle_input_mode(key),
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        // Background
        frame.render_widget(
            Block::default().style(Style::default().bg(BG)),
            area,
        );

        if self.mode == Mode::ViewFile {
            // Full-screen file viewer
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(1)])
                .split(area);
            self.render_file_viewer(frame, chunks[0]);
            frame.render_widget(
                Paragraph::new(
                    " [↑↓/PgUp/Dn]scroll  [Home/End]jump  [Esc/q/←]back"
                ).style(Style::default().fg(MUTED)),
                chunks[1],
            );
            return;
        }

        // Main layout: header / body / footer
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(2),
            ])
            .split(area);

        self.render_header(frame, main_chunks[0]);

        // Body: sidebar + content
        let sidebar_w = if main_chunks[1].width >= 60 { 20u16 } else { 0 };
        let body_chunks = if sidebar_w > 0 {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(sidebar_w), Constraint::Min(20)])
                .split(main_chunks[1])
        } else {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(20)])
                .split(main_chunks[1])
        };

        if sidebar_w > 0 {
            self.render_sidebar(frame, body_chunks[0]);
            self.render_content(frame, body_chunks[1]);
        } else {
            self.render_content(frame, body_chunks[0]);
        }

        self.render_footer(frame, main_chunks[2]);

        // Overlays
        match self.mode {
            Mode::NewDir | Mode::NewFile | Mode::Rename |
            Mode::JumpTo | Mode::Search => {
                self.render_input_overlay(frame, area);
            }
            Mode::DeleteConfirm => {
                self.render_delete_confirm_overlay(frame, area);
            }
            Mode::Properties => {
                self.render_properties_overlay(frame, area);
            }
            _ => {}
        }
    }

    fn on_close(&mut self) {}

    fn save_state(&self) -> Option<Value> {
        Some(serde_json::json!({ "cwd": self.cwd }))
    }

    fn load_state(&mut self, state: Value) {
        if let Some(cwd) = state.get("cwd").and_then(|v| v.as_str()) {
            // Only restore cwd if user is allowed to access it
            if self.can_access(cwd) {
                self.cwd = cwd.to_string();
            }
            self.needs_refresh_flag = true;
        }
    }

    fn ai_tools(&self) -> Vec<Value> { vec![] }
    fn handle_ai_tool(&mut self, _: &str, _: Value) -> Option<Value> { None }
    fn as_any(&self)     -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

// ── VFS helpers ───────────────────────────────────────────────────────────────

fn join_path(dir: &str, name: &str) -> String {
    if dir == "/" { format!("/{}", name) } else { format!("{}/{}", dir, name) }
}

fn parent_of(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() <= 1 { "/".to_string() } else { format!("/{}", parts[..parts.len()-1].join("/")) }
}

/// Recursively copy src → dst in VFS. Handles both files and directories.
async fn copy_vfs_recursive(vfs: &Vfs, src: &str, dst: &str, username: &str) -> Result<(), String> {
    match vfs.read_file(src).await {
        Ok(data) => {
            // It's a file
            vfs.write_file(dst, data, username).await.map_err(|e| e.to_string())
        }
        Err(_) => {
            // Try as directory
            vfs.mkdir(dst, username).await.map_err(|e| e.to_string())?;
            match vfs.list_dir(src).await {
                Ok(children) => {
                    for child in children {
                        let child_src = format!("{}/{}", src, child);
                        let child_dst = format!("{}/{}", dst, child);
                        Box::pin(copy_vfs_recursive(vfs, &child_src, &child_dst, username)).await?;
                    }
                    Ok(())
                }
                Err(e) => Err(e.to_string()),
            }
        }
    }
}

/// Recursively remove a path from VFS (handles non-empty directories).
async fn remove_vfs_recursive(vfs: &Vfs, path: &str) {
    // Try direct remove first (works for files and empty dirs)
    if vfs.remove(path).await.is_ok() { return; }
    // Non-empty directory: remove children first
    if let Ok(children) = vfs.list_dir(path).await {
        for child in children {
            let child_path = format!("{}/{}", path, child);
            Box::pin(remove_vfs_recursive(vfs, &child_path)).await;
        }
    }
    let _ = vfs.remove(path).await;
}

/// Convert permission bits to rwxrwxrwx string.
fn perms_to_rwx(mode: u16) -> String {
    let bits = [
        (0o400, 'r'), (0o200, 'w'), (0o100, 'x'),
        (0o040, 'r'), (0o020, 'w'), (0o010, 'x'),
        (0o004, 'r'), (0o002, 'w'), (0o001, 'x'),
    ];
    bits.iter().map(|(mask, c)| if mode & mask != 0 { *c } else { '-' }).collect()
}
