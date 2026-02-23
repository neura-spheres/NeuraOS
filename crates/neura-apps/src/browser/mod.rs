use std::any::Any;
use std::sync::Arc;
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui::style::{Style, Modifier};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::Value;
use neura_app_framework::app_trait::App;
use neura_app_framework::palette::*;
use neura_storage::vfs::Vfs;

// ── Content Line ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum LineKind {
    Empty,
    Normal,
    H1,
    H2,
    H3,
    Code,
    Quote,
    Bullet,
    Numbered(usize),
    Separator,
    _Link,
}

#[derive(Debug, Clone)]
struct ContentLine {
    kind: LineKind,
    text: String,
    _url: Option<String>,
}

impl ContentLine {
    fn normal(t: impl Into<String>) -> Self { Self { kind: LineKind::Normal, text: t.into(), _url: None } }
    fn empty() -> Self { Self { kind: LineKind::Empty, text: String::new(), _url: None } }
    fn heading(level: u8, t: impl Into<String>) -> Self {
        let kind = match level { 1 => LineKind::H1, 2 => LineKind::H2, _ => LineKind::H3 };
        Self { kind, text: t.into(), _url: None }
    }
    fn bullet(t: impl Into<String>) -> Self { Self { kind: LineKind::Bullet, text: t.into(), _url: None } }
    fn numbered(n: usize, t: impl Into<String>) -> Self { Self { kind: LineKind::Numbered(n), text: t.into(), _url: None } }
    fn code(t: impl Into<String>) -> Self { Self { kind: LineKind::Code, text: t.into(), _url: None } }
    fn quote(t: impl Into<String>) -> Self { Self { kind: LineKind::Quote, text: t.into(), _url: None } }
    fn separator() -> Self { Self { kind: LineKind::Separator, text: String::new(), _url: None } }
}

// ── Tab ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Tab {
    _id: usize,
    title: String,
    url: String,
    content: Vec<ContentLine>,
    links: Vec<(String, String)>,   // (display_text, full_url)
    scroll: usize,
    http_status: Option<u16>,
    bytes_loaded: usize,
    loading: bool,
    error: Option<String>,
    pending_fetch: Option<String>,
    is_secure: bool,
    word_count: usize,
    // Per-tab back/forward history
    nav_history: Vec<String>,
    nav_cursor: usize,
    is_history_nav: bool,
}

impl Tab {
    fn new(id: usize) -> Self {
        Self {
            _id: id,
            title: String::new(),
            url: String::new(),
            content: Vec::new(),
            links: Vec::new(),
            scroll: 0,
            http_status: None,
            bytes_loaded: 0,
            loading: false,
            error: None,
            pending_fetch: None,
            is_secure: false,
            word_count: 0,
            nav_history: Vec::new(),
            nav_cursor: 0,
            is_history_nav: false,
        }
    }

    fn display_title(&self, max: usize) -> String {
        if self.loading { return "⟳ Loading…".to_string(); }
        let raw = if !self.title.is_empty() {
            self.title.clone()
        } else if !self.url.is_empty() {
            let s = self.url.trim_start_matches("https://").trim_start_matches("http://");
            s.split('/').next().unwrap_or("New Tab").to_string()
        } else {
            "New Tab".to_string()
        };
        if raw.len() > max { format!("{}…", &raw[..max.saturating_sub(1)]) } else { raw }
    }

    fn reading_time(&self) -> String {
        if self.word_count < 50 { return String::new(); }
        let min = ((self.word_count as u32 + 199) / 200).max(1);
        format!("{} min read", min)
    }

    fn _domain(&self) -> String {
        let s = self.url.trim_start_matches("https://").trim_start_matches("http://");
        s.split('/').next().unwrap_or("").to_string()
    }
}

// ── History / Bookmarks ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct HistoryEntry {
    url: String,
    title: String,
    time: String,
}

#[derive(Debug, Clone)]
struct Bookmark {
    title: String,
    url: String,
}

// ── Modes ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum PanelMode { None, History, Bookmarks, Links }

#[derive(Debug, Clone, PartialEq)]
enum InputMode { UrlBar, Browsing, FindBar }

// ── BrowserApp ────────────────────────────────────────────────────────────────

pub struct BrowserApp {
    vfs: Arc<Vfs>,
    username: String,
    // Tabs
    tabs: Vec<Tab>,
    active_tab: usize,
    next_tab_id: usize,
    // URL bar
    url_input: String,
    url_cursor: usize,
    url_suggestions: Vec<String>,
    suggestion_sel: Option<usize>,
    // Global history (for suggestions + panel)
    global_history: Vec<HistoryEntry>,
    history_list_sel: usize,
    // Bookmarks
    bookmarks: Vec<Bookmark>,
    bookmark_selected: usize,
    // State
    input_mode: InputMode,
    panel_mode: PanelMode,
    // Find in page
    find_query: String,
    find_cursor: usize,
    find_results: Vec<usize>,
    find_sel: usize,
    // Links panel
    link_selected: usize,
    // Reading mode
    reading_mode: bool,
    // Status
    status_msg: String,
    // Async
    pub pending_fetch: Option<String>,   // kept for compatibility
    needs_load: bool,
    needs_save_bookmarks: bool,
}

impl BrowserApp {
    pub fn new(vfs: Arc<Vfs>, username: &str) -> Self {
        let mut tabs = Vec::new();
        tabs.push(Tab::new(0));
        Self {
            vfs,
            username: username.to_string(),
            tabs,
            active_tab: 0,
            next_tab_id: 1,
            url_input: String::new(),
            url_cursor: 0,
            url_suggestions: Vec::new(),
            suggestion_sel: None,
            global_history: Vec::new(),
            history_list_sel: 0,
            bookmarks: Vec::new(),
            bookmark_selected: 0,
            input_mode: InputMode::UrlBar,
            panel_mode: PanelMode::None,
            find_query: String::new(),
            find_cursor: 0,
            find_results: Vec::new(),
            find_sel: 0,
            link_selected: 0,
            reading_mode: false,
            status_msg: String::new(),
            pending_fetch: None,
            needs_load: true,
            needs_save_bookmarks: false,
        }
    }

    // ── Public flags ──────────────────────────────────────────────────────────

    pub fn needs_fetch(&self) -> bool {
        self.tabs.iter().any(|t| t.pending_fetch.is_some())
    }
    pub fn needs_data_load(&self) -> bool { self.needs_load }
    pub fn needs_bookmark_save(&self) -> bool { self.needs_save_bookmarks }

    // ── Tab helpers ───────────────────────────────────────────────────────────

    fn tab(&self) -> &Tab { &self.tabs[self.active_tab] }
    fn tab_mut(&mut self) -> &mut Tab { &mut self.tabs[self.active_tab] }

    fn open_new_tab(&mut self, url: Option<String>) {
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        let mut tab = Tab::new(id);
        if let Some(u) = url {
            let resolved = resolve_url(&u);
            tab.nav_history.push(resolved.clone());
            tab.nav_cursor = 0;
            tab.pending_fetch = Some(resolved);
            tab.loading = true;
        }
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        self.input_mode = if self.tabs[self.active_tab].loading { InputMode::Browsing } else { InputMode::UrlBar };
        self.url_input.clear();
        self.url_cursor = 0;
        self.panel_mode = PanelMode::None;
        self.find_query.clear();
        self.find_results.clear();
        self.status_msg = "New tab — type a URL and press Enter.".to_string();
    }

    fn close_active_tab(&mut self) {
        if self.tabs.len() == 1 {
            self.tabs[0] = Tab::new(0);
            self.input_mode = InputMode::UrlBar;
            self.url_input.clear();
            self.status_msg = "Tab cleared.".to_string();
            return;
        }
        self.tabs.remove(self.active_tab);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        self.panel_mode = PanelMode::None;
    }

    fn next_tab(&mut self) { self.active_tab = (self.active_tab + 1) % self.tabs.len(); }
    fn prev_tab(&mut self) {
        if self.active_tab == 0 { self.active_tab = self.tabs.len() - 1; } else { self.active_tab -= 1; }
    }

    // ── Navigation ────────────────────────────────────────────────────────────

    fn trigger_navigate(&mut self, raw: String) {
        let url = resolve_url(&raw);
        let tab = self.tab_mut();
        // Truncate forward history
        if !tab.nav_history.is_empty() {
            tab.nav_history.truncate(tab.nav_cursor + 1);
        }
        tab.nav_history.push(url.clone());
        tab.nav_cursor = tab.nav_history.len() - 1;
        tab.pending_fetch = Some(url);
        tab.loading = true;
        tab.error = None;
        tab.is_history_nav = false;
        self.input_mode = InputMode::Browsing;
        self.panel_mode = PanelMode::None;
        self.find_query.clear();
        self.find_results.clear();
    }

