use std::any::Any;
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui::style::{Color, Style, Modifier};
use crossterm::event::{KeyCode, KeyEvent};
use neura_app_framework::app_trait::App;
use neura_app_framework::consts::{OS_NAME, OS_VERSION};
use neura_app_framework::palette::{PRIMARY, TEXT, BORDER, MUTED};

/// ASCII art logo for NeuraOS displayed on the left side of the info panel.
const LOGO: &[&str] = &[
    r"  _   _  ___  ____  ",
    r" | \ | |/ _ \/ ___| ",
    r" |  \| | | | \___ \ ",
    r" | |\  | |_| |___) |",
    r" |_| \_|\___/|____/ ",
    r"                     ",
    r"   N e u r a O S     ",
];

/// The neofetch-style color palette row characters.
const PALETTE_BLOCK: &str = "███";

/// Classic neofetch palette colors (two rows: normal + bright).
const PALETTE_NORMAL: &[Color] = &[
    Color::Rgb(26, 27, 38),    // bg / black
    Color::Rgb(247, 118, 142), // red
    Color::Rgb(158, 206, 106), // green
    Color::Rgb(224, 175, 104), // yellow / orange
    Color::Rgb(122, 162, 247), // blue / primary
    Color::Rgb(187, 154, 247), // magenta / purple
    Color::Rgb(125, 207, 255), // cyan
    Color::Rgb(192, 202, 245), // white / text
];

const PALETTE_BRIGHT: &[Color] = &[
    Color::Rgb(59, 66, 97),    // bright black / border
    Color::Rgb(255, 150, 170), // bright red
    Color::Rgb(180, 230, 130), // bright green
    Color::Rgb(255, 200, 130), // bright yellow
    Color::Rgb(150, 185, 255), // bright blue
    Color::Rgb(210, 180, 255), // bright magenta
    Color::Rgb(155, 230, 255), // bright cyan
    Color::Rgb(220, 225, 250), // bright white
];

/// A system-info entry with a label and value.
#[derive(Debug, Clone)]
struct InfoEntry {
    label: String,
    value: String,
}

pub struct SysInfoApp {
    entries: Vec<InfoEntry>,
    initialized: bool,
}

impl SysInfoApp {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            initialized: false,
        }
    }

    /// Collect system information from compile-time and runtime sources.
    fn collect_info(&mut self) {
        self.entries.clear();

        let os_name = match std::env::consts::OS {
            "windows" => "Windows",
            "linux" => "Linux",
            "macos" => "macOS",
            other => other,
        };
        let arch = std::env::consts::ARCH;
        self.entries.push(InfoEntry {
            label: "OS".into(),
            value: format!("{} {} ({} {})", OS_NAME, OS_VERSION, os_name, arch),
        });
        self.entries.push(InfoEntry {
            label: "Host".into(),
            value: format!("{} {}", os_name, arch),
        });
        self.entries.push(InfoEntry {
            label: "Kernel".into(),
            value: format!("neura-kernel {}", OS_VERSION),
        });
        self.entries.push(InfoEntry {
            label: "Shell".into(),
            value: format!("neura-shell {}", OS_VERSION),
        });
        self.entries.push(InfoEntry {
            label: "DE".into(),
            value: format!("neura-desktop {}", OS_VERSION),
        });
        self.entries.push(InfoEntry {
            label: "AI Engine".into(),
            value: "neura-ai-core (Gemini)".into(),
        });
        self.entries.push(InfoEntry {
            label: "Database".into(),
            value: "SQLite via neura-storage".into(),
        });
        self.entries.push(InfoEntry {
            label: "Theme".into(),
            value: "Tokyo Night".into(),
        });
        self.entries.push(InfoEntry {
            label: "Packages".into(),
            value: "16 (neura-pkg)".into(),
        });

        // Compute a rough uptime estimate from the current process start time.
        let uptime_secs = {
            // std::time::Instant doesn't give wall-clock, but we can use
            // the elapsed since the app was initialized as an approximation.
            // For a real OS this would query /proc/uptime or similar.
            // We'll show process uptime via std::time::SystemTime.
            let now = std::time::SystemTime::now();
            now.duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() % 86400) // seconds within today as pseudo-uptime
                .unwrap_or(0)
        };
        let hours = uptime_secs / 3600;
        let minutes = (uptime_secs % 3600) / 60;
        self.entries.push(InfoEntry {
            label: "Uptime".into(),
            value: format!("~{}h {}m (session)", hours, minutes),
        });
        self.entries.push(InfoEntry {
            label: "Terminal".into(),
            value: "ratatui/crossterm TUI".into(),
        });
        self.entries.push(InfoEntry {
            label: "Arch".into(),
            value: arch.to_string(),
        });
    }
}

