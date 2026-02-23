use std::any::Any;
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui::style::{Style, Modifier};
use crossterm::event::{KeyCode, KeyEvent};
use serde_json::Value;
use chrono::{Utc, FixedOffset};
use neura_app_framework::app_trait::App;
use neura_app_framework::palette::*;
use neura_app_framework::consts::WORLD_ZONES;

// ── Internal types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Tab {
    Clock,
    Stopwatch,
    Timer,
    Alarm,
    WorldClocks,
}

#[derive(Debug, Clone, PartialEq)]
enum AlarmField {
    Label,
    Hour,
    Minute,
}

#[derive(Debug, Clone)]
struct Alarm {
    label:   String,
    hour:    u8,
    minute:  u8,
    enabled: bool,
    ringing: bool,
}

// ── ClockApp ─────────────────────────────────────────────────────────────────

pub struct ClockApp {
    tab: Tab,

    // External settings — updated by main.rs on hot-reload
    pub use_24h:              bool,
    pub show_seconds:         bool,
    pub timezone_offset_mins: i32,
    pub timezone_label:       String,

    // Timezone-change signal — main.rs reads and clears this each tick
    pub timezone_changed:         bool,
    pub new_timezone_offset_mins: i32,
    pub new_timezone_label:       String,

    // Stopwatch
    sw_start:    Option<chrono::DateTime<Utc>>,
    sw_elapsed:  i64,   // accumulated ms
    sw_laps:     Vec<i64>,

    // Timer
    timer_total_secs:     i64,
    timer_start:          Option<chrono::DateTime<Utc>>,
    timer_paused_ms:      i64,   // remaining ms when paused
    timer_editing:        bool,
    timer_input:          String,
    timer_expired:        bool,

    // Alarms
    alarms:          Vec<Alarm>,
    alarm_selected:  usize,
    alarm_editing:   bool,
    alarm_field:     AlarmField,
    alarm_edit_buf:  String,

    // World clocks
    world_selected: usize,

    initialized: bool,
}

impl ClockApp {
    pub fn new() -> Self {
        Self {
            tab: Tab::Clock,
            use_24h:              true,
            show_seconds:         true,
            timezone_offset_mins: 0,
            timezone_label:       "UTC".to_string(),
            timezone_changed:         false,
            new_timezone_offset_mins: 0,
            new_timezone_label:       String::new(),
            sw_start:   None,
            sw_elapsed: 0,
            sw_laps:    Vec::new(),
            timer_total_secs: 0,
            timer_start:      None,
            timer_paused_ms:  0,
            timer_editing:    false,
            timer_input:      String::new(),
            timer_expired:    false,
            alarms: vec![
                Alarm { label: "Wake up".into(), hour: 7, minute: 0, enabled: false, ringing: false },
            ],
            alarm_selected: 0,
            alarm_editing:  false,
            alarm_field:    AlarmField::Hour,
            alarm_edit_buf: String::new(),
            world_selected: 0,
            initialized: false,
        }
    }