    fn navigate_back(&mut self) {
        let tab = self.tab_mut();
        if tab.nav_cursor > 0 {
            tab.nav_cursor -= 1;
            let url = tab.nav_history[tab.nav_cursor].clone();
            tab.pending_fetch = Some(url);
            tab.loading = true;
            tab.is_history_nav = true;
            tab.error = None;
        } else {
            self.status_msg = "No previous page.".to_string();
        }
    }

    fn navigate_forward(&mut self) {
        let tab = self.tab_mut();
        if tab.nav_cursor + 1 < tab.nav_history.len() {
            tab.nav_cursor += 1;
            let url = tab.nav_history[tab.nav_cursor].clone();
            tab.pending_fetch = Some(url);
            tab.loading = true;
            tab.is_history_nav = true;
            tab.error = None;
        } else {
            self.status_msg = "No next page.".to_string();
        }
    }

    fn refresh_tab(&mut self) {
        let url = self.tabs[self.active_tab].url.clone();
        if !url.is_empty() {
            let tab = self.tab_mut();
            tab.pending_fetch = Some(url);
            tab.loading = true;
            tab.is_history_nav = true;
            tab.error = None;
        }
    }

    fn bookmark_current(&mut self) {
        let url = self.tab().url.clone();
        let title = self.tab().title.clone();
        if url.is_empty() { self.status_msg = "No page loaded.".to_string(); return; }
        if self.bookmarks.iter().any(|b| b.url == url) {
            self.status_msg = "Already bookmarked.".to_string();
            return;
        }
        let display = if title.is_empty() { url.clone() } else { title.clone() };
        self.bookmarks.push(Bookmark { title, url });
        self.needs_save_bookmarks = true;
        self.status_msg = format!("★  Bookmarked: {}", display);
    }

    fn remove_bookmark_current(&mut self) {
        let url = self.tab().url.clone();
        if let Some(pos) = self.bookmarks.iter().position(|b| b.url == url) {
            self.bookmarks.remove(pos);
            self.needs_save_bookmarks = true;
            self.status_msg = "Bookmark removed.".to_string();
        }
    }

    fn is_bookmarked(&self) -> bool {
        let url = &self.tabs[self.active_tab].url;
        !url.is_empty() && self.bookmarks.iter().any(|b| &b.url == url)
    }

    // ── URL autocomplete ──────────────────────────────────────────────────────

    fn update_suggestions(&mut self) {
        if self.url_input.len() < 2 {
            self.url_suggestions.clear();
            self.suggestion_sel = None;
            return;
        }
        let query = self.url_input.to_lowercase();
        let mut seen = std::collections::HashSet::new();
        self.url_suggestions = self.global_history.iter().rev()
            .filter_map(|h| {
                if (h.url.to_lowercase().contains(&query) || h.title.to_lowercase().contains(&query))
                    && seen.insert(h.url.clone())
                {
                    Some(h.url.clone())
                } else { None }
            })
            .take(5)
            .collect();
        // Also add bookmarks
        for bm in &self.bookmarks {
            if (bm.url.to_lowercase().contains(&query) || bm.title.to_lowercase().contains(&query))
                && seen.insert(bm.url.clone())
                && self.url_suggestions.len() < 5
            {
                self.url_suggestions.push(bm.url.clone());
            }
        }
        if self.suggestion_sel.map_or(false, |s| s >= self.url_suggestions.len()) {
            self.suggestion_sel = None;
        }
    }

    // ── Find in page ──────────────────────────────────────────────────────────

    fn perform_find(&mut self) {
        if self.find_query.is_empty() {
            self.find_results.clear();
            self.find_sel = 0;
            return;
        }
        let q = self.find_query.to_lowercase();
        self.find_results = self.tabs[self.active_tab].content.iter().enumerate()
            .filter(|(_, l)| l.text.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect();
        self.find_sel = 0;
        if !self.find_results.is_empty() {
            let target = self.find_results[0];
            self.tabs[self.active_tab].scroll = target.saturating_sub(3);
            self.status_msg = format!("  {} match{} found", self.find_results.len(),
                if self.find_results.len() == 1 { "" } else { "es" });
        } else {
            self.status_msg = format!("  \"{}\" not found on this page", self.find_query);
        }
    }

    fn find_next(&mut self) {
        if self.find_results.is_empty() { return; }
        self.find_sel = (self.find_sel + 1) % self.find_results.len();
        let t = self.find_results[self.find_sel];
        self.tabs[self.active_tab].scroll = t.saturating_sub(3);
    }

    fn find_prev(&mut self) {
        if self.find_results.is_empty() { return; }
        self.find_sel = if self.find_sel == 0 { self.find_results.len() - 1 } else { self.find_sel - 1 };
        let t = self.find_results[self.find_sel];
        self.tabs[self.active_tab].scroll = t.saturating_sub(3);
    }

    // ── Async ─────────────────────────────────────────────────────────────────

    pub async fn async_load_data(&mut self) {
        self.needs_load = false;
        let path = format!("/home/{}/browser_bookmarks.json", self.username);
        if let Ok(data) = self.vfs.read_file(&path).await {
            if let Ok(arr) = serde_json::from_slice::<Vec<Value>>(&data) {
                self.bookmarks = arr.iter().filter_map(|b| Some(Bookmark {
                    title: b.get("title")?.as_str()?.to_string(),
                    url: b.get("url")?.as_str()?.to_string(),
                })).collect();
            }
        }
    }

    pub async fn async_save_bookmarks(&mut self) {
        self.needs_save_bookmarks = false;
        let path = format!("/home/{}/browser_bookmarks.json", self.username);
        let arr: Vec<Value> = self.bookmarks.iter().map(|b| serde_json::json!({"title": b.title, "url": b.url})).collect();
        if let Ok(bytes) = serde_json::to_vec(&arr) {
            let _ = self.vfs.write_file(&path, bytes, &self.username).await;
        }
    }

    pub async fn async_fetch(&mut self) {
        let tab_idx = match self.tabs.iter().position(|t| t.pending_fetch.is_some()) {
            Some(i) => i,
            None => return,
        };
        let url = self.tabs[tab_idx].pending_fetch.take().unwrap();
        let is_history_nav = self.tabs[tab_idx].is_history_nav;
        self.tabs[tab_idx].loading = true;
        self.tabs[tab_idx].is_history_nav = false;

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:125.0) Gecko/20100101 Firefox/125.0")
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                self.tabs[tab_idx].loading = false;
                self.tabs[tab_idx].error = Some(format!("Client error: {}", e));
                return;
            }
        };

        match client.get(&url).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let final_url = resp.url().to_string();
                let is_secure = final_url.starts_with("https://");
                match resp.bytes().await {
                    Ok(bytes) => {
                        let bytes_len = bytes.len();
                        let html = String::from_utf8_lossy(&bytes).to_string();
                        let (title, content, links) = parse_html(&html);
                        let word_count: usize = content.iter().map(|l| l.text.split_whitespace().count()).sum();

                        // If not a history nav, update nav_history cursor's URL to final (after redirect)
                        if !is_history_nav {
                            let tab = &mut self.tabs[tab_idx];
                            if let Some(last) = tab.nav_history.last_mut() {
                                *last = final_url.clone();
                            }
                        }

                        let tab = &mut self.tabs[tab_idx];
                        tab.title = if title.is_empty() { domain_from_url(&final_url) } else { title };
                        tab.url = final_url.clone();
                        tab.content = content;
                        tab.links = links;
                        tab.scroll = 0;
                        tab.http_status = Some(status);
                        tab.bytes_loaded = bytes_len;
                        tab.loading = false;
                        tab.error = None;
                        tab.is_secure = is_secure;
                        tab.word_count = word_count;

                        // Global history
                        let ts = chrono::Local::now().format("%b %d  %H:%M").to_string();
                        self.global_history.push(HistoryEntry {
                            url: final_url,
                            title: self.tabs[tab_idx].title.clone(),
                            time: ts,
                        });
                        if self.global_history.len() > 500 { self.global_history.remove(0); }

                        let rt = self.tabs[tab_idx].reading_time();
                        let kb = bytes_len as f64 / 1024.0;
                        self.status_msg = format!(
                            "HTTP {}  │  {:.1} KB  │  {} lines{}",
                            status, kb,
                            self.tabs[tab_idx].content.len(),
                            if rt.is_empty() { String::new() } else { format!("  │  {}", rt) }
                        );
                    }
                    Err(e) => {
                        self.tabs[tab_idx].loading = false;
                        self.tabs[tab_idx].error = Some(format!("Read error: {}", e));
                    }
                }
            }
            Err(e) => {
                self.tabs[tab_idx].loading = false;
                self.tabs[tab_idx].error = Some(format!("Connection failed: {}", e));
            }
        }
        if tab_idx == self.active_tab {
            self.input_mode = InputMode::Browsing;
        }
    }
}