impl Default for SysInfoApp {
    fn default() -> Self {
        Self::new()
    }
}

impl App for SysInfoApp {
    fn id(&self) -> &str { "sysinfo" }
    fn name(&self) -> &str { "NeuraSystemInfo" }

    fn init(&mut self) -> anyhow::Result<()> {
        if !self.initialized {
            self.collect_info();
            self.initialized = true;
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => false,
            KeyCode::Char('r') => {
                self.collect_info();
                true
            }
            _ => true,
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        // Outer block
        let outer_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .title(" NeuraSystemInfo ")
            .title_style(Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD));
        let inner = outer_block.inner(area);
        frame.render_widget(outer_block, area);

        // Vertical layout: main content + palette row + help bar
        let vert = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(5),
                Constraint::Length(3),
                Constraint::Length(1),
            ])
            .split(inner);

        // Horizontal split: logo on left, info on right
        let logo_width: u16 = 24;
        let horiz = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(logo_width),
                Constraint::Min(20),
            ])
            .split(vert[0]);

        // ── Render ASCII logo ──
        let logo_lines: Vec<Line> = LOGO.iter().map(|line| {
            Line::from(Span::styled(*line, Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)))
        }).collect();
        let logo_widget = Paragraph::new(logo_lines)
            .alignment(Alignment::Left);
        frame.render_widget(logo_widget, horiz[0]);

        // ── Render system info entries ──
        let separator_line = Line::from(Span::styled(
            "─".repeat(horiz[1].width as usize),
            Style::default().fg(BORDER),
        ));

        let mut info_lines: Vec<Line> = Vec::new();

        // Username@hostname header
        info_lines.push(Line::from(vec![
            Span::styled("neura", Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled("@", Style::default().fg(MUTED)),
            Span::styled("neuraos", Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
        ]));
        info_lines.push(separator_line);

        for entry in &self.entries {
            info_lines.push(Line::from(vec![
                Span::styled(
                    format!("{:<12}", entry.label),
                    Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD),
                ),
                Span::styled(&entry.value, Style::default().fg(TEXT)),
            ]));
        }

        let info_widget = Paragraph::new(info_lines);
        frame.render_widget(info_widget, horiz[1]);

        // ── Render color palette bars ──
        let palette_area = vert[1];
        let mut palette_lines: Vec<Line> = Vec::new();

        // Normal row
        let normal_spans: Vec<Span> = PALETTE_NORMAL.iter().map(|&c| {
            Span::styled(PALETTE_BLOCK, Style::default().fg(c))
        }).collect();
        palette_lines.push(Line::from(normal_spans));

        // Bright row
        let bright_spans: Vec<Span> = PALETTE_BRIGHT.iter().map(|&c| {
            Span::styled(PALETTE_BLOCK, Style::default().fg(c))
        }).collect();
        palette_lines.push(Line::from(bright_spans));

        let palette_widget = Paragraph::new(palette_lines)
            .alignment(Alignment::Center);
        frame.render_widget(palette_widget, palette_area);

        // ── Help bar ──
        let help = Paragraph::new(" [r] refresh  [Esc] back")
            .style(Style::default().fg(MUTED));
        frame.render_widget(help, vert[2]);
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
