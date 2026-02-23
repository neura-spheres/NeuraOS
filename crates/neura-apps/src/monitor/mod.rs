use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui::style::{Style, Modifier};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::Value;
use chrono::{Utc, Datelike, Timelike};
use neura_app_framework::app_trait::App;

// ── Shared tracker types (imported by main.rs) ────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum AppLifecycle {
    Active,     // currently in the foreground
    Background, // was opened at least once, now minimised
    Idle,       // never opened (or was killed / restarted)
}

impl AppLifecycle {
    fn label(&self) -> &'static str {
        match self {
            Self::Active     => "Active",
            Self::Background => "Background",
            Self::Idle       => "Idle",
        }
    }
    fn color(&self) -> Color {
        match self {
            Self::Active     => GREEN,
            Self::Background => ORANGE,
            Self::Idle       => DIM,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppEntry {
    pub id:                String,
    pub name:              String,
    pub lifecycle:         AppLifecycle,
    pub total_opens:       u32,
    pub active_since:      Option<std::time::Instant>,
    pub total_active_secs: u64,
}

impl AppEntry {
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            id:                id.to_string(),
            name:              name.to_string(),
            lifecycle:         AppLifecycle::Idle,
            total_opens:       0,
            active_since:      None,
            total_active_secs: 0,
        }
    }

    /// Call when an app transitions to the foreground.
    pub fn set_active(&mut self) {
        if self.lifecycle != AppLifecycle::Active {
            self.lifecycle   = AppLifecycle::Active;
            self.active_since = Some(std::time::Instant::now());
            self.total_opens += 1;
        }
    }

    /// Call when an app is minimised back to the shell (still "running").
    pub fn set_background(&mut self) {
        if self.lifecycle == AppLifecycle::Active {
            self.total_active_secs += self.active_since
                .take()
                .map(|t| t.elapsed().as_secs())
                .unwrap_or(0);
            self.lifecycle = AppLifecycle::Background;
        }
    }

    /// Call when an app is killed or restarted (fully stopped).
    pub fn set_idle(&mut self) {
        if self.lifecycle == AppLifecycle::Active {
            self.total_active_secs += self.active_since
                .take()
                .map(|t| t.elapsed().as_secs())
                .unwrap_or(0);
        }
        self.lifecycle    = AppLifecycle::Idle;
        self.active_since = None;
    }

    /// Total seconds the app has been in the foreground (including current session).
    pub fn active_secs(&self) -> u64 {
        self.total_active_secs
            + self.active_since.as_ref().map(|t| t.elapsed().as_secs()).unwrap_or(0)
    }
}

/// Shared between TaskManagerApp and main.rs — both hold a clone of this Arc.
pub type OpenAppsTracker = Arc<Mutex<HashMap<String, AppEntry>>>;

// ── Colour palette ────────────────────────────────────────────────────────────
use neura_app_framework::palette::*;

// ── Sort mode ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum SortBy { Status, Name, Opens, Time }

impl SortBy {
    fn label(&self) -> &'static str {
        match self {
            Self::Status => "Status",
            Self::Name   => "Name",
            Self::Opens  => "Opens",
            Self::Time   => "Time",
        }
    }
    fn next(&self) -> Self {
        match self {
            Self::Status => Self::Name,
            Self::Name   => Self::Opens,
            Self::Opens  => Self::Time,
            Self::Time   => Self::Status,
        }
    }
}

// ── TaskManagerApp ────────────────────────────────────────────────────────────

pub struct TaskManagerApp {
    tracker:    OpenAppsTracker,
    started_at: chrono::DateTime<Utc>,
    selected:   usize,
    scroll:     usize,
    sort_by:    SortBy,

    /// Set by handle_key; drained by main.rs each frame.
    pub kill_request:    Option<String>,
    pub focus_request:   Option<String>,
    pub restart_request: Option<String>,

    /// Pending confirmation for kill (app_id). Cleared when user confirms or cancels.
    confirm_kill: Option<String>,
    status_msg:   String,
}