// ── App trait ─────────────────────────────────────────────────────────────────

impl App for BrowserApp {
    fn id(&self) -> &str { "browser" }
    fn name(&self) -> &str { "NeuraBrowser" }
    fn init(&mut self) -> anyhow::Result<()> { Ok(()) }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt  = key.modifiers.contains(KeyModifiers::ALT);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);

        // ── Global tab shortcuts ──────────────────────────────────────────────
        match key.code {
            KeyCode::Char('t') if ctrl => { self.open_new_tab(None); return true; }
            KeyCode::Char('w') if ctrl => { self.close_active_tab(); return true; }
            KeyCode::Tab if ctrl && shift => { self.prev_tab(); return true; }
            KeyCode::Tab if ctrl => { self.next_tab(); return true; }
            KeyCode::BackTab if ctrl => { self.prev_tab(); return true; }
            KeyCode::Char('1') if ctrl => { if !self.tabs.is_empty() { self.active_tab = 0; } return true; }
            KeyCode::Char('2') if ctrl => { if self.tabs.len() > 1 { self.active_tab = 1; } return true; }
            KeyCode::Char('3') if ctrl => { if self.tabs.len() > 2 { self.active_tab = 2; } return true; }
            KeyCode::Char('4') if ctrl => { if self.tabs.len() > 3 { self.active_tab = 3; } return true; }
            KeyCode::Char('5') if ctrl => { if self.tabs.len() > 4 { self.active_tab = 4; } return true; }
            _ => {}
        }

        // ── Find bar ──────────────────────────────────────────────────────────
        if self.input_mode == InputMode::FindBar {
            match key.code {
                KeyCode::Esc => {
                    self.input_mode = InputMode::Browsing;
                    self.find_query.clear();
                    self.find_results.clear();
                }
                KeyCode::Enter => { self.find_next(); }
                KeyCode::Char('n') if ctrl => { self.find_next(); }
                KeyCode::Char('p') if ctrl => { self.find_prev(); }
                KeyCode::Char(c) => {
                    self.find_query.insert(self.find_cursor, c);
                    self.find_cursor += 1;
                    self.perform_find();
                }
                KeyCode::Backspace => {
                    if self.find_cursor > 0 {
                        self.find_cursor -= 1;
                        self.find_query.remove(self.find_cursor);
                        self.perform_find();
                    }
                }
                KeyCode::Delete => {
                    if self.find_cursor < self.find_query.len() {
                        self.find_query.remove(self.find_cursor);
                        self.perform_find();
                    }
                }
                KeyCode::Left  => { if self.find_cursor > 0 { self.find_cursor -= 1; } }
                KeyCode::Right => { if self.find_cursor < self.find_query.len() { self.find_cursor += 1; } }
                _ => {}
            }
            return true;
        }

        // ── URL bar ───────────────────────────────────────────────────────────
        if self.input_mode == InputMode::UrlBar {
            match key.code {
                KeyCode::Esc => {
                    let has_content = !self.tabs[self.active_tab].content.is_empty()
                        || self.tabs[self.active_tab].error.is_some();
                    if has_content {
                        self.input_mode = InputMode::Browsing;
                    } else if self.tabs.len() > 1 {
                        self.close_active_tab();
                    } else {
                        return false;
                    }
                    self.url_input.clear();
                    self.url_cursor = 0;
                    self.url_suggestions.clear();
                    self.suggestion_sel = None;
                }
                KeyCode::Enter => {
                    let query = if let Some(sel) = self.suggestion_sel {
                        self.url_suggestions.get(sel).cloned().unwrap_or_else(|| self.url_input.trim().to_string())
                    } else {
                        self.url_input.trim().to_string()
                    };
                    if !query.is_empty() {
                        self.url_input.clear();
                        self.url_cursor = 0;
                        self.url_suggestions.clear();
                        self.suggestion_sel = None;
                        self.trigger_navigate(query);
                    }
                }
                KeyCode::Tab => {
                    if let Some(sel) = self.suggestion_sel {
                        if let Some(s) = self.url_suggestions.get(sel).cloned() {
                            self.url_input = s;
                            self.url_cursor = self.url_input.len();
                            self.url_suggestions.clear();
                            self.suggestion_sel = None;
                        }
                    } else if !self.url_suggestions.is_empty() {
                        self.suggestion_sel = Some(0);
                    }
                }
                KeyCode::Up => {
                    if !self.url_suggestions.is_empty() {
                        self.suggestion_sel = Some(
                            self.suggestion_sel.map_or(0, |s| s.saturating_sub(1))
                        );
                    }
                }
                KeyCode::Down => {
                    if !self.url_suggestions.is_empty() {
                        self.suggestion_sel = Some(
                            self.suggestion_sel.map_or(0, |s| (s + 1).min(self.url_suggestions.len() - 1))
                        );
                    }
                }
                KeyCode::Char(c) => {
                    self.url_input.insert(self.url_cursor, c);
                    self.url_cursor += 1;
                    self.update_suggestions();
                    self.suggestion_sel = None;
                }
                KeyCode::Backspace => {
                    if self.url_cursor > 0 {
                        self.url_cursor -= 1;
                        self.url_input.remove(self.url_cursor);
                        self.update_suggestions();
                        self.suggestion_sel = None;
                    }
                }
                KeyCode::Delete => {
                    if self.url_cursor < self.url_input.len() {
                        self.url_input.remove(self.url_cursor);
                        self.update_suggestions();
                    }
                }
                KeyCode::Left  if ctrl => {
                    // Jump word left
                    while self.url_cursor > 0 && self.url_input.as_bytes().get(self.url_cursor - 1) == Some(&b' ') { self.url_cursor -= 1; }
                    while self.url_cursor > 0 && self.url_input.as_bytes().get(self.url_cursor - 1) != Some(&b' ') { self.url_cursor -= 1; }
                }
                KeyCode::Right if ctrl => {
                    // Jump word right
                    while self.url_cursor < self.url_input.len() && self.url_input.as_bytes().get(self.url_cursor) != Some(&b' ') { self.url_cursor += 1; }
                    while self.url_cursor < self.url_input.len() && self.url_input.as_bytes().get(self.url_cursor) == Some(&b' ') { self.url_cursor += 1; }
                }
                KeyCode::Left  => { if self.url_cursor > 0 { self.url_cursor -= 1; } }
                KeyCode::Right => { if self.url_cursor < self.url_input.len() { self.url_cursor += 1; } }
                KeyCode::Home  => { self.url_cursor = 0; }
                KeyCode::End   => { self.url_cursor = self.url_input.len(); }
                _ => {}
            }
            return true;
        }

        // ── Panel modes ───────────────────────────────────────────────────────
        match self.panel_mode.clone() {
            PanelMode::History => {
                match key.code {
                    KeyCode::Esc => { self.panel_mode = PanelMode::None; }
                    KeyCode::Up | KeyCode::Char('k') => { if self.history_list_sel > 0 { self.history_list_sel -= 1; } }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if self.history_list_sel + 1 < self.global_history.len() { self.history_list_sel += 1; }
                    }
                    KeyCode::Enter => {
                        let idx = self.global_history.len().saturating_sub(1).saturating_sub(self.history_list_sel);
                        if let Some(e) = self.global_history.get(idx) {
                            let url = e.url.clone();
                            self.panel_mode = PanelMode::None;
                            self.trigger_navigate(url);
                        }
                    }
                    KeyCode::Char('n') => {
                        let idx = self.global_history.len().saturating_sub(1).saturating_sub(self.history_list_sel);
                        if let Some(e) = self.global_history.get(idx) {
                            let url = e.url.clone();
                            self.panel_mode = PanelMode::None;
                            self.open_new_tab(Some(url));
                        }
                    }
                    KeyCode::Char('c') => {
                        self.global_history.clear();
                        self.history_list_sel = 0;
                        self.status_msg = "History cleared.".to_string();
                        self.panel_mode = PanelMode::None;
                    }
                    _ => {}
                }
                return true;
            }
            PanelMode::Bookmarks => {
                match key.code {
                    KeyCode::Esc => { self.panel_mode = PanelMode::None; }
                    KeyCode::Up | KeyCode::Char('k') => { if self.bookmark_selected > 0 { self.bookmark_selected -= 1; } }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if self.bookmark_selected + 1 < self.bookmarks.len() { self.bookmark_selected += 1; }
                    }
                    KeyCode::Enter => {
                        if let Some(bm) = self.bookmarks.get(self.bookmark_selected) {
                            let url = bm.url.clone();
                            self.panel_mode = PanelMode::None;
                            self.trigger_navigate(url);
                        }
                    }
                    KeyCode::Char('n') => {
                        if let Some(bm) = self.bookmarks.get(self.bookmark_selected) {
                            let url = bm.url.clone();
                            self.panel_mode = PanelMode::None;
                            self.open_new_tab(Some(url));
                        }
                    }
                    KeyCode::Char('d') => {
                        if self.bookmark_selected < self.bookmarks.len() {
                            self.bookmarks.remove(self.bookmark_selected);
                            if self.bookmark_selected > 0 && self.bookmark_selected >= self.bookmarks.len() {
                                self.bookmark_selected -= 1;
                            }
                            self.needs_save_bookmarks = true;
                            self.status_msg = "Bookmark deleted.".to_string();
                        }
                    }
                    _ => {}
                }
                return true;
            }
            PanelMode::Links => {
                let links_len = self.tabs[self.active_tab].links.len();
                match key.code {
                    KeyCode::Esc => { self.panel_mode = PanelMode::None; }
                    KeyCode::Up | KeyCode::Char('k') => { if self.link_selected > 0 { self.link_selected -= 1; } }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if self.link_selected + 1 < links_len { self.link_selected += 1; }
                    }
                    KeyCode::Enter => {
                        let url_opt = self.tabs[self.active_tab].links.get(self.link_selected).map(|(_, u)| u.clone());
                        if let Some(url) = url_opt {
                            self.panel_mode = PanelMode::None;
                            self.trigger_navigate(url);
                        }
                    }
                    KeyCode::Char('n') => {
                        let url_opt = self.tabs[self.active_tab].links.get(self.link_selected).map(|(_, u)| u.clone());
                        if let Some(url) = url_opt {
                            self.panel_mode = PanelMode::None;
                            self.open_new_tab(Some(url));
                        }
                    }
                    _ => {}
                }
                return true;
            }
            PanelMode::None => {}
        }

        // ── Browsing mode ─────────────────────────────────────────────────────
        let is_loading = self.tabs[self.active_tab].loading;

        match key.code {
            KeyCode::Esc => {
                if is_loading { return true; }
                return false;
            }

            // URL bar activation
            KeyCode::Char('l') if ctrl => {
                self.input_mode = InputMode::UrlBar;
                self.url_input = self.tabs[self.active_tab].url.clone();
                self.url_cursor = self.url_input.len();
                self.url_suggestions.clear();
                self.suggestion_sel = None;
            }
            KeyCode::Char('u') | KeyCode::F(6) if !ctrl && !alt => {
                self.input_mode = InputMode::UrlBar;
                self.url_input.clear();
                self.url_cursor = 0;
                self.url_suggestions.clear();
                self.suggestion_sel = None;
            }

            // Navigation
            KeyCode::Left  if alt => { self.navigate_back(); }
            KeyCode::Right if alt => { self.navigate_forward(); }
            KeyCode::Char('r') if ctrl => { self.refresh_tab(); }
            KeyCode::F(5)  => { self.refresh_tab(); }

            // Bookmarks
            KeyCode::Char('d') if ctrl => {
                if self.is_bookmarked() { self.remove_bookmark_current(); } else { self.bookmark_current(); }
            }

            // Panels
            KeyCode::Char('h') if ctrl => {
                self.history_list_sel = 0;
                self.panel_mode = PanelMode::History;
            }
            KeyCode::Char('b') if ctrl => {
                self.bookmark_selected = 0;
                self.panel_mode = PanelMode::Bookmarks;
            }
            KeyCode::Char('l') if !ctrl && !alt => {
                self.link_selected = 0;
                self.panel_mode = PanelMode::Links;
            }

            // Find in page
            KeyCode::Char('f') if ctrl => {
                self.input_mode = InputMode::FindBar;
                self.find_query.clear();
                self.find_cursor = 0;
                self.find_results.clear();
            }

            // Reading mode
            KeyCode::Char('m') if ctrl => {
                self.reading_mode = !self.reading_mode;
                self.status_msg = if self.reading_mode { "Reading mode ON".to_string() } else { "Reading mode OFF".to_string() };
            }

            // Scroll
            KeyCode::Up | KeyCode::Char('k') => {
                self.tabs[self.active_tab].scroll = self.tabs[self.active_tab].scroll.saturating_sub(3);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.tabs[self.active_tab].scroll = self.tabs[self.active_tab].scroll.saturating_add(3);
            }
            KeyCode::PageUp => {
                self.tabs[self.active_tab].scroll = self.tabs[self.active_tab].scroll.saturating_sub(20);
            }
            KeyCode::PageDown => {
                self.tabs[self.active_tab].scroll = self.tabs[self.active_tab].scroll.saturating_add(20);
            }
            KeyCode::Home => { self.tabs[self.active_tab].scroll = 0; }
            KeyCode::End  => { self.tabs[self.active_tab].scroll = usize::MAX; }

            // Open new tab with current link
            KeyCode::Char('T') if shift => { self.open_new_tab(None); }

            _ => {}
        }
        true
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        let has_find = self.input_mode == InputMode::FindBar;

        // Outer layout
        let mut constraints = vec![
            Constraint::Length(3),  // toolbar (with border)
            Constraint::Length(1),  // tab bar
            Constraint::Min(3),     // content
        ];
        if has_find { constraints.push(Constraint::Length(1)); }  // find bar
        constraints.push(Constraint::Length(1));                   // status bar

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints.clone())
            .split(area);

        let toolbar_area  = chunks[0];
        let tabbar_area   = chunks[1];
        let content_area  = chunks[2];
        let find_area     = if has_find { Some(chunks[chunks.len() - 2]) } else { None };
        let status_area   = chunks[chunks.len() - 1];

        self.render_toolbar(frame, toolbar_area);
        self.render_tab_bar(frame, tabbar_area);

        // Content + optional side panel
        if self.panel_mode != PanelMode::None {
            let split = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
                .split(content_area);
            self.render_content_area(frame, split[0]);
            self.render_panel(frame, split[1]);
        } else {
            self.render_content_area(frame, content_area);
        }

        if let Some(fa) = find_area { self.render_find_bar(frame, fa); }
        self.render_status_bar(frame, status_area);
    }

    fn on_resume(&mut self) { self.needs_load = true; }
    fn on_pause(&mut self) {}
    fn on_close(&mut self) {}

    fn save_state(&self) -> Option<Value> {
        let bms: Vec<Value> = self.bookmarks.iter().map(|b| serde_json::json!({"title": b.title, "url": b.url})).collect();
        let hist: Vec<Value> = self.global_history.iter().map(|h| serde_json::json!({"url": h.url, "title": h.title, "time": h.time})).collect();
        Some(serde_json::json!({ "bookmarks": bms, "history": hist }))
    }

    fn load_state(&mut self, state: Value) {
        if let Some(bms) = state.get("bookmarks").and_then(|v| v.as_array()) {
            self.bookmarks = bms.iter().filter_map(|b| Some(Bookmark {
                title: b.get("title")?.as_str()?.to_string(),
                url: b.get("url")?.as_str()?.to_string(),
            })).collect();
        }
        if let Some(hist) = state.get("history").and_then(|v| v.as_array()) {
            self.global_history = hist.iter().filter_map(|h| Some(HistoryEntry {
                url: h.get("url")?.as_str()?.to_string(),
                title: h.get("title")?.as_str()?.to_string(),
                time: h.get("time").and_then(|t| t.as_str()).unwrap_or("").to_string(),
            })).collect();
        }
        self.needs_load = false;
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

// ── Render helpers ────────────────────────────────────────────────────────────

impl BrowserApp {
    fn render_toolbar(&self, frame: &mut Frame, area: Rect) {
        let tab = &self.tabs[self.active_tab];
        let is_url_active = self.input_mode == InputMode::UrlBar;
        let border_clr = if is_url_active { PRIMARY } else { BORDER };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_clr))
            .style(Style::default().bg(BG));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        // [  nav_w  ][  url_w  ][  act_w  ]
        let nav_w: u16 = 10;
        let act_w: u16 = 7;
        let url_w = inner.width.saturating_sub(nav_w + act_w);

        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(nav_w), Constraint::Length(url_w), Constraint::Length(act_w)])
            .split(inner);

        // ── Nav buttons
        let can_back = tab.nav_cursor > 0;
        let can_fwd  = tab.nav_cursor + 1 < tab.nav_history.len();
        let nav_spans = vec![
            Span::styled(" ", Style::default()),
            Span::styled("←", Style::default().fg(if can_back { TEXT } else { DIM })),
            Span::styled(" ", Style::default()),
            Span::styled("→", Style::default().fg(if can_fwd { TEXT } else { DIM })),
            Span::styled(" ", Style::default()),
            Span::styled(if tab.loading { "⟳" } else { "↺" }, Style::default().fg(if tab.loading { ORANGE } else { TEXT })),
            Span::styled("  ⌂", Style::default().fg(CYAN)),
            Span::styled(" ", Style::default()),
        ];
        frame.render_widget(Paragraph::new(Line::from(nav_spans)), cols[0]);

        // ── URL bar
        let url_bar_clr = if is_url_active { PRIMARY } else { BORDER };
        let url_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(url_bar_clr));
        let url_inner = url_block.inner(cols[1]);
        frame.render_widget(url_block, cols[1]);

        let (sec_icon, sec_clr) = if tab.url.is_empty() {
            (" ", DIM)
        } else if tab.is_secure {
            ("🔒", GREEN)
        } else {
            ("⚠ ", ORANGE)
        };

        let display_url: &str = if is_url_active { &self.url_input } else { &tab.url };
        let avail_w = url_inner.width.saturating_sub(3) as usize;
        let url_display = if display_url.len() > avail_w {
            format!("…{}", &display_url[display_url.len().saturating_sub(avail_w)..])
        } else {
            display_url.to_string()
        };

        let url_text_clr = if is_url_active { TEXT } else { MUTED };
        let url_spans = vec![
            Span::styled(sec_icon, Style::default().fg(sec_clr)),
            Span::styled(" ", Style::default()),
            Span::styled(url_display, Style::default().fg(url_text_clr)),
        ];
        frame.render_widget(Paragraph::new(Line::from(url_spans)), url_inner);

        // Cursor in URL bar
        if is_url_active {
            let offset = self.url_input.len().saturating_sub(avail_w);
            let vis_cursor = self.url_cursor.saturating_sub(offset);
            let cx = url_inner.x + 2 + vis_cursor as u16;
            if cx < url_inner.x + url_inner.width {
                frame.set_cursor_position((cx, url_inner.y));
            }
        }

        // ── URL suggestions dropdown
        if is_url_active && !self.url_suggestions.is_empty() {
            let sug_h = self.url_suggestions.len().min(5) as u16;
            let sug_y = area.y + area.height;
            let sug_area = Rect { x: cols[1].x, y: sug_y, width: cols[1].width.min(60), height: sug_h };
            if sug_y + sug_h < sug_y + 20 {
                let sug_block = Block::default()
                    .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
                    .border_style(Style::default().fg(PRIMARY))
                    .style(Style::default().bg(PANEL));
                let sug_inner = sug_block.inner(sug_area);
                frame.render_widget(sug_block, sug_area);
                let items: Vec<ListItem> = self.url_suggestions.iter().enumerate().map(|(i, s)| {
                    let sel = self.suggestion_sel == Some(i);
                    let st = if sel { Style::default().fg(PRIMARY).bg(SEL_BG).add_modifier(Modifier::BOLD) }
                             else   { Style::default().fg(MUTED) };
                    ListItem::new(format!("{}{}", if sel { "▶ " } else { "  " }, s)).style(st)
                }).collect();
                frame.render_widget(List::new(items), sug_inner);
            }
        }

        // ── Action buttons  ★ ≡
        let star_clr = if self.is_bookmarked() { ORANGE } else { DIM };
        let act_spans = vec![
            Span::styled(" ", Style::default()),
            Span::styled("★", Style::default().fg(star_clr)),
            Span::styled("  ", Style::default()),
            Span::styled("≡", Style::default().fg(MUTED)),
            Span::styled(" ", Style::default()),
        ];
        frame.render_widget(Paragraph::new(Line::from(act_spans)), cols[2]);
    }

    fn render_tab_bar(&self, frame: &mut Frame, area: Rect) {
        let n = self.tabs.len();
        // Reserve space for [+] button (5 chars)
        let avail_w = area.width.saturating_sub(5);
        let per_tab_w = ((avail_w as usize) / n.max(1)).min(22).max(8) as u16;

        let mut spans: Vec<Span> = Vec::new();
        for (i, tab) in self.tabs.iter().enumerate() {
            let is_active = i == self.active_tab;
            let title = tab.display_title(per_tab_w.saturating_sub(4) as usize);
            if is_active {
                spans.push(Span::styled("[", Style::default().fg(PRIMARY)));
                spans.push(Span::styled(title, Style::default().fg(TEXT).add_modifier(Modifier::BOLD)));
                spans.push(Span::styled(" ×]", Style::default().fg(RED)));
            } else {
                spans.push(Span::styled(" ", Style::default().fg(DIM)));
                spans.push(Span::styled(title, Style::default().fg(DIM)));
                spans.push(Span::styled("  ", Style::default()));
            }
        }
        spans.push(Span::styled(" [+]", Style::default().fg(GREEN)));

        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(PANEL)),
            area,
        );
    }

    fn render_content_area(&self, frame: &mut Frame, area: Rect) {
        let tab = &self.tabs[self.active_tab];
        if tab.loading {
            self.render_loading(frame, area);
        } else if let Some(err) = &tab.error {
            self.render_error_page(frame, area, err);
        } else if tab.content.is_empty() {
            self.render_home(frame, area);
        } else {
            self.render_page(frame, area);
        }
    }

    fn render_loading(&self, frame: &mut Frame, area: Rect) {
        let tab = &self.tabs[self.active_tab];
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(CYAN))
            .title(" Connecting… ")
            .title_style(Style::default().fg(CYAN))
            .style(Style::default().bg(BG));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let mid = inner.height / 2;
        let short_url = if tab.url.len() > 60 { format!("{}…", &tab.url[..59]) } else { tab.url.clone() };

        let lines = vec![
            Line::from(""),
            Line::from(Span::styled("   ⟳  Fetching page…", Style::default().fg(CYAN).add_modifier(Modifier::BOLD))),
            Line::from(""),
            Line::from(Span::styled(format!("   {}", short_url), Style::default().fg(MUTED))),
            Line::from(""),
            Line::from(Span::styled("   Press Esc to cancel", Style::default().fg(DIM))),
        ];

        let load_area = Rect { x: inner.x, y: inner.y + mid.saturating_sub(3), width: inner.width, height: 6.min(inner.height) };
        frame.render_widget(Paragraph::new(lines), load_area);
    }

    fn render_error_page(&self, frame: &mut Frame, area: Rect, err: &str) {
        let tab = &self.tabs[self.active_tab];
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(RED))
            .title("  Cannot Reach Page ")
            .title_style(Style::default().fg(RED).add_modifier(Modifier::BOLD))
            .style(Style::default().bg(BG));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let short_url = if tab.url.len() > 70 { format!("{}…", &tab.url[..69]) } else { tab.url.clone() };
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled("  ✕  This page could not be loaded.", Style::default().fg(RED).add_modifier(Modifier::BOLD))),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Error: ", Style::default().fg(MUTED)),
                Span::styled(err, Style::default().fg(TEXT)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  URL:  ", Style::default().fg(MUTED)),
                Span::styled(short_url, Style::default().fg(DIM)),
            ]),
            Line::from(""),
            Line::from(Span::styled("  ─────────────────────────────────────────────────", Style::default().fg(BORDER))),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Ctrl+R", Style::default().fg(PRIMARY)),
                Span::styled("  Retry      ", Style::default().fg(MUTED)),
                Span::styled("  u", Style::default().fg(PRIMARY)),
                Span::styled("  New URL      ", Style::default().fg(MUTED)),
                Span::styled("  Alt+←", Style::default().fg(PRIMARY)),
                Span::styled("  Back", Style::default().fg(MUTED)),
            ]),
        ];
        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn render_home(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER))
            .style(Style::default().bg(BG));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("   ⌂  ", Style::default().fg(CYAN)),
            Span::styled("NeuraBrowser", Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled("  —  Modern CLI Web Browser", Style::default().fg(DIM)),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("   Press ", Style::default().fg(MUTED)),
            Span::styled("u", Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(" or ", Style::default().fg(MUTED)),
            Span::styled("Ctrl+L", Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(" to enter a URL or search term", Style::default().fg(MUTED)),
        ]));
        lines.push(Line::from(""));

        if !self.bookmarks.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("   ─── Bookmarks ", Style::default().fg(ORANGE)),
                Span::styled("─".repeat(30), Style::default().fg(BORDER)),
            ]));
            for bm in self.bookmarks.iter().take(8) {
                lines.push(Line::from(vec![
                    Span::styled("   ★  ", Style::default().fg(ORANGE)),
                    Span::styled(format!("{:<32}", bm.title.chars().take(32).collect::<String>()), Style::default().fg(TEXT)),
                    Span::styled(format!("  {}", bm.url.chars().take(50).collect::<String>()), Style::default().fg(DIM)),
                ]));
            }
            lines.push(Line::from(""));
        }

        if !self.global_history.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("   ─── Recently Visited ", Style::default().fg(CYAN)),
                Span::styled("─".repeat(25), Style::default().fg(BORDER)),
            ]));
            for entry in self.global_history.iter().rev().take(6) {
                lines.push(Line::from(vec![
                    Span::styled("   ●  ", Style::default().fg(DIM)),
                    Span::styled(format!("{:<35}", entry.title.chars().take(35).collect::<String>()), Style::default().fg(MUTED)),
                    Span::styled(format!("  {}", entry.time), Style::default().fg(DIM)),
                ]));
            }
        }

        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn render_page(&self, frame: &mut Frame, area: Rect) {
        let tab = &self.tabs[self.active_tab];
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER))
            .title(format!(" {} ", tab.title.chars().take(60).collect::<String>()))
            .title_style(Style::default().fg(GREEN).add_modifier(Modifier::BOLD))
            .style(Style::default().bg(BG));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let visible_h = inner.height as usize;
        let total = tab.content.len();
        let max_scroll = total.saturating_sub(visible_h);
        let scroll_off = if tab.scroll == usize::MAX { max_scroll } else { tab.scroll.min(max_scroll) };

        // Width available for text (subtract 2 for indent, 1 for scrollbar)
        let text_w = inner.width.saturating_sub(3) as usize;

        let lines: Vec<Line> = tab.content.iter()
            .enumerate()
            .skip(scroll_off)
            .take(visible_h)
            .map(|(idx, cl)| {
                let is_find_hit = self.input_mode == InputMode::FindBar && self.find_results.contains(&idx);
                let is_find_cur = self.input_mode == InputMode::FindBar && self.find_results.get(self.find_sel) == Some(&idx);

                let highlight_style = if is_find_cur {
                    Style::default().fg(BG).bg(ORANGE).add_modifier(Modifier::BOLD)
                } else if is_find_hit {
                    Style::default().fg(BG).bg(YELLOW)
                } else {
                    Style::default().fg(TEXT)
                };

                match &cl.kind {
                    LineKind::Empty => Line::from(""),

                    LineKind::H1 => {
                        Line::from(vec![
                            Span::styled("  ", Style::default()),
                            Span::styled(cl.text.chars().take(text_w).collect::<String>(),
                                Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)),
                        ])
                    }

                    LineKind::H2 => Line::from(vec![
                        Span::styled("  ", Style::default()),
                        Span::styled(cl.text.chars().take(text_w).collect::<String>(),
                            Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
                    ]),

                    LineKind::H3 => Line::from(vec![
                        Span::styled("  ", Style::default()),
                        Span::styled(cl.text.chars().take(text_w).collect::<String>(),
                            Style::default().fg(MAGENTA).add_modifier(Modifier::BOLD)),
                    ]),

                    LineKind::Bullet => Line::from(vec![
                        Span::styled("  • ", Style::default().fg(GREEN)),
                        Span::styled(cl.text.chars().take(text_w.saturating_sub(4)).collect::<String>(), highlight_style),
                    ]),

                    LineKind::Numbered(n) => Line::from(vec![
                        Span::styled(format!("  {}. ", n), Style::default().fg(GREEN)),
                        Span::styled(cl.text.chars().take(text_w.saturating_sub(5)).collect::<String>(), highlight_style),
                    ]),

                    LineKind::Code => Line::from(vec![
                        Span::styled("  │ ", Style::default().fg(BORDER)),
                        Span::styled(cl.text.chars().take(text_w.saturating_sub(4)).collect::<String>(),
                            Style::default().fg(ORANGE)),
                    ]),

                    LineKind::Quote => Line::from(vec![
                        Span::styled("  ▌ ", Style::default().fg(PURPLE)),
                        Span::styled(cl.text.chars().take(text_w.saturating_sub(4)).collect::<String>(),
                            Style::default().fg(MUTED).add_modifier(Modifier::ITALIC)),
                    ]),

                    LineKind::Separator => Line::from(vec![
                        Span::styled("  ", Style::default()),
                        Span::styled("─".repeat(text_w), Style::default().fg(BORDER)),
                    ]),

                    LineKind::_Link => Line::from(vec![
                        Span::styled("  ↗ ", Style::default().fg(PRIMARY)),
                        Span::styled(cl.text.chars().take(text_w.saturating_sub(4)).collect::<String>(),
                            Style::default().fg(PRIMARY).add_modifier(Modifier::UNDERLINED)),
                    ]),

                    LineKind::Normal => Line::from(vec![
                        Span::styled("  ", Style::default()),
                        Span::styled(cl.text.chars().take(text_w).collect::<String>(), highlight_style),
                    ]),
                }
            })
            .collect();

        frame.render_widget(Paragraph::new(lines), inner);

        // ── Scrollbar
        if total > visible_h && inner.height > 2 {
            let bar_h = inner.height.saturating_sub(0) as usize;
            let thumb_h = ((visible_h * bar_h) / total.max(1)).max(1);
            let thumb_top = (scroll_off * (bar_h.saturating_sub(thumb_h))) / max_scroll.max(1);

            for row in 0..inner.height {
                let r = row as usize;
                let is_thumb = r >= thumb_top && r < thumb_top + thumb_h;
                let sym = if is_thumb { "▐" } else { "│" };
                let clr = if is_thumb { PRIMARY } else { BORDER };
                frame.render_widget(
                    Paragraph::new(sym).style(Style::default().fg(clr)),
                    Rect { x: inner.x + inner.width, y: inner.y + row, width: 1, height: 1 },
                );
            }
        }
    }

    fn render_panel(&self, frame: &mut Frame, area: Rect) {
        match &self.panel_mode {
            PanelMode::History   => self.render_panel_history(frame, area),
            PanelMode::Bookmarks => self.render_panel_bookmarks(frame, area),
            PanelMode::Links     => self.render_panel_links(frame, area),
            PanelMode::None      => {}
        }
    }

    fn render_panel_history(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(CYAN))
            .title(" ● History ")
            .title_style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD))
            .style(Style::default().bg(PANEL));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.global_history.is_empty() {
            frame.render_widget(Paragraph::new("\n  No history yet.").style(Style::default().fg(DIM)), inner);
            return;
        }

        let visible_h = inner.height as usize;
        let items: Vec<ListItem> = self.global_history.iter().rev().enumerate()
            .take(visible_h)
            .map(|(i, e)| {
                let sel = i == self.history_list_sel;
                let base = if sel { Style::default().fg(CYAN).bg(SEL_BG).add_modifier(Modifier::BOLD) }
                           else   { Style::default().fg(TEXT) };
                let pfx = if sel { "▶ " } else { "  " };
                let w = inner.width.saturating_sub(4) as usize;
                let title = e.title.chars().take(w).collect::<String>();
                let url   = e.url.chars().take(w).collect::<String>();
                ListItem::new(vec![
                    Line::from(vec![Span::styled(format!("{}{}", pfx, title), base)]),
                    Line::from(vec![
                        Span::styled("    ", Style::default()),
                        Span::styled(url, Style::default().fg(DIM)),
                        Span::styled(format!("  {}", e.time), Style::default().fg(DIM)),
                    ]),
                ])
            })
            .collect();

        frame.render_widget(List::new(items), inner);

        let help = Rect { x: area.x + 1, y: area.y + area.height.saturating_sub(1), width: area.width.saturating_sub(2), height: 1 };
        frame.render_widget(
            Paragraph::new(" ↑↓ nav  Enter go  n new tab  c clear  Esc close").style(Style::default().fg(DIM)),
            help,
        );
    }

    fn render_panel_bookmarks(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ORANGE))
            .title(format!(" ★ Bookmarks ({}) ", self.bookmarks.len()))
            .title_style(Style::default().fg(ORANGE).add_modifier(Modifier::BOLD))
            .style(Style::default().bg(PANEL));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.bookmarks.is_empty() {
            frame.render_widget(
                Paragraph::new("\n  No bookmarks yet.\n\n  Press Ctrl+D on any page\n  to add it.").style(Style::default().fg(DIM)),
                inner,
            );
            return;
        }

        let items: Vec<ListItem> = self.bookmarks.iter().enumerate().map(|(i, bm)| {
            let sel = i == self.bookmark_selected;
            let base = if sel { Style::default().fg(ORANGE).bg(SEL_BG).add_modifier(Modifier::BOLD) }
                       else   { Style::default().fg(TEXT) };
            let pfx = if sel { "★ " } else { "  " };
            let w = inner.width.saturating_sub(4) as usize;
            ListItem::new(vec![
                Line::from(vec![Span::styled(format!("{}{}", pfx, bm.title.chars().take(w).collect::<String>()), base)]),
                Line::from(vec![
                    Span::styled("    ", Style::default()),
                    Span::styled(bm.url.chars().take(w).collect::<String>(), Style::default().fg(DIM)),
                ]),
            ])
        }).collect();

        frame.render_widget(List::new(items), inner);

        let help = Rect { x: area.x + 1, y: area.y + area.height.saturating_sub(1), width: area.width.saturating_sub(2), height: 1 };
        frame.render_widget(
            Paragraph::new(" ↑↓ nav  Enter go  n new tab  d delete  Esc close").style(Style::default().fg(DIM)),
            help,
        );
    }

    fn render_panel_links(&self, frame: &mut Frame, area: Rect) {
        let tab = &self.tabs[self.active_tab];
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(PRIMARY))
            .title(format!(" ↗ Links ({}) ", tab.links.len()))
            .title_style(Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD))
            .style(Style::default().bg(PANEL));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if tab.links.is_empty() {
            frame.render_widget(
                Paragraph::new("\n  No links found on this page.").style(Style::default().fg(DIM)),
                inner,
            );
            return;
        }

        let items: Vec<ListItem> = tab.links.iter().enumerate().map(|(i, (text, url))| {
            let sel = i == self.link_selected;
            let base = if sel { Style::default().fg(PRIMARY).bg(SEL_BG).add_modifier(Modifier::BOLD) }
                       else   { Style::default().fg(TEXT) };
            let pfx = if sel { "▶ " } else { "  " };
            let w = inner.width.saturating_sub(4) as usize;
            let label = if text.trim().is_empty() { url.chars().take(w).collect::<String>() } else { text.chars().take(w).collect::<String>() };
            ListItem::new(vec![
                Line::from(vec![Span::styled(format!("{}{}", pfx, label), base)]),
                Line::from(vec![
                    Span::styled("    ", Style::default()),
                    Span::styled(url.chars().take(w).collect::<String>(), Style::default().fg(DIM)),
                ]),
            ])
        }).collect();

        frame.render_widget(List::new(items), inner);

        let help = Rect { x: area.x + 1, y: area.y + area.height.saturating_sub(1), width: area.width.saturating_sub(2), height: 1 };
        frame.render_widget(
            Paragraph::new(" ↑↓ nav  Enter go  n new tab  Esc close").style(Style::default().fg(DIM)),
            help,
        );
    }

    fn render_find_bar(&self, frame: &mut Frame, area: Rect) {
        let result_info = if !self.find_query.is_empty() {
            if self.find_results.is_empty() {
                format!(" (no matches)")
            } else {
                format!(" ({}/{})", self.find_sel + 1, self.find_results.len())
            }
        } else { String::new() };

        let no_match = !self.find_query.is_empty() && self.find_results.is_empty();
        let result_clr = if no_match { RED } else { GREEN };

        let spans = vec![
            Span::styled("  🔍 ", Style::default().fg(PRIMARY)),
            Span::styled(&self.find_query, Style::default().fg(TEXT)),
            Span::styled(&result_info, Style::default().fg(result_clr)),
            Span::styled("   Enter/^N next  ^P prev  Esc close", Style::default().fg(DIM)),
        ];
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(PANEL)),
            area,
        );
        // Cursor
        let cx = area.x + 5 + self.find_cursor as u16;
        if cx < area.x + area.width { frame.set_cursor_position((cx, area.y)); }
    }

    fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
        let tab = &self.tabs[self.active_tab];
        let n_tabs = self.tabs.len();

        let (status_text, status_clr) = if tab.loading {
            (" ⟳ Loading…".to_string(), ORANGE)
        } else if let Some(code) = tab.http_status {
            let text = if code < 300 {
                format!(" ✓ {}  {:.1}KB{}", code, tab.bytes_loaded as f64 / 1024.0,
                    if tab.reading_time().is_empty() { String::new() } else { format!("  {}", tab.reading_time()) })
            } else {
                format!(" ✕ {}  {:.1}KB", code, tab.bytes_loaded as f64 / 1024.0)
            };
            let clr = if code >= 400 { RED } else if code >= 300 { YELLOW } else { GREEN };
            (text, clr)
        } else if !self.status_msg.is_empty() {
            (format!("  {}", self.status_msg), MUTED)
        } else {
            (" Ready ".to_string(), DIM)
        };

        let mode_text = match (&self.panel_mode, &self.input_mode) {
            (PanelMode::History, _)   => " History ",
            (PanelMode::Bookmarks, _) => " Bookmarks ",
            (PanelMode::Links, _)     => " Links ",
            (_, InputMode::UrlBar)    => " URL ",
            (_, InputMode::FindBar)   => " Find ",
            _                          => " Browse ",
        };

        let hint = if n_tabs > 1 {
            format!(" ^T tab  ^W close  ^Tab next  ^F find  l links  Alt+←→ nav  [{} tabs]", n_tabs)
        } else {
            " ^T new tab  ^F find  l links  ^D bookmark  ^H hist  ^B bkm  u URL  Alt+←→ back/fwd  Esc exit".to_string()
        };

        let spans = vec![
            Span::styled(&status_text, Style::default().fg(status_clr)),
            Span::styled("  │  ", Style::default().fg(BORDER)),
            Span::styled(mode_text, Style::default().fg(CYAN)),
            Span::styled("  │ ", Style::default().fg(BORDER)),
            Span::styled(&hint, Style::default().fg(DIM)),
        ];

        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(PANEL)),
            area,
        );
    }
}

