use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui::style::{Style, Modifier};
use ratatui::layout::{Layout, Constraint, Direction, Rect};
use chrono::{Utc, FixedOffset, Timelike};

use neura_app_framework::app_trait::App;
use neura_app_framework::consts::{OS_TAGLINE, OS_VERSION, OS_NAME};
use crate::statusbar::StatusBar;
use crate::theme::Theme;

// ── Mode enums ───────────────────────────────────────────────────────────────

/// Represents which pane/mode the desktop is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopMode {
    /// OS home screen — app grid, widgets, pinned apps, integrated console.
    HomeScreen,
    /// Full shell — complete command history + prompt (toggled with F12).
    Shell,
    /// An application is open and filling the main area.
    AppView(String),
    /// Command palette overlay (over whichever base mode is active).
    CommandPalette,
}

/// Which element has keyboard focus on the home screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HomeFocus {
    Console,
    AppGrid,
}

/// Which section of the app grid is navigated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HomeSection {
    Pinned,
    AllApps,
}

// ── Helper functions ─────────────────────────────────────────────────────────

/// Returns a single unicode icon character for an app ID.
pub fn app_icon(id: &str) -> &'static str {
    match id {
        "notes"    => "✎",
        "tasks"    => "✓",
        "files"    => "◧",
        "settings" => "⚙",
        "chat"     => "◎",
        "calc"     => "∑",
        "clock"    => "◷",
        "monitor"  => "▣",
        "terminal" => "⊡",
        "dev"      => "◈",
        "weather"  => "◌",
        "browser"  => "◰",
        "media"    => "♫",
        "calendar" => "▦",
        "contacts" => "◑",
        "logs"     => "≡",
        "sysinfo"  => "◬",
        "backup"   => "↑",
        "mail"     => "✉",
        "ssh"      => "⇒",
        "ftp"      => "⇅",
        "db"       => "◪",
        "sync"     => "↻",
        "store"    => "■",
        _          => "□",
    }
}

/// Returns a short display name for an app ID.
pub fn app_display_name(id: &str) -> &'static str {
    match id {
        "notes"    => "Notes",
        "tasks"    => "Tasks",
        "files"    => "Files",
        "settings" => "Settings",
        "chat"     => "Chat",
        "calc"     => "Calc",
        "clock"    => "Clock",
        "monitor"  => "Monitor",
        "terminal" => "Terminal",
        "dev"      => "Dev",
        "weather"  => "Weather",
        "browser"  => "Browser",
        "media"    => "Media",
        "calendar" => "Calendar",
        "contacts" => "Contacts",
        "logs"     => "Logs",
        "sysinfo"  => "Sys Info",
        "backup"   => "Backup",
        "mail"     => "Mail",
        "ssh"      => "SSH",
        "ftp"      => "FTP",
        "db"       => "Database",
        "sync"     => "Sync",
        "store"    => "Store",
        _          => "App",
    }
}

fn time_of_day_greeting(hour: u32) -> &'static str {
    match hour {
        5..=11  => "morning",
        12..=17 => "afternoon",
        18..=21 => "evening",
        _       => "night",
    }
}

// ── Desktop struct ────────────────────────────────────────────────────────────

/// Top-level desktop state.
pub struct Desktop {
    pub mode: DesktopMode,
    pub statusbar: StatusBar,
    pub theme: Theme,
    pub shell_history: Vec<String>,
    pub shell_input: String,
    pub shell_cursor: usize,
    pub shell_scroll: usize,
    pub notifications: Vec<String>,
    pub show_help: bool,
    pub shell_prompt: String,
    pub show_greeting: bool,
    // Clock settings
    pub clock_24h: bool,
    pub clock_show_seconds: bool,
    // Timezone
    pub timezone_offset_mins: i32,
    pub timezone_label: String,
    // UI display settings
    pub show_clock: bool,
    pub border_type: BorderType,
    pub transparent_bg: bool,
    // Command palette state
    pub palette_input: String,
    pub palette_selected: usize,
    /// True when the palette was opened from HomeScreen (so Esc returns there).
    pub palette_from_home: bool,
    // Autocomplete suggestions
    pub suggestions: Vec<String>,
    pub suggestion_selected: usize,
    // Navigation state
    /// True when HomeScreen is the "base" — apps opened return to HomeScreen on close.
    pub home_is_base: bool,
    pub home_focus: HomeFocus,
    pub home_section: HomeSection,
    pub home_app_idx: usize,
    /// App IDs the user has pinned to the home screen dock.
    pub pinned_apps: Vec<String>,
    /// (id, display_name) pairs for every registered app, sorted alphabetically.
    pub all_apps_list: Vec<(String, String)>,
}