    /// Called every ~50 ms from main.rs to check timer/alarm expiry.
    pub fn tick(&mut self) {
        // Timer expiry
        if let Some(start) = self.timer_start {
            let elapsed_ms = Utc::now().signed_duration_since(start).num_milliseconds();
            if elapsed_ms >= self.timer_total_secs * 1000 && self.timer_total_secs > 0 {
                self.timer_start    = None;
                self.timer_paused_ms = 0;
                self.timer_expired  = true;
            }
        }

        // Alarms — fire once at second 0 of the target minute
        let now   = self.local_now();
        let h: u8 = now.format("%H").to_string().parse().unwrap_or(0);
        let m: u8 = now.format("%M").to_string().parse().unwrap_or(0);
        let s: u8 = now.format("%S").to_string().parse().unwrap_or(1); // default non-zero
        if s == 0 {
            for alarm in &mut self.alarms {
                if alarm.enabled && alarm.hour == h && alarm.minute == m {
                    alarm.ringing = true;
                }
            }
        }
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn local_now(&self) -> chrono::DateTime<FixedOffset> {
        let offset = FixedOffset::east_opt(self.timezone_offset_mins * 60)
            .unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
        Utc::now().with_timezone(&offset)
    }

    fn sw_running(&self) -> bool { self.sw_start.is_some() }

    fn sw_total_ms(&self) -> i64 {
        let running = self.sw_start.map_or(0, |s| {
            Utc::now().signed_duration_since(s).num_milliseconds()
        });
        self.sw_elapsed + running
    }

    fn timer_running(&self) -> bool { self.timer_start.is_some() }

    fn timer_remaining_ms(&self) -> i64 {
        match self.timer_start {
            Some(start) => {
                let elapsed = Utc::now().signed_duration_since(start).num_milliseconds();
                (self.timer_total_secs * 1000 - elapsed).max(0)
            }
            None => self.timer_paused_ms,
        }
    }

    fn fmt_ms(ms: i64) -> String {
        let ms_part = ms % 1000;
        let t = ms / 1000;
        let s = t % 60; let t = t / 60;
        let m = t % 60; let h = t / 60;
        format!("{:02}:{:02}:{:02}.{:03}", h, m, s, ms_part)
    }

    fn fmt_secs(total: i64) -> String {
        let s = total % 60; let t = total / 60;
        let m = t % 60;     let h = t / 60;
        format!("{:02}:{:02}:{:02}", h, m, s)
    }

    fn parse_timer_input(&mut self) {
        let digits: String = self.timer_input.chars().filter(|c| c.is_ascii_digit()).collect();
        let secs = match digits.len() {
            0 => 0,
            1 | 2 => digits.parse::<i64>().unwrap_or(0),
            3 | 4 => {
                let (mins, secs) = digits.split_at(digits.len() - 2);
                mins.parse::<i64>().unwrap_or(0) * 60 + secs.parse::<i64>().unwrap_or(0)
            }
            _ => {
                let (hrs, rest) = digits.split_at(digits.len() - 4);
                let (mins, secs) = rest.split_at(2);
                hrs.parse::<i64>().unwrap_or(0) * 3600
                    + mins.parse::<i64>().unwrap_or(0) * 60
                    + secs.parse::<i64>().unwrap_or(0)
            }
        };
        self.timer_total_secs = secs;
        self.timer_paused_ms  = secs * 1000;
    }

    // ── Big-digit renderer ────────────────────────────────────────────────────

    /// Each digit is exactly 5 display-columns wide so all rows line up perfectly.
    fn digit_art(d: char) -> [&'static str; 5] {
        match d {
            // 0 — oval
            '0' => [" ███ ", "█   █", "█   █", "█   █", " ███ "],
            // 1 — serif top, foot at bottom
            '1' => [" ██  ", "  █  ", "  █  ", "  █  ", " ███ "],
            // 2 — top-right, step down left, full bottom
            '2' => [" ███ ", "    █", "  ██ ", " █   ", "█████"],
            // 3 — double right-side bumps
            '3' => [" ███ ", "    █", "  ██ ", "    █", " ███ "],
            // 4 — open top, crossbar, right column
            '4' => ["█   █", "█   █", "█████", "    █", "    █"],
            // 5 — top-left, step to bottom-right
            '5' => ["█████", "█    ", "████ ", "    █", "█████"],
            // 6 — top-left, loop at bottom
            '6' => [" ███ ", "█    ", "████ ", "█   █", " ███ "],
            // 7 — diagonal descent
            '7' => ["█████", "   █ ", "  █  ", " █   ", " █   "],
            // 8 — double oval (figure 8)
            '8' => [" ███ ", "█   █", " ███ ", "█   █", " ███ "],
            // 9 — top oval, right descent
            '9' => [" ███ ", "█   █", " ████", "    █", " ███ "],
            // : — two centred dots
            ':' => ["     ", "  █  ", "     ", "  █  ", "     "],
            _   => ["     ", "     ", "     ", "     ", "     "],
        }
    }

    fn build_big(s: &str) -> Vec<String> {
        let mut rows = vec![String::new(); 5];
        for ch in s.chars() {
            let art = Self::digit_art(ch);
            for (i, row) in art.iter().enumerate() {
                rows[i].push_str(row);
                rows[i].push(' ');  // 1-column gap between glyphs
            }
        }
        rows
    }

    /// Centre `text` inside `width` terminal columns.
    ///
    /// IMPORTANT: uses `.chars().count()` (display columns), NOT `.len()` (bytes).
    /// `█` is 3 bytes in UTF-8 but occupies 1 terminal column, so byte-count
    /// centering would produce completely wrong alignment.
    fn centered(text: &str, width: u16) -> String {
        let cols = text.chars().count();
        let w    = width as usize;
        if cols >= w { return text.to_string(); }
        let pad = (w - cols) / 2;
        format!("{}{}", " ".repeat(pad), text)
    }

    // ── Tab-bar render ────────────────────────────────────────────────────────

    fn render_tabbar(&self, frame: &mut Frame, area: Rect) {
        let tabs = [
            ("[1] Clock", Tab::Clock),
            ("[2] Stopwatch", Tab::Stopwatch),
            ("[3] Timer", Tab::Timer),
            ("[4] Alarm", Tab::Alarm),
            ("[5] World", Tab::WorldClocks),
        ];
        let mut spans: Vec<Span> = Vec::new();
        for (label, t) in &tabs {
            let style = if *t == self.tab {
                Style::default().fg(BG).bg(PRIMARY).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(STATUSBAR_MUTED)
            };
            spans.push(Span::styled(*label, style));
            spans.push(Span::raw("  "));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
    }

    fn render_help(&self, frame: &mut Frame, area: Rect) {
        let text = match self.tab {
            Tab::Clock =>
                " [1-5] switch tab  [Tab] next  [Esc] exit",
            Tab::Stopwatch =>
                " [s/Enter] start/stop  [l] lap  [r] reset  [Tab] next",
            Tab::Timer => if self.timer_editing {
                " Type digits (e.g. 500=5m  300=3m  13000=1h30m) then [Enter]  [Esc] cancel"
            } else {
                " [s/Enter] start/pause  [r] reset  [e] set duration  [Tab] next"
            },
            Tab::Alarm => if self.alarm_editing {
                " Type new value then [Enter] to save  [Esc] cancel"
            } else {
                " [t] toggle  [n] new  [d] delete  [e] label  [h] hour  [m] minute"
            },
            Tab::WorldClocks =>
                " [j/k or ↑↓] select  [Enter] set as my timezone  [Tab] next",
        };
        frame.render_widget(
            Paragraph::new(text).style(Style::default().fg(STATUSBAR_MUTED)),
            area,
        );
    }
}

// ── App trait ─────────────────────────────────────────────────────────────────

impl App for ClockApp {
    fn id(&self)   -> &str { "clock" }
    fn name(&self) -> &str { "NeuraClock" }

    fn init(&mut self) -> anyhow::Result<()> {
        self.initialized = true;
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        // Dismiss ringing / expired first
        let any_ringing = self.alarms.iter().any(|a| a.ringing);
        if any_ringing || self.timer_expired {
            if key.code == KeyCode::Esc || key.code == KeyCode::Enter || key.code == KeyCode::Char(' ') {
                for a in &mut self.alarms { a.ringing = false; }
                self.timer_expired = false;
                return true;
            }
        }

        // Tab switching (global)
        match key.code {
            KeyCode::Char('1') => { self.tab = Tab::Clock;       return true; }
            KeyCode::Char('2') => { self.tab = Tab::Stopwatch;   return true; }
            KeyCode::Char('3') => { self.tab = Tab::Timer;       return true; }
            KeyCode::Char('4') => { self.tab = Tab::Alarm;       return true; }
            KeyCode::Char('5') => { self.tab = Tab::WorldClocks; return true; }
            KeyCode::Tab => {
                self.tab = match self.tab {
                    Tab::Clock       => Tab::Stopwatch,
                    Tab::Stopwatch   => Tab::Timer,
                    Tab::Timer       => Tab::Alarm,
                    Tab::Alarm       => Tab::WorldClocks,
                    Tab::WorldClocks => Tab::Clock,
                };
                return true;
            }
            KeyCode::Esc => {
                if self.tab == Tab::Timer && self.timer_editing {
                    self.timer_editing = false;
                    self.timer_input.clear();
                    return true;
                }
                if self.tab == Tab::Alarm && self.alarm_editing {
                    self.alarm_editing = false;
                    return true;
                }
                return false; // close app
            }
            _ => {}
        }

        match self.tab {
            Tab::Clock       => self.key_clock(key),
            Tab::Stopwatch   => self.key_stopwatch(key),
            Tab::Timer       => self.key_timer(key),
            Tab::Alarm       => self.key_alarm(key),
            Tab::WorldClocks => self.key_world(key),
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        // Layout: tabbar(1) | content(min) | help(1)
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(5), Constraint::Length(1)])
            .split(area);

        self.render_tabbar(frame, chunks[0]);

        match self.tab {
            Tab::Clock       => self.draw_clock(frame, chunks[1]),
            Tab::Stopwatch   => self.draw_stopwatch(frame, chunks[1]),
            Tab::Timer       => self.draw_timer(frame, chunks[1]),
            Tab::Alarm       => self.draw_alarms(frame, chunks[1]),
            Tab::WorldClocks => self.draw_world(frame, chunks[1]),
        }

        self.render_help(frame, chunks[2]);
    }

    fn on_close(&mut self) {}

    fn save_state(&self) -> Option<Value> {
        Some(serde_json::json!({
            "sw_elapsed": self.sw_elapsed,
            "sw_running": self.sw_running(),
            "timer_total_secs": self.timer_total_secs,
            "timer_remaining_ms": self.timer_remaining_ms(),
            "alarms": self.alarms.iter().map(|a| serde_json::json!({
                "label": a.label, "hour": a.hour,
                "minute": a.minute, "enabled": a.enabled,
            })).collect::<Vec<_>>(),
        }))
    }

    fn load_state(&mut self, state: Value) {
        if let Some(v) = state.get("sw_elapsed").and_then(|v| v.as_i64()) {
            self.sw_elapsed = v;
        }
        if state.get("sw_running").and_then(|v| v.as_bool()) == Some(true) {
            self.sw_start = Some(Utc::now());
        }
        if let Some(v) = state.get("timer_total_secs").and_then(|v| v.as_i64()) {
            self.timer_total_secs = v;
        }
        if let Some(v) = state.get("timer_remaining_ms").and_then(|v| v.as_i64()) {
            self.timer_paused_ms = v;
        }
        if let Some(arr) = state.get("alarms").and_then(|v| v.as_array()) {
            let loaded: Vec<Alarm> = arr.iter().filter_map(|a| {
                Some(Alarm {
                    label:   a.get("label")?.as_str()?.to_string(),
                    hour:    a.get("hour")?.as_u64()? as u8,
                    minute:  a.get("minute")?.as_u64()? as u8,
                    enabled: a.get("enabled")?.as_bool()?,
                    ringing: false,
                })
            }).collect();
            if !loaded.is_empty() { self.alarms = loaded; }
        }
    }

    fn ai_tools(&self) -> Vec<Value> { Vec::new() }
    fn handle_ai_tool(&mut self, _: &str, _: Value) -> Option<Value> { None }
    fn as_any(&self)     -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

// ── Key handlers ──────────────────────────────────────────────────────────────

impl ClockApp {
    fn key_clock(&mut self, _key: KeyEvent) -> bool { true }

    fn key_stopwatch(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('s') | KeyCode::Enter => {
                if self.sw_running() {
                    if let Some(start) = self.sw_start.take() {
                        self.sw_elapsed += Utc::now().signed_duration_since(start).num_milliseconds();
                    }
                } else {
                    self.sw_start = Some(Utc::now());
                }
            }
            KeyCode::Char('l') => {
                if self.sw_running() {
                    self.sw_laps.push(self.sw_total_ms());
                }
            }
            KeyCode::Char('r') => {
                self.sw_start   = None;
                self.sw_elapsed = 0;
                self.sw_laps.clear();
            }
            _ => {}
        }
        true
    }

    fn key_timer(&mut self, key: KeyEvent) -> bool {
        if self.timer_editing {
            match key.code {
                KeyCode::Char(c) if c.is_ascii_digit() => { self.timer_input.push(c); }
                KeyCode::Backspace => { self.timer_input.pop(); }
                KeyCode::Enter => {
                    self.parse_timer_input();
                    self.timer_editing = false;
                }
                _ => {}
            }
            return true;
        }
        match key.code {
            KeyCode::Char('e') => {
                self.timer_editing = true;
                self.timer_input.clear();
            }
            KeyCode::Char('s') | KeyCode::Enter => {
                if self.timer_running() {
                    // Pause
                    self.timer_paused_ms = self.timer_remaining_ms();
                    self.timer_start     = None;
                } else if self.timer_paused_ms > 0 {
                    // Resume from paused position
                    let remaining = self.timer_paused_ms;
                    let fake_start = Utc::now()
                        - chrono::Duration::milliseconds(self.timer_total_secs * 1000 - remaining);
                    self.timer_start    = Some(fake_start);
                    self.timer_paused_ms = 0;
                } else if self.timer_total_secs > 0 {
                    // Fresh start
                    self.timer_start     = Some(Utc::now());
                    self.timer_paused_ms = 0;
                    self.timer_expired   = false;
                }
            }
            KeyCode::Char('r') => {
                self.timer_start     = None;
                self.timer_paused_ms = self.timer_total_secs * 1000;
                self.timer_expired   = false;
            }
            _ => {}
        }
        true
    }

    fn key_alarm(&mut self, key: KeyEvent) -> bool {
        if self.alarm_editing {
            match key.code {
                KeyCode::Char(c) => { self.alarm_edit_buf.push(c); }
                KeyCode::Backspace => { self.alarm_edit_buf.pop(); }
                KeyCode::Enter => {
                    if let Some(alarm) = self.alarms.get_mut(self.alarm_selected) {
                        match self.alarm_field {
                            AlarmField::Label  => { alarm.label = self.alarm_edit_buf.clone(); }
                            AlarmField::Hour   => {
                                if let Ok(h) = self.alarm_edit_buf.parse::<u8>() {
                                    alarm.hour = h % 24;
                                }
                            }
                            AlarmField::Minute => {
                                if let Ok(m) = self.alarm_edit_buf.parse::<u8>() {
                                    alarm.minute = m % 60;
                                }
                            }
                        }
                    }
                    self.alarm_editing = false;
                }
                _ => {}
            }
            return true;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.alarm_selected > 0 { self.alarm_selected -= 1; }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.alarm_selected + 1 < self.alarms.len() { self.alarm_selected += 1; }
            }
            KeyCode::Char('t') | KeyCode::Enter => {
                if let Some(a) = self.alarms.get_mut(self.alarm_selected) { a.enabled = !a.enabled; }
            }
            KeyCode::Char('n') => {
                self.alarms.push(Alarm {
                    label: format!("Alarm {}", self.alarms.len() + 1),
                    hour: 8, minute: 0, enabled: false, ringing: false,
                });
                self.alarm_selected = self.alarms.len() - 1;
            }
            KeyCode::Char('d') => {
                if !self.alarms.is_empty() {
                    self.alarms.remove(self.alarm_selected);
                    if self.alarm_selected > 0 && self.alarm_selected >= self.alarms.len() {
                        self.alarm_selected -= 1;
                    }
                }
            }
            KeyCode::Char('e') => {
                if let Some(a) = self.alarms.get(self.alarm_selected) {
                    self.alarm_field   = AlarmField::Label;
                    self.alarm_edit_buf = a.label.clone();
                    self.alarm_editing  = true;
                }
            }
            KeyCode::Char('h') => {
                if let Some(a) = self.alarms.get(self.alarm_selected) {
                    self.alarm_field   = AlarmField::Hour;
                    self.alarm_edit_buf = a.hour.to_string();
                    self.alarm_editing  = true;
                }
            }
            KeyCode::Char('m') => {
                if let Some(a) = self.alarms.get(self.alarm_selected) {
                    self.alarm_field   = AlarmField::Minute;
                    self.alarm_edit_buf = a.minute.to_string();
                    self.alarm_editing  = true;
                }
            }
            _ => {}
        }
        true
    }