impl TaskManagerApp {
    pub fn new(tracker: OpenAppsTracker) -> Self {
        Self {
            tracker,
            started_at:      Utc::now(),
            selected:        0,
            scroll:          0,
            sort_by:         SortBy::Status,
            kill_request:    None,
            focus_request:   None,
            restart_request: None,
            confirm_kill:    None,
            status_msg:      String::new(),
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn sorted_entries(&self) -> Vec<AppEntry> {
        let lock = self.tracker.lock().unwrap_or_else(|p| p.into_inner());
        let mut v: Vec<AppEntry> = lock.values().cloned().collect();
        match self.sort_by {
            SortBy::Name => v.sort_by(|a, b| a.name.cmp(&b.name)),
            SortBy::Opens => v.sort_by(|a, b| {
                b.total_opens.cmp(&a.total_opens).then(a.name.cmp(&b.name))
            }),
            SortBy::Time => v.sort_by(|a, b| {
                b.active_secs().cmp(&a.active_secs()).then(a.name.cmp(&b.name))
            }),
            SortBy::Status => v.sort_by(|a, b| {
                let rank = |s: &AppLifecycle| match s {
                    AppLifecycle::Active     => 0,
                    AppLifecycle::Background => 1,
                    AppLifecycle::Idle       => 2,
                };
                rank(&a.lifecycle).cmp(&rank(&b.lifecycle)).then(a.name.cmp(&b.name))
            }),
        }
        v
    }

    fn selected_id(&self) -> Option<String> {
        self.sorted_entries().get(self.selected).map(|e| e.id.clone())
    }

    fn uptime_string(&self) -> String {
        let secs = Utc::now().signed_duration_since(self.started_at).num_seconds().max(0);
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        let s = secs % 60;
        if h > 0 { format!("{}h {:02}m {:02}s", h, m, s) }
        else if m > 0 { format!("{}m {:02}s", m, s) }
        else { format!("{}s", s) }
    }

    fn fmt_secs(secs: u64) -> String {
        if secs == 0 { return "—".to_string(); }
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        let s = secs % 60;
        if h > 0 { format!("{}:{:02}:{:02}", h, m, s) }
        else      { format!("{}:{:02}", m, s) }
    }
}

// ── App trait ─────────────────────────────────────────────────────────────────

impl App for TaskManagerApp {
    fn id(&self)   -> &str { "monitor" }
    fn name(&self) -> &str { "Task Manager" }

    fn init(&mut self) -> anyhow::Result<()> {
        self.started_at = Utc::now();
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        // ── Confirm-kill overlay eats all keys ───────────────────────────────
        if self.confirm_kill.is_some() {
            match key.code {
                KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                    if let Some(id) = self.confirm_kill.take() {
                        self.kill_request = Some(id.clone());
                        self.status_msg = format!("Killed '{}'", id);
                    }
                }
                _ => {
                    self.confirm_kill = None;
                    self.status_msg = "Kill cancelled.".to_string();
                }
            }
            return true;
        }

        // ── Ctrl shortcuts ────────────────────────────────────────────────────
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            // no ctrl shortcuts yet — fall through to normal keys
        }

        match key.code {
            // Exit app
            KeyCode::Esc | KeyCode::Char('q') => return false,

            // Navigation
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected > 0 { self.selected -= 1; }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let n = self.sorted_entries().len();
                if self.selected + 1 < n { self.selected += 1; }
            }
            KeyCode::Home => { self.selected = 0; self.scroll = 0; }
            KeyCode::End  => {
                let n = self.sorted_entries().len();
                if n > 0 { self.selected = n - 1; }
            }

            // Focus / switch to selected app
            KeyCode::Enter | KeyCode::Char('f') => {
                if let Some(id) = self.selected_id() {
                    if id != "monitor" {
                        self.focus_request = Some(id.clone());
                        self.status_msg = format!("Switching to '{}'…", id);
                    } else {
                        self.status_msg = "Already in Task Manager.".to_string();
                    }
                }
            }

            // Kill selected app
            KeyCode::Char('x') | KeyCode::Delete => {
                if let Some(id) = self.selected_id() {
                    if id == "monitor" {
                        self.status_msg = "Cannot kill the Task Manager itself.".to_string();
                    } else {
                        self.confirm_kill = Some(id);
                    }
                }
            }

            // Restart selected app
            KeyCode::Char('r') => {
                if let Some(id) = self.selected_id() {
                    if id == "monitor" {
                        self.status_msg = "Cannot restart the Task Manager.".to_string();
                    } else {
                        self.restart_request = Some(id.clone());
                        self.status_msg = format!("Restarted '{}'.", id);
                    }
                }
            }

            // Cycle sort
            KeyCode::Char('s') => {
                self.sort_by = self.sort_by.next();
                self.selected = 0;
                self.scroll   = 0;
                self.status_msg = format!("Sort: {}", self.sort_by.label());
            }

            // Clear status
            KeyCode::Char(' ') => { self.status_msg.clear(); }

            _ => {}
        }
        true
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::vertical([
            Constraint::Length(7),
            Constraint::Min(6),
            Constraint::Length(1),
        ]).split(area);