// ── HTML Parser ───────────────────────────────────────────────────────────────

fn parse_html(html: &str) -> (String, Vec<ContentLine>, Vec<(String, String)>) {
    let mut title      = String::new();
    let mut content: Vec<ContentLine> = Vec::new();
    let mut links: Vec<(String, String)> = Vec::new();

    // Parser state
    let mut in_tag         = false;
    let mut in_script      = false;
    let mut in_style       = false;
    let mut in_title_tag   = false;
    let mut in_pre         = false;
    let mut in_code        = false;
    let mut in_blockquote  = false;
    let mut in_heading: Option<u8> = None;

    // Link tracking
    let mut in_a         = false;
    let mut current_href = String::new();
    let mut a_text       = String::new();

    // List tracking
    let mut list_stack: Vec<bool>   = Vec::new(); // true = ordered
    let mut list_counters: Vec<usize> = Vec::new();

    let mut tag_buf    = String::new();
    let mut text_buf   = String::new();
    let mut entity_buf = String::new();
    let mut in_entity  = false;

    macro_rules! flush_text {
        () => {{
            let t = text_buf.trim().to_string();
            text_buf.clear();
            if !t.is_empty() {
                let cl = if in_pre || in_code {
                    ContentLine::code(t)
                } else if in_blockquote {
                    ContentLine::quote(t)
                } else {
                    match in_heading {
                        Some(1) => ContentLine::heading(1, t),
                        Some(2) => ContentLine::heading(2, t),
                        Some(_) => ContentLine::heading(3, t),
                        None    => ContentLine::normal(t),
                    }
                };
                content.push(cl);
            }
        }};
    }

    macro_rules! push_empty {
        () => {{
            if !content.last().map_or(false, |l| l.kind == LineKind::Empty) {
                content.push(ContentLine::empty());
            }
        }};
    }

    let chars: Vec<char> = html.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let ch = chars[i];

        if in_tag {
            if ch == '>' {
                in_tag = false;
                let tag_raw  = tag_buf.trim().to_string();
                tag_buf.clear();
                let tag_lower = tag_raw.to_lowercase();
                let tag_name  = tag_lower.split_whitespace().next().unwrap_or("")
                    .trim_start_matches('/').to_string();
                let is_closing = tag_raw.trim_start().starts_with('/');

                match tag_name.as_str() {
                    "script"  => { in_script = !is_closing; }
                    "style"   => { in_style  = !is_closing; }
                    "title"   => { in_title_tag = !is_closing; }
                    _ => {}
                }

                if !in_script && !in_style {
                    match tag_name.as_str() {
                        "br" | "br/" => {
                            flush_text!();
                            push_empty!();
                        }
                        "hr" | "hr/" => {
                            flush_text!();
                            push_empty!();
                            content.push(ContentLine::separator());
                            push_empty!();
                        }
                        "p" | "/p" | "div" | "/div"
                        | "article" | "/article" | "section" | "/section"
                        | "main"    | "/main"    | "header"  | "/header"
                        | "footer"  | "/footer"  | "nav"     | "/nav"
                        | "aside"   | "/aside" => {
                            flush_text!();
                            push_empty!();
                        }
                        "h1" => { flush_text!(); push_empty!(); in_heading = Some(1); }
                        "h2" => { flush_text!(); push_empty!(); in_heading = Some(2); }
                        "h3" | "h4" | "h5" | "h6" => { flush_text!(); push_empty!(); in_heading = Some(3); }
                        "/h1" | "/h2" | "/h3" | "/h4" | "/h5" | "/h6" => {
                            flush_text!();
                            in_heading = None;
                            push_empty!();
                        }
                        "pre" if !is_closing => { flush_text!(); in_pre = true; }
                        "/pre"  => { flush_text!(); in_pre = false; push_empty!(); }
                        "code" if !is_closing && !in_pre => { in_code = true; }
                        "/code" if !in_pre => { in_code = false; }
                        "blockquote" if !is_closing => { flush_text!(); in_blockquote = true; push_empty!(); }
                        "/blockquote" => { flush_text!(); in_blockquote = false; push_empty!(); }

                        "ul" if !is_closing => { list_stack.push(false); list_counters.push(0); }
                        "/ul" => { flush_text!(); list_stack.pop(); list_counters.pop(); push_empty!(); }
                        "ol" if !is_closing => { list_stack.push(true); list_counters.push(0); }
                        "/ol" => { flush_text!(); list_stack.pop(); list_counters.pop(); push_empty!(); }
                        "li" if !is_closing => {
                            flush_text!();
                            if let Some(cnt) = list_counters.last_mut() {
                                if *list_stack.last().unwrap_or(&false) { *cnt += 1; }
                            }
                        }
                        "/li" => {
                            let t = text_buf.trim().to_string();
                            text_buf.clear();
                            if !t.is_empty() {
                                let is_ord = list_stack.last().copied().unwrap_or(false);
                                if is_ord {
                                    let n = list_counters.last().copied().unwrap_or(1);
                                    content.push(ContentLine::numbered(n, t));
                                } else {
                                    content.push(ContentLine::bullet(t));
                                }
                            }
                        }

                        "a" if !is_closing => {
                            current_href = extract_attr(&tag_raw, "href").unwrap_or_default();
                            in_a = true;
                            a_text.clear();
                        }
                        "/a" => {
                            if in_a && !current_href.is_empty() && !a_text.trim().is_empty() {
                                let href = normalize_link_href(&current_href);
                                if !href.is_empty() {
                                    links.push((a_text.trim().to_string(), href));
                                }
                            }
                            in_a = false;
                            current_href.clear();
                            a_text.clear();
                        }

                        "img" | "img/" => {
                            if let Some(alt) = extract_attr(&tag_raw, "alt") {
                                let alt = alt.trim().to_string();
                                if !alt.is_empty() {
                                    flush_text!();
                                    content.push(ContentLine::normal(format!("[🖼  {}]", alt)));
                                }
                            }
                        }

                        "table" | "/table" => { flush_text!(); push_empty!(); }
                        "tr" | "/tr" => { flush_text!(); push_empty!(); }
                        "td" | "th"  => { text_buf.push_str("  │  "); }
                        _ => {}
                    }
                }
            } else {
                tag_buf.push(ch);
            }
        } else if ch == '<' {
            in_tag = true;
            tag_buf.clear();
        } else if in_script || in_style {
            // discard
        } else if ch == '&' {
            in_entity = true;
            entity_buf.clear();
        } else if in_entity {
            if ch == ';' {
                in_entity = false;
                let dec = decode_entity(&entity_buf);
                entity_buf.clear();
                if in_title_tag { title.push_str(&dec); }
                else {
                    if in_a { a_text.push_str(&dec); }
                    if in_pre {
                        // keep as-is inside pre
                        text_buf.push_str(&dec);
                    } else {
                        text_buf.push_str(&dec);
                    }
                }
            } else {
                entity_buf.push(ch);
                if entity_buf.len() > 14 {
                    in_entity = false;
                    let fallback = format!("&{}", entity_buf);
                    entity_buf.clear();
                    if !in_title_tag { text_buf.push_str(&fallback); }
                }
            }
        } else if in_title_tag {
            title.push(ch);
        } else if in_pre {
            if ch == '\n' {
                let line = text_buf.clone();
                text_buf.clear();
                content.push(ContentLine::code(line));
            } else if ch != '\r' {
                if in_a { a_text.push(ch); }
                text_buf.push(ch);
            }
        } else {
            if ch == '\n' || ch == '\r' {
                if !text_buf.ends_with(' ') && !text_buf.is_empty() {
                    text_buf.push(' ');
                }
            } else {
                if in_a { a_text.push(ch); }
                text_buf.push(ch);
            }
        }
        i += 1;
    }

    // Final flush
    flush_text!();

    // Deduplicate consecutive empty lines and trim ends
    let mut clean: Vec<ContentLine> = Vec::new();
    let mut prev_empty = true;
    for line in content {
        if line.kind == LineKind::Empty {
            if !prev_empty { clean.push(line); prev_empty = true; }
        } else {
            clean.push(line);
            prev_empty = false;
        }
    }
    while clean.last().map_or(false, |l| l.kind == LineKind::Empty) { clean.pop(); }
    while clean.first().map_or(false, |l| l.kind == LineKind::Empty) { clean.remove(0); }

    // Limit links to 200 unique URLs
    links.dedup_by(|a, b| a.1 == b.1);
    links.truncate(200);

    (title.trim().to_string(), clean, links)
}