    fn key_world(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.world_selected > 0 { self.world_selected -= 1; }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.world_selected + 1 < WORLD_ZONES.len() { self.world_selected += 1; }
            }
            KeyCode::Enter => {
                let (label, offset_mins) = WORLD_ZONES[self.world_selected];
                self.timezone_changed         = true;
                self.new_timezone_label       = label.to_string();
                self.new_timezone_offset_mins = offset_mins;
            }
            _ => {}
        }
        true
    }
}

// ── Render methods ─────────────────────────────────────────────────────────────

impl ClockApp {
    fn draw_clock(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .title(" NeuraClock ")
            .title_style(Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let now = self.local_now();

        // ── Time string ───────────────────────────────────────────────────────
        let time_str = if self.use_24h {
            if self.show_seconds { now.format("%H:%M:%S").to_string() }
            else                 { now.format("%H:%M").to_string() }
        } else {
            if self.show_seconds { now.format("%I:%M:%S").to_string() }
            else                 { now.format("%I:%M").to_string() }
        };
        let am_pm    = if !self.use_24h { now.format(" %p").to_string() } else { String::new() };
        let date_str = now.format("%A, %B %d, %Y").to_string();

        let h = self.timezone_offset_mins / 60;
        let m = (self.timezone_offset_mins % 60).abs();
        let tz_str = if m == 0 {
            format!("{}  UTC{:+}", self.timezone_label, h)
        } else {
            format!("{}  UTC{:+}:{:02}", self.timezone_label, h, m)
        };

        // ── Build big digits ──────────────────────────────────────────────────
        let big = Self::build_big(&time_str);
        // Display width of the big-digit row (char count, NOT byte count)
        let big_cols = big[0].chars().count() as u16;

        // Separator line matching the big-digit width (capped to inner area)
        let sep_cols = big_cols.min(inner.width.saturating_sub(2)) as usize;
        let sep      = "─".repeat(sep_cols);

        // ── Layout ───────────────────────────────────────────────────────────
        // 5 digit rows
        // 1 AM/PM or blank
        // 1 separator
        // 1 date
        // 1 timezone
        // = 9 rows of content
        let content_h: u16 = 9;
        let top = if inner.height > content_h { (inner.height - content_h) / 2 } else { 0 };

        let mut lines: Vec<Line> = vec![Line::from(""); top as usize];

        // Big digits — accent blue, bold
        for row in &big {
            lines.push(Line::from(Span::styled(
                Self::centered(row, inner.width),
                Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD),
            )));
        }

        // AM/PM (12 h) or blank row for spacing
        if !am_pm.is_empty() {
            lines.push(Line::from(Span::styled(
                Self::centered(am_pm.trim(), inner.width),
                Style::default().fg(PRIMARY),
            )));
        } else {
            lines.push(Line::from(""));
        }

        // Thin separator between clock face and info area
        lines.push(Line::from(Span::styled(
            Self::centered(&sep, inner.width),
            Style::default().fg(BORDER),
        )));

        // Date — full foreground colour
        lines.push(Line::from(Span::styled(
            Self::centered(&date_str, inner.width),
            Style::default().fg(TEXT),
        )));

        // Timezone + UTC offset — muted
        lines.push(Line::from(Span::styled(
            Self::centered(&tz_str, inner.width),
            Style::default().fg(STATUSBAR_MUTED),
        )));

        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn draw_stopwatch(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .title(" Stopwatch ")
            .title_style(Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let total_ms = self.sw_total_ms();
        let full     = Self::fmt_ms(total_ms);
        let hms      = &full[..8];   // "HH:MM:SS"
        let frac     = &full[8..];   // ".mmm"

        let big = Self::build_big(hms);

        let (status_text, status_color) = if self.sw_running() {
            ("● RUNNING", GREEN)
        } else if total_ms > 0 {
            ("■ PAUSED", ORANGE)
        } else {
            ("○ READY", STATUSBAR_MUTED)
        };

        let laps_shown = self.sw_laps.len().min(5) as u16;
        let content_h: u16 = 5 + 1 + 1 + 1 + if laps_shown > 0 { 1 + laps_shown } else { 0 };
        let top = if inner.height > content_h { (inner.height - content_h) / 2 } else { 0 };

        let mut lines: Vec<Line> = vec![Line::from(""); top as usize];

        for row in &big {
            lines.push(Line::from(Span::styled(
                Self::centered(row, inner.width),
                Style::default().fg(ORANGE).add_modifier(Modifier::BOLD),
            )));
        }

        lines.push(Line::from(Span::styled(
            Self::centered(frac, inner.width),
            Style::default().fg(STATUSBAR_MUTED),
        )));

        lines.push(Line::from(""));

        lines.push(Line::from(Span::styled(
            Self::centered(status_text, inner.width),
            Style::default().fg(status_color).add_modifier(Modifier::BOLD),
        )));

        if !self.sw_laps.is_empty() {
            lines.push(Line::from(""));
            let start_i = self.sw_laps.len().saturating_sub(5);
            for (i, &lap_ms) in self.sw_laps.iter().enumerate().skip(start_i) {
                let lt = Self::fmt_ms(lap_ms);
                lines.push(Line::from(vec![
                    Span::styled(format!("  Lap {:>2}: ", i + 1), Style::default().fg(STATUSBAR_MUTED)),
                    Span::styled(lt, Style::default().fg(TEXT)),
                ]));
            }
        }

        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn draw_timer(&self, frame: &mut Frame, area: Rect) {
        let (title, title_color) = if self.timer_expired {
            (" Timer — TIME'S UP! ", RED)
        } else {
            (" Timer ", PRIMARY)
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if self.timer_expired { RED } else { BORDER }))
            .title(title)
            .title_style(Style::default().fg(title_color).add_modifier(Modifier::BOLD));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.timer_editing {
            let top = if inner.height > 5 { (inner.height - 5) / 2 } else { 0 };
            let mut lines: Vec<Line> = vec![Line::from(""); top as usize];
            lines.push(Line::from(Span::styled(
                Self::centered("Set Timer Duration", inner.width),
                Style::default().fg(STATUSBAR_MUTED),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                Self::centered(&format!("> {}▋", self.timer_input), inner.width),
                Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                Self::centered("e.g. 300 = 5 min | 3000 = 30 min | 10000 = 1h 40m", inner.width),
                Style::default().fg(STATUSBAR_MUTED),
            )));
            frame.render_widget(Paragraph::new(lines), inner);
            return;
        }

        let remaining_ms   = self.timer_remaining_ms();
        let remaining_secs = remaining_ms / 1000;
        let time_str = Self::fmt_secs(remaining_secs);
        let big = Self::build_big(&time_str);

        let digit_color = if self.timer_expired {
            RED
        } else if remaining_secs <= 10 && self.timer_running() {
            ORANGE
        } else {
            GREEN
        };

        let status = if self.timer_expired {
            "TIME'S UP!"
        } else if self.timer_running() {
            "COUNTING DOWN"
        } else if self.timer_total_secs == 0 {
            "Press [e] to set duration"
        } else {
            "PAUSED"
        };

        let content_h: u16 = 5 + 1 + 1 + 1;
        let top = if inner.height > content_h { (inner.height - content_h) / 2 } else { 0 };
        let mut lines: Vec<Line> = vec![Line::from(""); top as usize];

        for row in &big {
            lines.push(Line::from(Span::styled(
                Self::centered(row, inner.width),
                Style::default().fg(digit_color).add_modifier(Modifier::BOLD),
            )));
        }

        lines.push(Line::from(""));

        lines.push(Line::from(Span::styled(
            Self::centered(&format!("Total: {}", Self::fmt_secs(self.timer_total_secs)), inner.width),
            Style::default().fg(STATUSBAR_MUTED),
        )));

        lines.push(Line::from(Span::styled(
            Self::centered(status, inner.width),
            Style::default().fg(digit_color).add_modifier(Modifier::BOLD),
        )));

        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn draw_alarms(&self, frame: &mut Frame, area: Rect) {
        let any_ringing = self.alarms.iter().any(|a| a.ringing);
        let (title, t_color, b_color) = if any_ringing {
            (" Alarms — RINGING! Press Space/Enter to dismiss ", RED, RED)
        } else {
            (" Alarms ", PRIMARY, BORDER)
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(b_color))
            .title(title)
            .title_style(Style::default().fg(t_color).add_modifier(Modifier::BOLD));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.alarm_editing {
            let field_name = match self.alarm_field {
                AlarmField::Label  => "Alarm Label",
                AlarmField::Hour   => "Hour (0–23)",
                AlarmField::Minute => "Minute (0–59)",
            };
            let top = if inner.height > 5 { (inner.height - 5) / 2 } else { 0 };
            let mut lines: Vec<Line> = vec![Line::from(""); top as usize];
            lines.push(Line::from(Span::styled(
                Self::centered(&format!("Edit: {}", field_name), inner.width),
                Style::default().fg(STATUSBAR_MUTED),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                Self::centered(&format!("> {}▋", self.alarm_edit_buf), inner.width),
                Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                Self::centered("[Enter] save  [Esc] cancel", inner.width),
                Style::default().fg(STATUSBAR_MUTED),
            )));
            frame.render_widget(Paragraph::new(lines), inner);
            return;
        }

        if self.alarms.is_empty() {
            let msg = "No alarms — press [n] to create one";
            let top = if inner.height > 1 { inner.height / 2 } else { 0 };
            let mut lines: Vec<Line> = vec![Line::from(""); top as usize];
            lines.push(Line::from(Span::styled(
                Self::centered(msg, inner.width),
                Style::default().fg(STATUSBAR_MUTED),
            )));
            frame.render_widget(Paragraph::new(lines), inner);
            return;
        }

        let items: Vec<Line> = self.alarms.iter().enumerate().map(|(i, alarm)| {
            let selected = i == self.alarm_selected;
            let prefix = if selected { " ▶ " } else { "   " };

            let (status_txt, status_col) = if alarm.ringing {
                ("🔔 RINGING!", RED)
            } else if alarm.enabled {
                ("[ON] ", GREEN)
            } else {
                ("[OFF]", STATUSBAR_MUTED)
            };

            let row_style = if selected {
                Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT)
            };

            Line::from(vec![
                Span::styled(prefix, row_style),
                Span::styled(status_txt, Style::default().fg(status_col).add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                Span::styled(format!("{:02}:{:02}", alarm.hour, alarm.minute), row_style),
                Span::raw("  "),
                Span::styled(alarm.label.as_str(), row_style),
            ])
        }).collect();

        frame.render_widget(Paragraph::new(items), inner);
    }

    fn draw_world(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .title(" World Clocks ")
            .title_style(Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let now_utc = Utc::now();
        let visible = inner.height as usize;
        let scroll = if self.world_selected >= visible {
            self.world_selected + 1 - visible
        } else {
            0
        };

        let items: Vec<Line> = WORLD_ZONES.iter().enumerate()
            .skip(scroll)
            .take(visible)
            .map(|(i, (label, offset_mins))| {
                let offset = FixedOffset::east_opt(*offset_mins * 60)
                    .unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
                let local = now_utc.with_timezone(&offset);
                let time  = local.format("%H:%M:%S").to_string();
                let is_sel     = i == self.world_selected;
                let is_active  = *offset_mins == self.timezone_offset_mins
                    && label == &self.timezone_label.as_str();

                let prefix = if is_sel { " ▶ " } else { "   " };
                let active_marker = if is_active { "  ← active" } else { "" };

                let row_style = if is_sel {
                    Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)
                } else if is_active {
                    Style::default().fg(GREEN)
                } else {
                    Style::default().fg(TEXT)
                };

                Line::from(vec![
                    Span::styled(prefix, row_style),
                    Span::styled(format!("{:<26}", label), row_style),
                    Span::styled(time, Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
                    Span::styled(active_marker, Style::default().fg(GREEN)),
                ])
            })
            .collect();

        frame.render_widget(Paragraph::new(items), inner);
    }
}