impl Desktop {
    pub fn new(hostname: &str, username: &str) -> Self {
        let default_prompt = format!("{}@{} ~ >", username, hostname);
        Self {
            mode: DesktopMode::HomeScreen,
            statusbar: StatusBar::new(hostname, username),
            theme: Theme::default(),
            shell_history: vec![
                format!("  \u{2588}\u{2588}\u{2588}\u{2557}   \u{2588}\u{2588}\u{2557}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557}\u{2588}\u{2588}\u{2557}   \u{2588}\u{2588}\u{2557}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557} \u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557}"),
                format!("  \u{2588}\u{2588}\u{2588}\u{2588}\u{2557}  \u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d}\u{2588}\u{2588}\u{2551}   \u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2588}\u{2588}\u{2557}\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2588}\u{2588}\u{2557}\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2550}\u{2588}\u{2588}\u{2557}\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d}"),
                format!("  \u{2588}\u{2588}\u{2554}\u{2588}\u{2588}\u{2557} \u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557}  \u{2588}\u{2588}\u{2551}   \u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2554}\u{255d}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2551}   \u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557}"),
                format!("  \u{2588}\u{2588}\u{2551}\u{255a}\u{2588}\u{2588}\u{2557}\u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{255d}  \u{2588}\u{2588}\u{2551}   \u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2588}\u{2588}\u{2557}\u{2588}\u{2588}\u{2554}\u{2550}\u{2550}\u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2551}   \u{2588}\u{2588}\u{2551}\u{255a}\u{2550}\u{2550}\u{2550}\u{2550}\u{2588}\u{2588}\u{2551}"),
                format!("  \u{2588}\u{2588}\u{2551} \u{255a}\u{2588}\u{2588}\u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2557}\u{255a}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2554}\u{255d}\u{2588}\u{2588}\u{2551}  \u{2588}\u{2588}\u{2551}\u{2588}\u{2588}\u{2551}  \u{2588}\u{2588}\u{2551}\u{255a}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2554}\u{255d}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2551}"),
                format!("  \u{255a}\u{2550}\u{255d}  \u{255a}\u{2550}\u{2550}\u{2550}\u{255d}\u{255a}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d} \u{255a}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d} \u{255a}\u{2550}\u{255d}  \u{255a}\u{2550}\u{255d}\u{255a}\u{2550}\u{255d}  \u{255a}\u{2550}\u{255d} \u{255a}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d} \u{255a}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d}"),
                String::new(),
                format!("  {} v{}", OS_TAGLINE, OS_VERSION),
                format!("  Type 'help' for commands. F12: toggle home. Ctrl+P: palette."),
                String::new(),
            ],
            shell_input: String::new(),
            shell_cursor: 0,
            shell_scroll: 0,
            notifications: Vec::new(),
            show_help: false,
            shell_prompt: default_prompt,
            show_greeting: true,
            clock_24h: true,
            clock_show_seconds: true,
            timezone_offset_mins: 0,
            timezone_label: "UTC".to_string(),
            show_clock: true,
            border_type: BorderType::Rounded,
            transparent_bg: false,
            palette_input: String::new(),
            palette_selected: 0,
            palette_from_home: true,
            suggestions: Vec::new(),
            suggestion_selected: 0,
            home_is_base: true,
            home_focus: HomeFocus::Console,
            home_section: HomeSection::Pinned,
            home_app_idx: 0,
            pinned_apps: vec![
                "chat".to_string(),
                "notes".to_string(),
                "tasks".to_string(),
                "files".to_string(),
                "terminal".to_string(),
                "settings".to_string(),
                "dev".to_string(),
                "clock".to_string(),
            ],
            all_apps_list: Vec::new(),
        }
    }