        self.render_header(frame, chunks[0]);
        self.render_table(frame, chunks[1]);
        self.render_statusbar(frame, chunks[2]);

        if let Some(ref id) = self.confirm_kill {
            self.render_confirm_overlay(frame, area, id);
        }
    }

    fn on_resume(&mut self) { self.status_msg.clear(); }
    fn on_pause(&mut self)  {}
    fn on_close(&mut self)  {}
    fn save_state(&self) -> Option<Value> { None }
    fn load_state(&mut self, _: Value) {}

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

// ── Render helpers ────────────────────────────────────────────────────────────

impl TaskManagerApp {
    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let now = Utc::now();
        let time_str = format!("{:04}-{:02}-{:02}  {:02}:{:02}:{:02} UTC",
            now.year(), now.month(), now.day(),
            now.hour(), now.minute(), now.second());

        let entries      = self.sorted_entries();
        let total        = entries.len();
        let active_cnt   = entries.iter().filter(|e| e.lifecycle == AppLifecycle::Active).count();
        let bg_cnt       = entries.iter().filter(|e| e.lifecycle == AppLifecycle::Background).count();
        let idle_cnt     = total - active_cnt - bg_cnt;

        let lines = vec![
            Line::from(vec![
                Span::styled("  Platform ", Style::default().fg(MUTED)),
                Span::styled(std::env::consts::OS,   Style::default().fg(TEXT)),
                Span::styled("   Arch ", Style::default().fg(MUTED)),
                Span::styled(std::env::consts::ARCH, Style::default().fg(TEXT)),
                Span::styled("   PID ", Style::default().fg(MUTED)),
                Span::styled(format!("{}", std::process::id()), Style::default().fg(ORANGE)),
            ]),
            Line::from(vec![
                Span::styled("  Session Uptime ", Style::default().fg(MUTED)),
                Span::styled(self.uptime_string(), Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
                Span::styled("   Time ", Style::default().fg(MUTED)),
                Span::styled(time_str, Style::default().fg(TEXT)),
            ]),
            Line::from(vec![
                Span::styled("  Apps ", Style::default().fg(MUTED)),
                Span::styled(format!("{}", total), Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
                Span::styled("   Active ", Style::default().fg(MUTED)),
                Span::styled(format!("{}", active_cnt), Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
                Span::styled("   Background ", Style::default().fg(MUTED)),
                Span::styled(format!("{}", bg_cnt), Style::default().fg(ORANGE)),
                Span::styled("   Idle ", Style::default().fg(MUTED)),
                Span::styled(format!("{}", idle_cnt), Style::default().fg(DIM)),
            ]),
            Line::from(vec![
                Span::styled("  Sort ", Style::default().fg(MUTED)),
                Span::styled(format!("[{}]", self.sort_by.label()), Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
                Span::styled("  press [s] to cycle", Style::default().fg(DIM)),
            ]),
            Line::from(""),
        ];

        let block = Block::default().borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .title(" Task Manager — NeuraOS ")
            .title_style(Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn render_table(&self, frame: &mut Frame, area: Rect) {
        let entries = self.sorted_entries();
        let block = Block::default().borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .title(format!(" Processes ({}) ", entries.len()))
            .title_style(Style::default().fg(ORANGE).add_modifier(Modifier::BOLD));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height < 2 { return; }
        let vis = inner.height as usize - 1; // subtract header row

        // Column widths
        let w      = inner.width as usize;
        let id_w   = 14usize;
        let st_w   = 12usize;
        let op_w   = 6usize;
        let tm_w   = 9usize;
        let nm_w   = w.saturating_sub(id_w + st_w + op_w + tm_w + 4);

        // Header row
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("  {:<w$}", "ID",     w=id_w), Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
                Span::styled(format!("{:<w$}", "Name",    w=nm_w), Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
                Span::styled(format!("{:<w$}", "Status",  w=st_w), Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
                Span::styled(format!("{:>w$}", "Opens",   w=op_w), Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
                Span::styled(format!("{:>w$}", "Active",  w=tm_w), Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
            ])).style(Style::default().bg(Color::Rgb(30, 30, 50))),
            Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 },
        );

        let list_y = inner.y + 1;
        let list_h = inner.height.saturating_sub(1);

        // Clamp scroll
        let scroll = {
            let max = entries.len().saturating_sub(vis);
            let mut s = self.scroll;
            if self.selected >= s + vis { s = self.selected.saturating_sub(vis - 1); }
            s.min(max)
        };

        for (row_i, (abs_i, entry)) in entries.iter()
            .enumerate()
            .skip(scroll)
            .take(vis)
            .enumerate()
        {
            let y = list_y + row_i as u16;
            if y >= list_y + list_h { break; }

            let is_sel = abs_i == self.selected;
            let bg      = if is_sel { SEL_BG } else { Color::Reset };
            let prefix  = if is_sel { "▸ " } else { "  " };

            let base_sty = Style::default().fg(if is_sel { TEXT } else { MUTED }).bg(bg);
            let st_sty   = Style::default()
                .fg(entry.lifecycle.color()).bg(bg)
                .add_modifier(if entry.lifecycle == AppLifecycle::Active { Modifier::BOLD } else { Modifier::empty() });

            let id_disp = truncate(&entry.id,   id_w);
            let nm_disp = truncate(&entry.name, nm_w);

            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(format!("{}{:<w$}", prefix, id_disp, w=id_w), base_sty),
                    Span::styled(format!("{:<w$}", nm_disp, w=nm_w),  base_sty),
                    Span::styled(format!("{:<w$}", entry.lifecycle.label(), w=st_w), st_sty),
                    Span::styled(
                        format!("{:>w$}", entry.total_opens, w=op_w),
                        Style::default().fg(if entry.total_opens > 0 { CYAN } else { DIM }).bg(bg),
                    ),
                    Span::styled(
                        format!("{:>w$}", Self::fmt_secs(entry.active_secs()), w=tm_w),
                        Style::default().fg(DIM).bg(bg),
                    ),
                ])),
                Rect { x: inner.x, y, width: inner.width, height: 1 },
            );
        }

        // Scrollbar indicator if list is taller than view
        if entries.len() > vis && inner.width > 2 {
            let bar_h  = list_h as usize;
            let thumb_h = ((bar_h * vis) / entries.len()).max(1).min(bar_h);
            let thumb_y = if entries.len() > vis {
                (scroll * (bar_h - thumb_h)) / (entries.len() - vis)
            } else { 0 };
            for row in 0..bar_h {
                let bar_ch = if row >= thumb_y && row < thumb_y + thumb_h { "█" } else { "░" };
                let bar_sty = Style::default().fg(if row >= thumb_y && row < thumb_y + thumb_h { PRIMARY } else { BORDER });
                frame.render_widget(
                    Paragraph::new(bar_ch).style(bar_sty),
                    Rect { x: inner.x + inner.width - 1, y: list_y + row as u16, width: 1, height: 1 },
                );
            }
        }
    }