// ── Utility functions ─────────────────────────────────────────────────────────

fn resolve_url(raw: &str) -> String {
    let t = raw.trim();
    if t.starts_with("http://") || t.starts_with("https://") {
        t.to_string()
    } else if t.contains('.') && !t.contains(' ') && !t.contains('/') || t.starts_with("www.") {
        format!("https://{}", t)
    } else {
        format!("https://html.duckduckgo.com/html/?q={}", url_encode(t))
    }
}

fn normalize_link_href(href: &str) -> String {
    let h = href.trim();
    if h.starts_with("http://") || h.starts_with("https://") {
        h.to_string()
    } else if h.starts_with("//") {
        format!("https:{}", h)
    } else {
        String::new()
    }
}

fn domain_from_url(url: &str) -> String {
    let s = url.trim_start_matches("https://").trim_start_matches("http://");
    s.split('/').next().unwrap_or("").to_string()
}

fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let tag_lower = tag.to_lowercase();
    let search = format!("{}=", attr);
    let pos = tag_lower.find(&search)?;
    let rest = &tag[pos + search.len()..].trim_start();
    if rest.starts_with('"') {
        let end = rest[1..].find('"')?;
        Some(rest[1..1 + end].to_string())
    } else if rest.starts_with('\'') {
        let end = rest[1..].find('\'')?;
        Some(rest[1..1 + end].to_string())
    } else {
        let end = rest.find(|c: char| c.is_whitespace() || c == '>').unwrap_or(rest.len());
        Some(rest[..end].to_string())
    }
}