    pub fn push_output(&mut self, line: &str) {
        self.shell_history.push(line.to_string());
        self.shell_scroll = self.shell_history.len();
    }

    /// Clamp shell_scroll to the actual max scroll position.
    pub fn clamp_shell_scroll(&mut self) {
        let (_, term_height) = crossterm::terminal::size().unwrap_or((80, 24));
        let visible = (term_height as usize).saturating_sub(6);
        let max_scroll = self.shell_history.len().saturating_sub(visible);
        if self.shell_scroll > max_scroll {
            self.shell_scroll = max_scroll;
        }
    }

    pub fn push_prompt(&mut self, prompt: &str, input: &str) {
        self.shell_prompt = prompt.to_string();
        self.shell_history.push(format!("{} {}", prompt, input));
        self.shell_scroll = self.shell_history.len();
    }

    pub fn palette_commands() -> Vec<(&'static str, &'static str)> {
        vec![
            ("open notes",    "Open NeuraNotes"),
            ("open tasks",    "Open NeuraTasks"),
            ("open files",    "Open NeuraFiles"),
            ("open chat",     "Open NeuraChat (AI)"),
            ("open settings", "Open NeuraSettings"),
            ("open calc",     "Open NeuraCalc"),
            ("open clock",    "Open NeuraClock"),
            ("open monitor",  "Open NeuraMonitor"),
            ("open calendar", "Open NeuraCalendar"),
            ("open contacts", "Open NeuraContacts"),
            ("open terminal", "Open NeuraTerminal"),
            ("open dev",      "Open NeuraDev"),
            ("open weather",  "Open NeuraWeather"),
            ("open browser",  "Open NeuraBrowser"),
            ("open media",    "Open NeuraMedia"),
            ("open mail",     "Open NeuraMail"),
            ("open logs",     "Open NeuraLogs"),
            ("open sysinfo",  "Open System Info"),
            ("apps",          "List available apps"),
            ("ai",            "Ask AI assistant"),
            ("sysinfo",       "System information"),
            ("neofetch",      "System info (fancy)"),
            ("help",          "Show help"),
            ("clear",         "Clear shell"),
            ("exit",          "Shutdown NeuraOS"),
        ]
    }

    pub fn filtered_palette_commands(&self) -> Vec<(&'static str, &'static str)> {
        let all = Self::palette_commands();
        if self.palette_input.is_empty() {
            return all;
        }
        let query = self.palette_input.to_lowercase();
        all.into_iter()
            .filter(|(cmd, desc)| {
                cmd.to_lowercase().contains(&query) || desc.to_lowercase().contains(&query)
            })
            .collect()
    }

    /// Returns the app id currently highlighted in the home grid, if any.
    pub fn home_selected_app_id(&self) -> Option<String> {
        match self.home_section {
            HomeSection::Pinned => {
                self.pinned_apps.get(self.home_app_idx).cloned()
            }
            HomeSection::AllApps => {
                self.all_apps_list.get(self.home_app_idx).map(|(id, _)| id.clone())
            }
        }
    }

    // ── Top-level render ──────────────────────────────────────────────────────

    /// Render the full desktop into a ratatui frame (non-app modes).
    pub fn render(&self, frame: &mut Frame) {
        let size = frame.area();

        let show_home_bg = matches!(self.mode, DesktopMode::HomeScreen)
            || (matches!(self.mode, DesktopMode::CommandPalette) && self.palette_from_home);

        if show_home_bg {
            self.render_home_screen(frame, size);
        } else {
            // Full shell layout
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Min(5),
                    Constraint::Length(3),
                ])
                .split(size);