    fn render_statusbar(&self, frame: &mut Frame, area: Rect) {
        let left = if self.status_msg.is_empty() {
            Line::from(vec![
                Span::styled("  [j/k] nav  ", Style::default().fg(DIM)),
                Span::styled("[Enter/f] focus  ", Style::default().fg(DIM)),
                Span::styled("[x/Del] kill  ", Style::default().fg(RED)),
                Span::styled("[r] restart  ", Style::default().fg(ORANGE)),
                Span::styled("[s] sort  ", Style::default().fg(DIM)),
                Span::styled("[q] exit", Style::default().fg(DIM)),
            ])
        } else {
            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(self.status_msg.clone(), Style::default().fg(CYAN)),
            ])
        };
        frame.render_widget(Paragraph::new(left), area);
    }

    fn render_confirm_overlay(&self, frame: &mut Frame, area: Rect, id: &str) {
        let dw: u16 = 52;
        let dh: u16 = 6;
        let x = area.x + area.width.saturating_sub(dw) / 2;
        let y = area.y + area.height.saturating_sub(dh) / 2;
        let dialog = Rect {
            x, y,
            width:  dw.min(area.width),
            height: dh.min(area.height),
        };
        frame.render_widget(Clear, dialog);
        let block = Block::default().borders(Borders::ALL)
            .border_style(Style::default().fg(RED))
            .title(" ⚠  Kill App ")
            .title_style(Style::default().fg(RED).add_modifier(Modifier::BOLD));
        let inner = block.inner(dialog);
        frame.render_widget(block, dialog);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("  Kill ", Style::default().fg(MUTED)),
                    Span::styled(format!("\"{}\"", id), Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
                    Span::styled("?  This will stop the app.", Style::default().fg(MUTED)),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  [Enter / y]", Style::default().fg(RED).add_modifier(Modifier::BOLD)),
                    Span::styled(" Confirm    ", Style::default().fg(MUTED)),
                    Span::styled("[Any other key]", Style::default().fg(DIM)),
                    Span::styled(" Cancel", Style::default().fg(MUTED)),
                ]),
            ]),
            inner,
        );
    }
}

// ── Utility ───────────────────────────────────────────────────────────────────

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() }
    else { format!("{}…", s.chars().take(max.saturating_sub(1)).collect::<String>()) }
}