fn decode_entity(entity: &str) -> String {
    match entity {
        "amp" | "AMP"   => "&",
        "lt"  | "LT"    => "<",
        "gt"  | "GT"    => ">",
        "quot"| "QUOT"  => "\"",
        "apos"          => "'",
        "nbsp"          => " ",
        "ndash"         => "–",
        "mdash"         => "—",
        "laquo"         => "«",
        "raquo"         => "»",
        "hellip"        => "…",
        "trade"         => "™",
        "reg"           => "®",
        "copy"          => "©",
        "euro"          => "€",
        "pound"         => "£",
        "bull"          => "•",
        "lsquo"         => "\u{2018}",
        "rsquo"         => "\u{2019}",
        "ldquo"         => "\u{201C}",
        "rdquo"         => "\u{201D}",
        "middot"        => "·",
        "times"         => "×",
        "divide"        => "÷",
        "minus"         => "−",
        "prime"         => "′",
        "Prime"         => "″",
        "infin"         => "∞",
        "ne"            => "≠",
        "le"            => "≤",
        "ge"            => "≥",
        "larr"          => "←",
        "rarr"          => "→",
        "uarr"          => "↑",
        "darr"          => "↓",
        "check"         => "✓",
        "cross"         => "✗",
        "star"          => "★",
        "hearts"        => "♥",
        "spades"        => "♠",
        "clubs"         => "♣",
        "diams"         => "♦",
        _ => {
            // Numeric character reference
            if entity.starts_with('#') {
                let num = &entity[1..];
                let code: Option<u32> = if num.to_lowercase().starts_with('x') {
                    u32::from_str_radix(&num[1..], 16).ok()
                } else {
                    num.parse().ok()
                };
                if let Some(code) = code {
                    if let Some(ch) = char::from_u32(code) {
                        return ch.to_string();
                    }
                }
            }
            return format!("&{};", entity);
        }
    }.to_string()
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