            self.render_statusbar(frame, chunks[0]);
            self.render_shell(frame, chunks[1]);
            self.render_input(frame, chunks[2]);
            self.render_suggestions(frame, chunks[2]);
        }

        if matches!(self.mode, DesktopMode::CommandPalette) {
            self.render_command_palette(frame, size);
        }

        if self.show_help {
            self.render_help_overlay(frame, size);
        }
    }

    /// Render the desktop with an active application filling the main area.
    pub fn render_with_app(&self, frame: &mut Frame, app: Option<&dyn App>) {
        let size = frame.area();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(5),
            ])
            .split(size);

        self.render_statusbar(frame, chunks[0]);

        if let Some(app) = app {
            app.render(frame, chunks[1]);
        } else {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(self.theme.border))
                .title(" App Not Found ")
                .title_style(Style::default().fg(self.theme.error));
            let inner = block.inner(chunks[1]);
            frame.render_widget(block, chunks[1]);
            let text = Paragraph::new("  Application not available. Press Esc to return.")
                .style(Style::default().fg(self.theme.fg));
            frame.render_widget(text, inner);
        }

        if self.show_help {
            self.render_help_overlay(frame, size);
        }
    }

    // ── Status bar ────────────────────────────────────────────────────────────

    fn render_statusbar(&self, frame: &mut Frame, area: Rect) {
        let width = area.width as usize;

        // Mode-aware workspace indicator
        let workspace_label = match &self.mode {
            DesktopMode::HomeScreen    => "  Home".to_string(),
            DesktopMode::Shell         => format!("  Shell  {}", self.statusbar.workspace),
            DesktopMode::AppView(id)   => format!("  {}", app_display_name(id)),
            DesktopMode::CommandPalette if self.palette_from_home => "  Home".to_string(),
            DesktopMode::CommandPalette => "  Shell".to_string(),
        };
        let left = format!(" NeuraOS |{} ", workspace_label);

        let right = if self.show_clock {
            let offset = FixedOffset::east_opt(self.timezone_offset_mins * 60)
                .unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
            let now = Utc::now().with_timezone(&offset);
            let time = if self.clock_show_seconds {
                if self.clock_24h { now.format("%H:%M:%S").to_string() }
                else              { now.format("%I:%M:%S %p").to_string() }
            } else if self.clock_24h {
                now.format("%H:%M").to_string()
            } else {
                now.format("%I:%M %p").to_string()
            };
            format!(" {} @ {} | {} {} ", self.statusbar.username, self.statusbar.hostname, time, self.timezone_label)
        } else {
            format!(" {} @ {} ", self.statusbar.username, self.statusbar.hostname)
        };

        let combined = left.len() + right.len();
        let (left, right) = if combined > width {
            let right_len = right.len().min(width);
            let left_len = width.saturating_sub(right_len);
            (left[..left.len().min(left_len)].to_string(), right[..right_len].to_string())
        } else {
            (left, right)
        };

        let padding  = width.saturating_sub(left.len()).saturating_sub(right.len());
        let bar_text = format!("{}{:width$}{}", left, "", right, width = padding);

        let bar_style = if self.transparent_bg {
            Style::default().fg(self.theme.statusbar_fg)
        } else {
            Style::default().bg(self.theme.statusbar_bg).fg(self.theme.statusbar_fg)
        };
        frame.render_widget(Paragraph::new(bar_text).style(bar_style), area);
    }

    // ── Home screen ───────────────────────────────────────────────────────────

    fn render_home_screen(&self, frame: &mut Frame, size: Rect) {
        let top_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(size);

        self.render_statusbar(frame, top_chunks[0]);

        let body = top_chunks[1];
        let h = body.height;

        // Adaptive section heights
        let widgets_h: u16 = if h >= 32 { 6 } else { 5 };
        let pinned_h:  u16 = if h >= 28 { 6 } else { 5 };
        let console_h: u16 = 3;

        let body_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(widgets_h),
                Constraint::Length(pinned_h),
                Constraint::Min(3),
                Constraint::Length(console_h),
            ])
            .split(body);

        self.render_home_widgets(frame, body_chunks[0]);
        self.render_home_pinned(frame, body_chunks[1]);
        self.render_home_all_apps(frame, body_chunks[2]);
        self.render_home_console(frame, body_chunks[3]);
    }

    fn render_home_widgets(&self, frame: &mut Frame, area: Rect) {
        let halves = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
            .split(area);

        self.render_clock_widget(frame, halves[0]);
        self.render_sysinfo_widget(frame, halves[1]);
    }

    fn render_clock_widget(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(self.border_type)
            .border_style(Style::default().fg(self.theme.border))
            .title(Span::styled(
                " ◷ Clock & Date ",
                Style::default().fg(self.theme.accent).add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height == 0 { return; }

        let offset = FixedOffset::east_opt(self.timezone_offset_mins * 60)
            .unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
        let now = Utc::now().with_timezone(&offset);

        let time_str = if self.clock_show_seconds {
            if self.clock_24h { now.format("%H:%M:%S").to_string() }
            else              { now.format("%I:%M:%S %p").to_string() }
        } else if self.clock_24h {
            now.format("%H:%M").to_string()
        } else {
            now.format("%I:%M %p").to_string()
        };

        let date_str  = now.format("%A, %B %d %Y").to_string();
        let greet_str = format!(
            "  Good {}, {}",
            time_of_day_greeting(now.hour()),
            self.statusbar.username,
        );

        // Line 1: time + tz
        let time_line = Paragraph::new(
            format!("  {} {}", time_str, self.timezone_label)
        ).style(Style::default().fg(self.theme.accent).add_modifier(Modifier::BOLD));
        frame.render_widget(time_line, Rect::new(inner.x, inner.y, inner.width, 1));

        // Line 2: date (if room)
        if inner.height >= 2 {
            let date_line = Paragraph::new(format!("  {}", date_str))
                .style(Style::default().fg(self.theme.fg));
            frame.render_widget(date_line, Rect::new(inner.x, inner.y + 1, inner.width, 1));
        }

        // Line 3: greeting (if room)
        if inner.height >= 3 {
            let greet_line = Paragraph::new(greet_str)
                .style(Style::default().fg(self.theme.muted));
            frame.render_widget(greet_line, Rect::new(inner.x, inner.y + 2, inner.width, 1));
        }
    }

    fn render_sysinfo_widget(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(self.border_type)
            .border_style(Style::default().fg(self.theme.border))
            .title(Span::styled(
                " ⊡ System ",
                Style::default().fg(self.theme.accent).add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height == 0 { return; }

        let lines: Vec<Line> = vec![
            Line::from(vec![
                Span::styled("  OS   ", Style::default().fg(self.theme.muted)),
                Span::styled(
                    format!("{} v{}", OS_NAME, OS_VERSION),
                    Style::default().fg(self.theme.fg).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("  Host ", Style::default().fg(self.theme.muted)),
                Span::styled(
                    format!("{}@{}", self.statusbar.username, self.statusbar.hostname),
                    Style::default().fg(self.theme.success),
                ),
            ]),
            Line::from(vec![
                Span::styled("  Keys ", Style::default().fg(self.theme.muted)),
                Span::styled(
                    "F12 shell  Ctrl+P palette  Tab apps",
                    Style::default().fg(self.theme.muted),
                ),
            ]),
        ];

        let para = Paragraph::new(lines);
        frame.render_widget(para, inner);
    }

    fn render_home_pinned(&self, frame: &mut Frame, area: Rect) {
        let is_active = self.home_focus == HomeFocus::AppGrid
            && self.home_section == HomeSection::Pinned;
        let border_color = if is_active { self.theme.border_focused } else { self.theme.border };
        let title_color  = if is_active { self.theme.accent } else { self.theme.muted };
        let title_mod    = if is_active { Modifier::BOLD } else { Modifier::empty() };

        let hint = if is_active {
            " ←/→: move  Enter: open  p: unpin  Tab: all apps  Esc: console "
        } else {
            " Tab: navigate apps "
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(self.border_type)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(
                " ★ Pinned Apps ",
                Style::default().fg(title_color).add_modifier(title_mod),
            ))
            .title_bottom(Span::styled(hint, Style::default().fg(self.theme.border)));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.width == 0 || inner.height == 0 { return; }
        if self.pinned_apps.is_empty() {
            let empty = Paragraph::new("  No pinned apps. Navigate to All Apps and press p to pin.")
                .style(Style::default().fg(self.theme.muted));
            frame.render_widget(empty, inner);
            return;
        }

        // Each chip: " ICON Name_______ " ≈ 13 chars wide
        let chip_w = 13usize;
        let cols   = ((inner.width as usize + 1) / (chip_w + 1)).max(1);

        let mut y = inner.y;
        let mut col = 0usize;
        let mut row_spans: Vec<Span> = Vec::new();

        for (i, app_id) in self.pinned_apps.iter().enumerate() {
            let name  = app_display_name(app_id);
            // Truncate name to fit chip: icon(1) + space(1) + name(up to 8) = 10 inner, +3 padding
            let name_trunc = if name.len() > 8 { &name[..8] } else { name };
            let chip  = format!(" {} {:<8} ", app_icon(app_id), name_trunc);

            let is_sel = is_active && i == self.home_app_idx;
            let style  = if is_sel {
                Style::default()
                    .fg(self.theme.selected_fg)
                    .bg(self.theme.selected_bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.theme.accent)
            };

            row_spans.push(Span::styled(chip, style));
            row_spans.push(Span::raw(" "));
            col += 1;

            if col >= cols || i == self.pinned_apps.len() - 1 {
                if y < inner.y + inner.height {
                    let line = Line::from(row_spans.drain(..).collect::<Vec<_>>());
                    let para = Paragraph::new(line);
                    frame.render_widget(para, Rect::new(inner.x, y, inner.width, 1));
                    y += 1;
                }
                col = 0;
            }
        }
    }

    fn render_home_all_apps(&self, frame: &mut Frame, area: Rect) {
        let is_active = self.home_focus == HomeFocus::AppGrid
            && self.home_section == HomeSection::AllApps;
        let border_color = if is_active { self.theme.border_focused } else { self.theme.border };
        let title_color  = if is_active { self.theme.accent } else { self.theme.muted };
        let title_mod    = if is_active { Modifier::BOLD } else { Modifier::empty() };

        let hint = if is_active {
            " ←/→: move  Enter: open  p: pin  Tab: pinned  Esc: console "
        } else {
            " Shift+Tab: navigate all apps "
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(self.border_type)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(
                " ◈ All Apps ",
                Style::default().fg(title_color).add_modifier(title_mod),
            ))
            .title_bottom(Span::styled(hint, Style::default().fg(self.theme.border)));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.width == 0 || inner.height == 0 || self.all_apps_list.is_empty() { return; }

        // Compact chips: " ICON name " — width varies per app name
        let mut y       = inner.y;
        let mut x_used  = 0usize;
        let max_x       = inner.width as usize;
        let mut row_spans: Vec<Span> = Vec::new();

        for (i, (app_id, _)) in self.all_apps_list.iter().enumerate() {
            let name      = app_display_name(app_id);
            let chip_text = format!(" {} {} ", app_icon(app_id), name);
            let chip_len  = chip_text.chars().count();

            // Wrap to next row if needed
            if x_used + chip_len + 1 > max_x && x_used > 0 {
                if y < inner.y + inner.height {
                    let line = Line::from(row_spans.drain(..).collect::<Vec<_>>());
                    let para = Paragraph::new(line);
                    frame.render_widget(para, Rect::new(inner.x, y, inner.width, 1));
                    y += 1;
                }
                x_used = 0;
            }

            let is_sel = is_active && i == self.home_app_idx;
            let style  = if is_sel {
                Style::default()
                    .fg(self.theme.selected_fg)
                    .bg(self.theme.selected_bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.theme.muted)
            };

            row_spans.push(Span::styled(chip_text, style));
            row_spans.push(Span::raw(" "));
            x_used += chip_len + 1;
        }

        // Flush last row
        if !row_spans.is_empty() && y < inner.y + inner.height {
            let line = Line::from(row_spans);
            let para = Paragraph::new(line);
            frame.render_widget(para, Rect::new(inner.x, y, inner.width, 1));
        }
    }

    fn render_home_console(&self, frame: &mut Frame, area: Rect) {
        let is_focused   = self.home_focus == HomeFocus::Console;
        let border_color = if is_focused { self.theme.border_focused } else { self.theme.border };
        let title_color  = if is_focused { self.theme.accent } else { self.theme.muted };
        let title_mod    = if is_focused { Modifier::BOLD } else { Modifier::empty() };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(self.border_type)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(
                " ⊡ Console ",
                Style::default().fg(title_color).add_modifier(title_mod),
            ))
            .title_bottom(Span::styled(
                " F12: Full Shell  Ctrl+P: Palette  Tab: Apps ",
                Style::default().fg(self.theme.border),
            ));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height == 0 { return; }

        let prompt_text = format!("{} {}", self.shell_prompt, self.shell_input);
        let input_para  = Paragraph::new(prompt_text.as_str())
            .style(Style::default().fg(self.theme.prompt));
        frame.render_widget(input_para, inner);

        // Show cursor
        let cursor_x = inner.x + self.shell_prompt.len() as u16 + 1 + self.shell_cursor as u16;
        let cursor_y = inner.y;
        if cursor_x < inner.x + inner.width {
            frame.set_cursor_position((cursor_x, cursor_y));
        }

        // Render autocomplete suggestions floating above the console
        // Use 'inner' area so alignment matches text inside the borders
        self.render_suggestions(frame, inner);
    }

    // ── Shell rendering ───────────────────────────────────────────────────────

    fn render_shell(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(self.border_type)
            .border_style(Style::default().fg(self.theme.border))
            .title(" Shell ")
            .title_style(Style::default().fg(self.theme.accent).add_modifier(Modifier::BOLD));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let visible_lines = inner.height as usize;
        let max_scroll    = self.shell_history.len().saturating_sub(visible_lines);
        let start         = self.shell_scroll.min(max_scroll);
        let end           = (start + visible_lines).min(self.shell_history.len());

        let lines: Vec<Line> = self.shell_history[start..end]
            .iter()
            .map(|line| {
                if line.contains("@neuraos") && line.contains(">") {
                    Line::from(Span::styled(line.as_str(), Style::default().fg(self.theme.prompt)))
                } else if line.starts_with("  \u{2588}") || line.starts_with("  \u{255a}") {
                    Line::from(Span::styled(line.as_str(), Style::default().fg(self.theme.accent)))
                } else if line.starts_with("Error:") || line.starts_with("neura:") {
                    Line::from(Span::styled(line.as_str(), Style::default().fg(self.theme.error)))
                } else {
                    Line::from(Span::styled(line.as_str(), Style::default().fg(self.theme.fg)))
                }
            })
            .collect();

        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn render_input(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(self.border_type)
            .border_style(Style::default().fg(self.theme.border))
            .title(" Input ")
            .title_style(Style::default().fg(self.theme.accent));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let prompt      = format!("{} {}", self.shell_prompt, self.shell_input);
        let input_para  = Paragraph::new(prompt.as_str())
            .style(Style::default().fg(self.theme.prompt));
        frame.render_widget(input_para, inner);

        let cursor_x = inner.x + self.shell_prompt.len() as u16 + 1 + self.shell_cursor as u16;
        let cursor_y = inner.y;
        if cursor_x < inner.x + inner.width {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }

    fn render_suggestions(&self, frame: &mut Frame, input_area: Rect) {
        if self.suggestions.is_empty() { return; }

        let count     = self.suggestions.len().min(8);
        let max_text  = self.suggestions.iter().take(count).map(|s| s.len()).max().unwrap_or(10);
        let pop_w     = ((max_text + 4) as u16).min(input_area.width.saturating_sub(2)).max(12);
        let pop_h     = count as u16 + 2;

        let prompt_offset = self.shell_prompt.len() as u16 + 1;
        let x = (input_area.x + prompt_offset)
            .min(input_area.x + input_area.width.saturating_sub(pop_w));
        let y = input_area.y.saturating_sub(pop_h);

        let popup_area = Rect::new(x, y, pop_w, pop_h);
        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent));
        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        let items: Vec<Line> = self.suggestions.iter()
            .enumerate()
            .take(count)
            .map(|(i, sug)| {
                let style = if i == self.suggestion_selected {
                    Style::default()
                        .fg(self.theme.selected_fg)
                        .bg(self.theme.selected_bg)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(self.theme.fg)
                };
                Line::styled(format!(" {} ", sug), style)
            })
            .collect();

        frame.render_widget(Paragraph::new(items), inner);
    }

    // ── Overlays ──────────────────────────────────────────────────────────────

    fn render_command_palette(&self, frame: &mut Frame, area: Rect) {
        let pop_w = 62u16.min(area.width.saturating_sub(4));
        let pop_h = 18u16.min(area.height.saturating_sub(4));
        let x = (area.width.saturating_sub(pop_w)) / 2;
        let y = (area.height.saturating_sub(pop_h)) / 3;
        let popup_area = Rect::new(x, y, pop_w, pop_h);

        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(self.border_type)
            .border_style(Style::default().fg(self.theme.accent))
            .title(" Command Palette ")
            .title_style(Style::default().fg(self.theme.warning).add_modifier(Modifier::BOLD))
            .title_bottom(Span::styled(
                " Ctrl+P or Esc to close ",
                Style::default().fg(self.theme.border),
            ));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        if inner.height < 3 { return; }

        // Search input
        let search_area = Rect::new(inner.x, inner.y, inner.width, 1);
        frame.render_widget(
            Paragraph::new(format!("  > {}_", self.palette_input))
                .style(Style::default().fg(self.theme.prompt)),
            search_area,
        );

        // Separator
        let sep_area = Rect::new(inner.x, inner.y + 1, inner.width, 1);
        frame.render_widget(
            Paragraph::new("  ".to_string() + &"─".repeat(inner.width.saturating_sub(4) as usize))
                .style(Style::default().fg(self.theme.border)),
            sep_area,
        );

        // Filtered command list
        let list_height = inner.height.saturating_sub(2) as usize;
        let filtered    = self.filtered_palette_commands();
        let list_area   = Rect::new(inner.x, inner.y + 2, inner.width, list_height as u16);

        let items: Vec<Line> = filtered.iter().enumerate()
            .take(list_height)
            .map(|(i, (cmd, desc))| {
                let is_sel = i == self.palette_selected;
                let prefix = if is_sel { "  ❯ " } else { "    " };
                let cmd_style = if is_sel {
                    Style::default().fg(self.theme.selected_fg).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(self.theme.fg)
                };
                let desc_style = if is_sel {
                    Style::default().fg(self.theme.accent)
                } else {
                    Style::default().fg(self.theme.muted)
                };
                Line::from(vec![
                    Span::styled(prefix, cmd_style),
                    Span::styled(format!("{:<24}", cmd), cmd_style),
                    Span::styled(*desc, desc_style),
                ])
            })
            .collect();

        frame.render_widget(Paragraph::new(items), list_area);
    }

    fn render_help_overlay(&self, frame: &mut Frame, area: Rect) {
        let pop_w = 60u16.min(area.width.saturating_sub(4));
        let pop_h = 22u16.min(area.height.saturating_sub(4));
        let x = (area.width.saturating_sub(pop_w)) / 2;
        let y = (area.height.saturating_sub(pop_h)) / 3;
        let popup_area = Rect::new(x, y, pop_w, pop_h);

        frame.render_widget(Clear, popup_area);

        let help = vec![
            ("  Home Screen", ""),
            ("  Tab",               "Focus next section (Console→Pinned→All)"),
            ("  Shift+Tab",         "Focus previous section"),
            ("  ←/→",              "Navigate app grid"),
            ("  Enter (on app)",    "Open selected app"),
            ("  p (on app)",        "Pin / unpin app"),
            ("  Esc (in grid)",     "Return focus to console"),
            ("  F12",               "Toggle full shell view"),
            ("", ""),
            ("  Everywhere", ""),
            ("  Ctrl+P",            "Command palette"),
            ("  Ctrl+H",            "Toggle this help"),
            ("  Ctrl+L",            "Clear shell history"),
            ("  Ctrl+C",            "Cancel / close overlay"),
            ("  Ctrl+D",            "Exit NeuraOS"),
            ("  Tab (console)",     "Accept autocomplete"),
            ("  ↑/↓ (console)",    "Navigate history"),
            ("  PgUp/PgDn",         "Scroll shell output"),
        ];

        let text: Vec<Line> = help.iter()
            .map(|(key, desc)| {
                if desc.is_empty() {
                    Line::from(Span::styled(*key, Style::default().fg(self.theme.accent).add_modifier(Modifier::BOLD)))
                } else {
                    Line::from(vec![
                        Span::styled(format!("{:<24}", key), Style::default().fg(self.theme.fg)),
                        Span::styled(*desc, Style::default().fg(self.theme.muted)),
                    ])
                }
            })
            .collect();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.accent))
            .title(" Help ")
            .title_style(Style::default().fg(self.theme.success).add_modifier(Modifier::BOLD))
            .title_bottom(Span::styled(
                " Ctrl+H to close ",
                Style::default().fg(self.theme.border),
            ));

        frame.render_widget(Paragraph::new(text).block(block), popup_area);
    }
}
