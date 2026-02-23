use std::any::Any;
use std::sync::Arc;
use std::collections::HashMap;
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui::style::{Style, Modifier};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use chrono::{Utc, Datelike, NaiveDate, Duration};
use neura_app_framework::app_trait::App;
use neura_app_framework::palette::*;
use neura_storage::vfs::Vfs;

// ── Calendar Event ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CalendarEvent {
    id:    u64,
    title: String,
    time:  String,   // "HH:MM" or empty
    notes: String,   // optional notes
}

// ── UI modes ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum ViewMode { Month, Year }

#[derive(Debug, Clone, PartialEq)]
enum UiMode { Normal, EventForm, DeleteConfirm }

#[derive(Debug, Clone, PartialEq)]
enum FormField { Title, Time, Notes }

// ── App ───────────────────────────────────────────────────────────────────────

pub struct CalendarApp {
    // Date state
    today:     NaiveDate,
    selected:  NaiveDate,
    view_mode: ViewMode,
    view_year: i32,

    // Events  (key: "YYYY-MM-DD")
    events:       HashMap<String, Vec<CalendarEvent>>,
    event_cursor: usize,   // selected index in the event list for `selected` day
    next_id:      u64,

    // UI mode
    ui_mode: UiMode,

    // Event form
    form_field:   FormField,
    form_title:   String,
    form_time:    String,
    form_notes:   String,
    form_edit_id: Option<u64>,   // None = new, Some(id) = editing

    // Delete confirm
    delete_candidate_id: Option<u64>,

    // Persistence
    vfs:        Arc<Vfs>,
    username:   String,
    pub needs_load: bool,
    pub needs_save: bool,
}

impl CalendarApp {
    pub fn new(vfs: Arc<Vfs>, username: &str) -> Self {
        let today = Utc::now().date_naive();
        Self {
            today,
            selected:  today,
            view_mode: ViewMode::Month,
            view_year: today.year(),
            events:       HashMap::new(),
            event_cursor: 0,
            next_id:      1,
            ui_mode:      UiMode::Normal,
            form_field:   FormField::Title,
            form_title:   String::new(),
            form_time:    String::new(),
            form_notes:   String::new(),
            form_edit_id: None,
            delete_candidate_id: None,
            vfs: vfs.clone(),
            username: username.to_string(),
            needs_load: true,
            needs_save: false,
        }
    }

    // ── Async persistence ─────────────────────────────────────────────────────

    pub async fn async_load_events(&mut self) {
        self.needs_load = false;
        let path = format!("/home/{}/calendar_events.json", self.username);
        if let Ok(bytes) = self.vfs.read_file(&path).await {
            if let Ok(data) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                if let Some(obj) = data.as_object() {
                    let mut map: HashMap<String, Vec<CalendarEvent>> = HashMap::new();
                    let mut max_id = 0u64;
                    for (k, v) in obj {
                        if let Ok(evts) = serde_json::from_value::<Vec<CalendarEvent>>(v.clone()) {
                            for e in &evts { if e.id > max_id { max_id = e.id; } }
                            map.insert(k.clone(), evts);
                        }
                    }
                    self.events  = map;
                    self.next_id = max_id + 1;
                }
            }
        }
    }

    pub async fn async_save_events(&mut self) {
        self.needs_save = false;
        let path = format!("/home/{}/calendar_events.json", self.username);
        if let Ok(bytes) = serde_json::to_vec(&self.events) {
            let _ = self.vfs.write_file(&path, bytes, &self.username).await;
        }
    }

    // ── Event helpers ─────────────────────────────────────────────────────────

    fn date_key(date: NaiveDate) -> String {
        format!("{}", date.format("%Y-%m-%d"))
    }

    fn events_for(&self, date: NaiveDate) -> &[CalendarEvent] {
        self.events.get(&Self::date_key(date)).map(|v| v.as_slice()).unwrap_or(&[])
    }

    fn has_events(&self, date: NaiveDate) -> bool {
        self.events.get(&Self::date_key(date)).map(|v| !v.is_empty()).unwrap_or(false)
    }

    fn open_new_form(&mut self) {
        self.form_edit_id = None;
        self.form_title.clear();
        self.form_time.clear();
        self.form_notes.clear();
        self.form_field = FormField::Title;
        self.ui_mode    = UiMode::EventForm;
    }

    fn open_edit_form(&mut self) {
        let events = self.events_for(self.selected);
        if events.is_empty() { return; }
        let idx = self.event_cursor.min(events.len().saturating_sub(1));
        let ev  = events[idx].clone();
        self.form_edit_id = Some(ev.id);
        self.form_title   = ev.title;
        self.form_time    = ev.time;
        self.form_notes   = ev.notes;
        self.form_field   = FormField::Title;
        self.ui_mode      = UiMode::EventForm;
    }

    fn open_delete_confirm(&mut self) {
        let events = self.events_for(self.selected);
        if events.is_empty() { return; }
        let idx = self.event_cursor.min(events.len().saturating_sub(1));
        self.delete_candidate_id = Some(events[idx].id);
        self.ui_mode = UiMode::DeleteConfirm;
    }

    fn save_form(&mut self) {
        let title = self.form_title.trim().to_string();
        if title.is_empty() { return; }

        let id = if let Some(eid) = self.form_edit_id {
            eid
        } else {
            let new_id = self.next_id;
            self.next_id += 1;
            new_id
        };

        let ev = CalendarEvent {
            id,
            title,
            time:  self.form_time.trim().to_string(),
            notes: self.form_notes.trim().to_string(),
        };

        let key  = Self::date_key(self.selected);
        let list = self.events.entry(key).or_default();

        if let Some(edit_id) = self.form_edit_id {
            if let Some(pos) = list.iter().position(|e| e.id == edit_id) {
                list[pos] = ev;
            } else {
                list.push(ev);
            }
        } else {
            list.push(ev);
        }

        self.ui_mode    = UiMode::Normal;
        self.needs_save = true;
    }

    fn delete_confirmed(&mut self) {
        if let Some(del_id) = self.delete_candidate_id.take() {
            let key = Self::date_key(self.selected);
            if let Some(list) = self.events.get_mut(&key) {
                list.retain(|e| e.id != del_id);
                if list.is_empty() { self.events.remove(&key); }
            }
            let cnt = self.events_for(self.selected).len();
            if cnt > 0 && self.event_cursor >= cnt { self.event_cursor = cnt - 1; }
            if cnt == 0 { self.event_cursor = 0; }
            self.needs_save = true;
        }
        self.ui_mode = UiMode::Normal;
    }

    // ── Key handlers ──────────────────────────────────────────────────────────

    fn handle_normal_key(&mut self, key: KeyEvent) -> bool {
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);

        match self.view_mode {
            ViewMode::Month => match key.code {
                // ── Event actions ──────────────────────────────────────────
                KeyCode::Char('n') => { self.open_new_form();    true }
                KeyCode::Char('e') => { self.open_edit_form();   true }
                KeyCode::Char('d') | KeyCode::Delete => {
                    self.open_delete_confirm(); true
                }
                // Event list navigation
                KeyCode::Char('j') => {
                    let cnt = self.events_for(self.selected).len();
                    if cnt > 0 && self.event_cursor + 1 < cnt { self.event_cursor += 1; }
                    true
                }
                KeyCode::Char('k') => {
                    if self.event_cursor > 0 { self.event_cursor -= 1; }
                    true
                }
                // ── Day navigation ─────────────────────────────────────────
                KeyCode::Left  if !shift => { self.move_days(-1);  true }
                KeyCode::Right if !shift => { self.move_days(1);   true }
                KeyCode::Up    if !shift => { self.move_days(-7);  true }
                KeyCode::Down  if !shift => { self.move_days(7);   true }
                // ── Month navigation ───────────────────────────────────────
                KeyCode::Left  if shift => { self.move_months(-1); true }
                KeyCode::Right if shift => { self.move_months(1);  true }
                KeyCode::Char('[') | KeyCode::Char(',') => { self.move_months(-1); true }
                KeyCode::Char(']') | KeyCode::Char('.') => { self.move_months(1);  true }
                // ── Year navigation ────────────────────────────────────────
                KeyCode::PageUp   => { self.move_years(-1); true }
                KeyCode::PageDown => { self.move_years(1);  true }
                // ── Shortcuts ──────────────────────────────────────────────
                KeyCode::Char('t') => { self.go_today();                       true }
                KeyCode::Char('y') => {
                    self.view_mode = ViewMode::Year;
                    self.view_year = self.selected.year();                      true
                }
                KeyCode::Char('g') => {
                    self.selected = NaiveDate::from_ymd_opt(
                        self.selected.year(), self.selected.month(), 1,
                    ).unwrap_or(self.selected);
                    self.event_cursor = 0;
                    true
                }
                KeyCode::Char('G') => {
                    let last = days_in_month(self.selected.year(), self.selected.month());
                    self.selected = NaiveDate::from_ymd_opt(
                        self.selected.year(), self.selected.month(), last,
                    ).unwrap_or(self.selected);
                    self.event_cursor = 0;
                    true
                }
                KeyCode::Esc => false,
                _ => true,
            },

            ViewMode::Year => match key.code {
                KeyCode::Left  => { self.move_months(-1); self.view_year = self.selected.year(); true }
                KeyCode::Right => { self.move_months(1);  self.view_year = self.selected.year(); true }
                KeyCode::Up    => { self.move_months(-3); self.view_year = self.selected.year(); true }
                KeyCode::Down  => { self.move_months(3);  self.view_year = self.selected.year(); true }
                KeyCode::PageUp   => { self.view_year -= 1; self.move_years(-1); true }
                KeyCode::PageDown => { self.view_year += 1; self.move_years(1);  true }
                KeyCode::Enter    => { self.view_mode = ViewMode::Month;          true }
                KeyCode::Char('m') => { self.view_mode = ViewMode::Month;         true }
                KeyCode::Char('t') => { self.go_today();                          true }
                KeyCode::Esc       => false,
                _ => true,
            },
        }
    }

    fn handle_form_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => { self.ui_mode = UiMode::Normal; }

            KeyCode::Tab => {
                self.form_field = match self.form_field {
                    FormField::Title => FormField::Time,
                    FormField::Time  => FormField::Notes,
                    FormField::Notes => FormField::Title,
                };
            }
            KeyCode::BackTab => {
                self.form_field = match self.form_field {
                    FormField::Title => FormField::Notes,
                    FormField::Time  => FormField::Title,
                    FormField::Notes => FormField::Time,
                };
            }

            KeyCode::Enter => {
                // Enter on Notes = save; Enter on other fields = advance
                match self.form_field {
                    FormField::Notes => { self.save_form(); }
                    _ => {
                        self.form_field = match self.form_field {
                            FormField::Title => FormField::Time,
                            FormField::Time  => FormField::Notes,
                            FormField::Notes => FormField::Title,
                        };
                    }
                }
            }

            KeyCode::Char('s') if ctrl => { self.save_form(); }

            KeyCode::Char(c) => {
                match self.form_field {
                    FormField::Title => self.form_title.push(c),
                    FormField::Time  => {
                        if (c.is_ascii_digit() || c == ':') && self.form_time.len() < 5 {
                            self.form_time.push(c);
                        }
                    }
                    FormField::Notes => self.form_notes.push(c),
                }
            }

            KeyCode::Backspace => {
                match self.form_field {
                    FormField::Title => { self.form_title.pop(); }
                    FormField::Time  => { self.form_time.pop(); }
                    FormField::Notes => { self.form_notes.pop(); }
                }
            }

            _ => {}
        }
        true
    }

    fn handle_delete_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => { self.delete_confirmed(); }
            KeyCode::Char('n') | KeyCode::Esc   => {
                self.delete_candidate_id = None;
                self.ui_mode = UiMode::Normal;
            }
            _ => {}
        }
        true
    }

    // ── Navigation helpers ────────────────────────────────────────────────────

    fn move_days(&mut self, delta: i64) {
        if let Some(d) = self.selected.checked_add_signed(Duration::days(delta)) {
            self.selected     = d;
            self.event_cursor = 0;
        }
    }

    fn move_months(&mut self, delta: i32) {
        let mut y = self.selected.year();
        let mut m = self.selected.month() as i32 + delta;
        while m < 1  { m += 12; y -= 1; }
        while m > 12 { m -= 12; y += 1; }
        let cap = days_in_month(y, m as u32);
        let d   = self.selected.day().min(cap);
        if let Some(date) = NaiveDate::from_ymd_opt(y, m as u32, d) {
            self.selected     = date;
            self.event_cursor = 0;
        }
    }

    fn move_years(&mut self, delta: i32) {
        let y   = self.selected.year() + delta;
        let m   = self.selected.month();
        let cap = days_in_month(y, m);
        let d   = self.selected.day().min(cap);
        if let Some(date) = NaiveDate::from_ymd_opt(y, m, d) {
            self.selected     = date;
            self.view_year    = y;
            self.event_cursor = 0;
        }
    }

    fn go_today(&mut self) {
        self.today        = Utc::now().date_naive();
        self.selected     = self.today;
        self.view_year    = self.today.year();
        self.event_cursor = 0;
    }

    // ── Calendar math ─────────────────────────────────────────────────────────

    fn build_full_grid(year: i32, month: u32) -> Vec<[NaiveDate; 7]> {
        let start      = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
        let offset     = start.weekday().num_days_from_sunday() as i64;
        let grid_start = start - Duration::days(offset);
        (0..6)
            .map(|week| {
                let mut row = [grid_start; 7];
                for col in 0..7usize {
                    row[col] = grid_start + Duration::days((week * 7 + col as i64) as i64);
                }
                row
            })
            .collect()
    }

    fn day_of_year(date: NaiveDate) -> u32 {
        let jan1 = NaiveDate::from_ymd_opt(date.year(), 1, 1).unwrap();
        (date.signed_duration_since(jan1).num_days() + 1) as u32
    }

    fn days_in_year(year: i32) -> u32 {
        if NaiveDate::from_ymd_opt(year, 2, 29).is_some() { 366 } else { 365 }
    }

    fn quarter(month: u32) -> u32 { (month + 2) / 3 }

    fn season_icon(month: u32) -> (&'static str, Color) {
        match month {
            3 | 4 | 5   => ("Spring", GREEN),
            6 | 7 | 8   => ("Summer", YELLOW),
            9 | 10 | 11 => ("Autumn", ORANGE),
            _           => ("Winter", CYAN),
        }
    }

    fn relative_label(sel: NaiveDate, today: NaiveDate) -> (String, Color) {
        let diff = sel.signed_duration_since(today).num_days();
        match diff {
             0 => ("Today".into(),              GREEN),
             1 => ("Tomorrow".into(),           CYAN),
            -1 => ("Yesterday".into(),          MUTED),
             d if d > 0 => (format!("+{} days", d),     TEXT),
             d           => (format!("{} days ago", -d), DIM),
        }
    }

    fn month_abbr(m: u32) -> &'static str {
        match m {
            1=>"Jan", 2=>"Feb", 3=>"Mar", 4=>"Apr", 5=>"May", 6=>"Jun",
            7=>"Jul", 8=>"Aug", 9=>"Sep", 10=>"Oct", 11=>"Nov", 12=>"Dec",
            _=>"???",
        }
    }

    fn month_name(m: u32) -> &'static str {
        match m {
            1=>"January",  2=>"February", 3=>"March",    4=>"April",
            5=>"May",      6=>"June",     7=>"July",     8=>"August",
            9=>"September",10=>"October",11=>"November",12=>"December",
            _=>"Unknown",
        }
    }

    fn weekday_name_full(date: NaiveDate) -> &'static str {
        match date.weekday() {
            chrono::Weekday::Mon => "Monday",
            chrono::Weekday::Tue => "Tuesday",
            chrono::Weekday::Wed => "Wednesday",
            chrono::Weekday::Thu => "Thursday",
            chrono::Weekday::Fri => "Friday",
            chrono::Weekday::Sat => "Saturday",
            chrono::Weekday::Sun => "Sunday",
        }
    }

    fn weekday_abbr(date: NaiveDate) -> &'static str {
        match date.weekday() {
            chrono::Weekday::Mon => "Mon",
            chrono::Weekday::Tue => "Tue",
            chrono::Weekday::Wed => "Wed",
            chrono::Weekday::Thu => "Thu",
            chrono::Weekday::Fri => "Fri",
            chrono::Weekday::Sat => "Sat",
            chrono::Weekday::Sun => "Sun",
        }
    }

    fn iso_week(date: NaiveDate) -> u32 { date.iso_week().week() }
}

// ── App trait ─────────────────────────────────────────────────────────────────

impl App for CalendarApp {
    fn id(&self)   -> &str { "calendar" }
    fn name(&self) -> &str { "NeuraCalendar" }

    fn init(&mut self) -> anyhow::Result<()> {
        self.today    = Utc::now().date_naive();
        self.selected = self.today;
        self.view_year = self.today.year();
        self.needs_load = true;
        Ok(())
    }

    fn on_resume(&mut self) {
        self.today = Utc::now().date_naive();
        if self.events.is_empty() { self.needs_load = true; }
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match self.ui_mode {
            UiMode::EventForm     => self.handle_form_key(key),
            UiMode::DeleteConfirm => self.handle_delete_key(key),
            UiMode::Normal        => self.handle_normal_key(key),
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        match self.view_mode {
            ViewMode::Month => {
                self.render_month_view(frame, area);
                match self.ui_mode {
                    UiMode::EventForm     => self.render_event_form(frame, area),
                    UiMode::DeleteConfirm => self.render_delete_confirm(frame, area),
                    UiMode::Normal        => {}
                }
            }
            ViewMode::Year => self.render_year_view(frame, area),
        }
    }

    fn save_state(&self) -> Option<Value> {
        Some(serde_json::json!({
            "selected_year":  self.selected.year(),
            "selected_month": self.selected.month(),
            "selected_day":   self.selected.day(),
            "view_year":      self.view_year,
        }))
    }

    fn load_state(&mut self, state: Value) {
        let y = state.get("selected_year") .and_then(|v| v.as_i64()).map(|v| v as i32).unwrap_or(self.today.year());
        let m = state.get("selected_month").and_then(|v| v.as_u64()).map(|v| v as u32).unwrap_or(self.today.month());
        let d = state.get("selected_day")  .and_then(|v| v.as_u64()).map(|v| v as u32).unwrap_or(self.today.day());
        if let Some(date) = NaiveDate::from_ymd_opt(y, m, d) { self.selected = date; }
        self.view_year = state.get("view_year").and_then(|v| v.as_i64()).map(|v| v as i32).unwrap_or(self.selected.year());
    }

    fn as_any(&self)     -> &dyn Any     { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

// ── Month view ────────────────────────────────────────────────────────────────

impl CalendarApp {
    fn render_month_view(&self, frame: &mut Frame, area: Rect) {
        let outer_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER))
            .style(Style::default().bg(BG));
        let inner = outer_block.inner(area);
        frame.render_widget(outer_block, area);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // month header
                Constraint::Min(10),    // calendar + info panel
                Constraint::Length(1),  // year strip
                Constraint::Length(1),  // footer
            ])
            .split(inner);

        self.render_month_header(frame, rows[0]);

        let info_w: u16 = (inner.width / 3).clamp(24, 32);
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(1), Constraint::Length(info_w)])
            .split(rows[1]);

        self.render_calendar_grid(frame, cols[0]);
        self.render_info_panel(frame, cols[1]);
        self.render_year_strip(frame, rows[2]);
        self.render_month_footer(frame, rows[3]);
    }

    fn render_month_header(&self, frame: &mut Frame, area: Rect) {
        let sel = self.selected;
        let is_cur_month = sel.year() == self.today.year() && sel.month() == self.today.month();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(if is_cur_month { PRIMARY } else { BORDER }))
            .style(Style::default().bg(BG));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let left = Line::from(vec![
            Span::styled(" ◂ ", Style::default().fg(DIM)),
            Span::styled(Self::month_name(sel.month()), Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled(format!("  {}", sel.year()), Style::default().fg(MUTED).add_modifier(Modifier::BOLD)),
            Span::styled("  ▸ ", Style::default().fg(DIM)),
            if is_cur_month {
                Span::styled("  ● current month", Style::default().fg(GREEN))
            } else {
                let diff = (sel.year() - self.today.year()) * 12 + sel.month() as i32 - self.today.month() as i32;
                let label = if diff > 0 { format!("  {} months ahead", diff) } else { format!("  {} months ago", -diff) };
                Span::styled(label, Style::default().fg(DIM))
            },
        ]);

        let today_str = format!(
            "Today: {}  {}  {}  {}",
            Self::weekday_abbr(self.today),
            self.today.day(),
            Self::month_abbr(self.today.month()),
            self.today.year(),
        );
        let label_len = today_str.len() as u16;

        frame.render_widget(Paragraph::new(left), inner);
        if inner.width > label_len {
            let lx      = inner.x + inner.width - label_len;
            let render_w = (inner.x + inner.width).saturating_sub(lx);
            frame.render_widget(
                Paragraph::new(Line::from(
                    Span::styled(today_str, Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
                )),
                Rect { x: lx, y: inner.y, width: render_w, height: 1 },
            );
        }
    }

    fn render_calendar_grid(&self, frame: &mut Frame, area: Rect) {
        let sel   = self.selected;
        let year  = sel.year();
        let month = sel.month();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER))
            .style(Style::default().bg(BG));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.width < 30 { return; }

        let wk_col: u16 = 4;
        let cell_w = ((inner.width.saturating_sub(wk_col)) / 7).max(3);
        let grid_w = wk_col + cell_w * 7;
        let left_pad = (inner.width.saturating_sub(grid_w)) / 2;

        let day_headers = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

        let mut header_spans = vec![
            Span::raw(" ".repeat(left_pad as usize)),
            Span::styled("Wk  ", Style::default().fg(DIM)),
        ];
        for (i, &h) in day_headers.iter().enumerate() {
            let is_weekend = i == 0 || i == 6;
            header_spans.push(Span::styled(
                center_str(h, cell_w as usize),
                Style::default()
                    .fg(if is_weekend { ORANGE } else { PRIMARY })
                    .add_modifier(Modifier::BOLD),
            ));
        }

        let sep = format!("{}{}", " ".repeat(left_pad as usize), "─".repeat(grid_w as usize));
        let grid = Self::build_full_grid(year, month);

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(header_spans));
        lines.push(Line::from(Span::styled(sep.clone(), Style::default().fg(BORDER))));

        for row in &grid {
            let week_num = Self::iso_week(row[0]);
            let mut spans: Vec<Span> = vec![
                Span::raw(" ".repeat(left_pad as usize)),
                Span::styled(format!("{:2}  ", week_num), Style::default().fg(DIM)),
            ];

            for (col_idx, &date) in row.iter().enumerate() {
                let in_month   = date.month() == month && date.year() == year;
                let is_today   = date == self.today;
                let is_sel     = date == self.selected;
                let is_weekend = col_idx == 0 || col_idx == 6;
                let has_events = in_month && self.has_events(date);

                let label = center_str(&format!("{}", date.day()), cell_w as usize);

                let style = if is_today && is_sel {
                    Style::default().fg(BG).bg(GREEN).add_modifier(Modifier::BOLD)
                } else if is_today {
                    Style::default().fg(GREEN).add_modifier(Modifier::BOLD)
                } else if is_sel {
                    Style::default().fg(BG).bg(PRIMARY).add_modifier(Modifier::BOLD)
                } else if !in_month {
                    Style::default().fg(Color::Rgb(50, 55, 75))
                } else if is_weekend {
                    Style::default().fg(ORANGE)
                } else {
                    Style::default().fg(TEXT)
                };

                // Underline days that have events (visible indicator)
                let style = if has_events {
                    style.add_modifier(Modifier::UNDERLINED)
                } else {
                    style
                };

                spans.push(Span::styled(label, style));
            }
            lines.push(Line::from(spans));
        }

        lines.push(Line::from(Span::styled(sep, Style::default().fg(BORDER))));
        let today_line = format!(
            "  Today: {}, {} {}, {}",
            Self::weekday_name_full(self.today),
            Self::month_name(self.today.month()),
            self.today.day(),
            self.today.year(),
        );
        lines.push(Line::from(Span::styled(today_line, Style::default().fg(GREEN))));
        // Legend hint for event indicator
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled("underlined", Style::default().fg(DIM).add_modifier(Modifier::UNDERLINED)),
            Span::styled(" = has events", Style::default().fg(DIM)),
        ]));

        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn render_info_panel(&self, frame: &mut Frame, area: Rect) {
        let sel = self.selected;
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER))
            .style(Style::default().bg(PANEL));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let w   = inner.width as usize;
        let sep = "─".repeat(w);

        let doy                  = Self::day_of_year(sel);
        let days_yr              = Self::days_in_year(sel.year());
        let week_num             = Self::iso_week(sel);
        let quarter              = Self::quarter(sel.month());
        let (season_name, season_clr) = Self::season_icon(sel.month());
        let (rel_label, rel_clr) = Self::relative_label(sel, self.today);
        let dom_last             = days_in_month(sel.year(), sel.month());
        let days_to_month_end    = dom_last - sel.day();
        let year_end             = NaiveDate::from_ymd_opt(sel.year(), 12, 31).unwrap();
        let days_to_year_end     = year_end.signed_duration_since(sel).num_days() as u32;
        let q_end_month          = quarter * 3;
        let q_end                = NaiveDate::from_ymd_opt(sel.year(), q_end_month, days_in_month(sel.year(), q_end_month)).unwrap();
        let days_to_q_end        = q_end.signed_duration_since(sel).num_days().max(0) as u32;

        let mut lines: Vec<Line> = Vec::new();

        // ── Date display ──────────────────────────────────────────────────────
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {}", Self::weekday_name_full(sel).to_uppercase()),
                Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {} {}", sel.day(), Self::month_name(sel.month()).to_uppercase()),
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled(format!(" {}", sel.year()), Style::default().fg(MUTED)),
        ]));

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(sep.clone(), Style::default().fg(BORDER))));
        lines.push(Line::from(""));

        // ── Day metrics ───────────────────────────────────────────────────────
        lines.push(info_row("Day of year", &format!("{:03} / {}", doy, days_yr), w));
        lines.push(info_row("Week number", &format!("W{:02}", week_num), w));
        lines.push(info_row("Quarter",     &format!("Q{}", quarter), w));
        lines.push(Line::from(vec![
            Span::styled(" Season  ", Style::default().fg(DIM)),
            Span::styled(season_name, Style::default().fg(season_clr)),
        ]));

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(sep.clone(), Style::default().fg(BORDER))));
        lines.push(Line::from(""));

        // ── Relative ──────────────────────────────────────────────────────────
        lines.push(Line::from(Span::styled(" Relative", Style::default().fg(DIM))));
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {}", rel_label),
                Style::default().fg(rel_clr).add_modifier(Modifier::BOLD),
            ),
        ]));

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(sep.clone(), Style::default().fg(BORDER))));
        lines.push(Line::from(""));

        // ── Countdowns ────────────────────────────────────────────────────────
        lines.push(info_row("End of month", &format!("{} days", days_to_month_end), w));
        lines.push(info_row("End of year",  &format!("{} days", days_to_year_end), w));
        if days_to_q_end > 0 {
            lines.push(info_row(&format!("End of Q{}", quarter), &format!("{} days", days_to_q_end), w));
        }

        // ── Events section ────────────────────────────────────────────────────
        let events = self.events_for(sel);
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(sep.clone(), Style::default().fg(BORDER))));

        // Header row: "EVENTS  [n]ew  [e]dit  [d]el"
        let has_ev = !events.is_empty();
        lines.push(Line::from(vec![
            Span::styled(" EVENTS ", Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
            Span::styled(" n", Style::default().fg(GREEN)),
            Span::styled("ew", Style::default().fg(DIM)),
            if has_ev {
                Span::styled("  e/d", Style::default().fg(DIM))
            } else {
                Span::raw("")
            },
        ]));

        if events.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(" No events", Style::default().fg(DIM)),
            ]));
            lines.push(Line::from(vec![
                Span::styled(" Press n to add", Style::default().fg(Color::Rgb(60, 70, 100))),
            ]));
        } else {
            for (idx, ev) in events.iter().enumerate() {
                let is_cur = idx == self.event_cursor;
                let bullet = if is_cur { "▶" } else { "·" };
                let time_part = if ev.time.is_empty() {
                    String::new()
                } else {
                    format!(" {}", ev.time)
                };
                let raw_title = format!("{} {}{}", bullet, ev.title, time_part);
                let max_len   = w.saturating_sub(2);
                let display   = truncate_str(&raw_title, max_len);

                let line_style = if is_cur {
                    Style::default().fg(BG).bg(PRIMARY).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(TEXT)
                };

                lines.push(Line::from(vec![
                    Span::raw(" "),
                    Span::styled(display, line_style),
                ]));

                // Show notes snippet for selected event
                if is_cur && !ev.notes.is_empty() {
                    let notes_display = truncate_str(&ev.notes, w.saturating_sub(4));
                    lines.push(Line::from(vec![
                        Span::styled(format!("   {}", notes_display), Style::default().fg(MUTED)),
                    ]));
                }
            }

            if events.len() > 1 {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled(" j↓ k↑ navigate", Style::default().fg(DIM)),
                ]));
            }
        }

        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn render_year_strip(&self, frame: &mut Frame, area: Rect) {
        let sel_month   = self.selected.month();
        let today_month = self.today.month();
        let same_year   = self.selected.year() == self.today.year();

        let mut spans: Vec<Span> = vec![
            Span::styled(format!(" {} ", self.selected.year()), Style::default().fg(DIM)),
            Span::styled("│ ", Style::default().fg(BORDER)),
        ];

        for m in 1u32..=12 {
            let is_sel   = m == sel_month;
            let is_today = same_year && m == today_month;
            let label    = format!(" {} ", Self::month_abbr(m));

            let style = if is_sel && is_today {
                Style::default().fg(BG).bg(GREEN).add_modifier(Modifier::BOLD)
            } else if is_sel {
                Style::default().fg(BG).bg(PRIMARY).add_modifier(Modifier::BOLD)
            } else if is_today {
                Style::default().fg(GREEN)
            } else if same_year && m < today_month {
                Style::default().fg(MUTED)
            } else {
                Style::default().fg(DIM)
            };

            spans.push(Span::styled(label, style));
            if m < 12 { spans.push(Span::raw(" ")); }
        }

        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(BG)),
            area,
        );
    }

    fn render_month_footer(&self, frame: &mut Frame, area: Rect) {
        let spans = vec![
            Span::styled("  ←→", Style::default().fg(PRIMARY)),
            Span::styled(" day  ", Style::default().fg(DIM)),
            Span::styled("↑↓", Style::default().fg(PRIMARY)),
            Span::styled(" week  ", Style::default().fg(DIM)),
            Span::styled("[  ]", Style::default().fg(PRIMARY)),
            Span::styled(" month  ", Style::default().fg(DIM)),
            Span::styled("n", Style::default().fg(GREEN)),
            Span::styled(" new event  ", Style::default().fg(DIM)),
            Span::styled("e", Style::default().fg(CYAN)),
            Span::styled(" edit  ", Style::default().fg(DIM)),
            Span::styled("d", Style::default().fg(RED)),
            Span::styled(" delete  ", Style::default().fg(DIM)),
            Span::styled("j k", Style::default().fg(MUTED)),
            Span::styled(" event list  ", Style::default().fg(DIM)),
            Span::styled("t", Style::default().fg(GREEN)),
            Span::styled(" today  ", Style::default().fg(DIM)),
            Span::styled("y", Style::default().fg(CYAN)),
            Span::styled(" year view  ", Style::default().fg(DIM)),
            Span::styled("Esc", Style::default().fg(RED)),
            Span::styled(" exit", Style::default().fg(DIM)),
        ];
        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(BG)),
            area,
        );
    }

    // ── Event form overlay ────────────────────────────────────────────────────

    fn render_event_form(&self, frame: &mut Frame, outer: Rect) {
        let modal_w = 46u16.min(outer.width.saturating_sub(4));
        let modal_h = 14u16.min(outer.height.saturating_sub(4));
        let modal_x = outer.x + (outer.width.saturating_sub(modal_w)) / 2;
        let modal_y = outer.y + (outer.height.saturating_sub(modal_h)) / 2;
        let modal   = Rect { x: modal_x, y: modal_y, width: modal_w, height: modal_h };

        // Backdrop
        frame.render_widget(Block::default().style(Style::default().bg(PANEL)), modal);

        let title = if self.form_edit_id.is_none() { " + New Event " } else { " ✎ Edit Event " };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(PRIMARY))
            .title(title)
            .title_style(Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD))
            .style(Style::default().bg(PANEL));
        let inner = block.inner(modal);
        frame.render_widget(block, modal);

        if inner.height < 2 { return; }

        let field_w  = inner.width.saturating_sub(2) as usize;
        let date_str = format!(
            " {}  {} {}  {}",
            Self::weekday_abbr(self.selected),
            Self::month_name(self.selected.month()),
            self.selected.day(),
            self.selected.year(),
        );

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(Span::styled(date_str, Style::default().fg(MUTED))));
        lines.push(Line::from(""));

        // Title field
        let tf = self.form_field == FormField::Title;
        let title_val = if self.form_title.is_empty() && !tf {
            "(required)".to_string()
        } else {
            format!("{}{}", self.form_title, if tf { "│" } else { "" })
        };
        lines.push(Line::from(vec![
            Span::styled(" Title ", Style::default().fg(if tf { CYAN } else { DIM })),
            if tf { Span::styled(" required", Style::default().fg(DIM)) } else { Span::raw("") },
        ]));
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                truncate_str(&title_val, field_w),
                Style::default()
                    .fg(if tf { TEXT } else { DIM })
                    .bg(if tf { SEL_BG } else { PANEL })
                    .add_modifier(if tf { Modifier::BOLD } else { Modifier::empty() }),
            ),
        ]));
        lines.push(Line::from(""));

        // Time field
        let tf2 = self.form_field == FormField::Time;
        let time_val = if self.form_time.is_empty() && !tf2 {
            "(optional  HH:MM)".to_string()
        } else {
            format!("{}{}", self.form_time, if tf2 { "│" } else { "" })
        };
        lines.push(Line::from(vec![
            Span::styled(" Time  ", Style::default().fg(if tf2 { CYAN } else { DIM })),
            Span::styled("HH:MM, optional", Style::default().fg(DIM)),
        ]));
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                truncate_str(&time_val, field_w),
                Style::default()
                    .fg(if tf2 { TEXT } else { DIM })
                    .bg(if tf2 { SEL_BG } else { PANEL }),
            ),
        ]));
        lines.push(Line::from(""));

        // Notes field
        let tf3 = self.form_field == FormField::Notes;
        let notes_val = if self.form_notes.is_empty() && !tf3 {
            "(optional)".to_string()
        } else {
            format!("{}{}", self.form_notes, if tf3 { "│" } else { "" })
        };
        lines.push(Line::from(vec![
            Span::styled(" Notes ", Style::default().fg(if tf3 { CYAN } else { DIM })),
            Span::styled("optional", Style::default().fg(DIM)),
        ]));
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                truncate_str(&notes_val, field_w),
                Style::default()
                    .fg(if tf3 { TEXT } else { DIM })
                    .bg(if tf3 { SEL_BG } else { PANEL }),
            ),
        ]));
        lines.push(Line::from(""));

        // Footer hints
        lines.push(Line::from(vec![
            Span::styled(" Tab", Style::default().fg(PRIMARY)),
            Span::styled(" next  ", Style::default().fg(DIM)),
            Span::styled("Ctrl+S", Style::default().fg(GREEN)),
            Span::styled(" save  ", Style::default().fg(DIM)),
            Span::styled("Esc", Style::default().fg(RED)),
            Span::styled(" cancel", Style::default().fg(DIM)),
        ]));

        frame.render_widget(Paragraph::new(lines), inner);
    }

    // ── Delete confirm overlay ────────────────────────────────────────────────

    fn render_delete_confirm(&self, frame: &mut Frame, outer: Rect) {
        let events = self.events_for(self.selected);
        let ev_title = if let Some(del_id) = self.delete_candidate_id {
            events.iter().find(|e| e.id == del_id).map(|e| e.title.as_str()).unwrap_or("event")
        } else {
            "event"
        };

        let modal_w = 40u16.min(outer.width.saturating_sub(4));
        let modal_h = 7u16;
        let modal_x = outer.x + (outer.width.saturating_sub(modal_w)) / 2;
        let modal_y = outer.y + (outer.height.saturating_sub(modal_h)) / 2;
        let modal   = Rect { x: modal_x, y: modal_y, width: modal_w, height: modal_h };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(RED))
            .title(" Delete Event ")
            .title_style(Style::default().fg(RED).add_modifier(Modifier::BOLD))
            .style(Style::default().bg(PANEL));
        let inner = block.inner(modal);
        frame.render_widget(block, modal);

        let w       = inner.width as usize;
        let display = truncate_str(ev_title, w.saturating_sub(12));
        let lines   = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    format!(" Delete \"{}\"?", display),
                    Style::default().fg(TEXT),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled(" y", Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
                Span::styled(" yes   ", Style::default().fg(DIM)),
                Span::styled("n", Style::default().fg(RED).add_modifier(Modifier::BOLD)),
                Span::styled(" / ", Style::default().fg(DIM)),
                Span::styled("Esc", Style::default().fg(RED)),
                Span::styled(" cancel", Style::default().fg(DIM)),
            ]),
        ];
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

// ── Year view ─────────────────────────────────────────────────────────────────

impl CalendarApp {
    fn render_year_view(&self, frame: &mut Frame, area: Rect) {
        let outer = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER))
            .title(format!(" NeuraCalendar  ·  {} ", self.view_year))
            .title_style(Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD))
            .style(Style::default().bg(BG));
        let inner = outer.inner(area);
        frame.render_widget(outer, area);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),  // year nav
                Constraint::Min(10),    // mini cals
                Constraint::Length(1),  // footer
            ])
            .split(inner);

        let nav_line = Line::from(vec![
            Span::styled("  ◂ ", Style::default().fg(DIM)),
            Span::styled(format!("{}", self.view_year - 1), Style::default().fg(DIM)),
            Span::styled("     ", Style::default()),
            Span::styled(format!("{}", self.view_year), Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled("     ", Style::default()),
            Span::styled(format!("{}", self.view_year + 1), Style::default().fg(DIM)),
            Span::styled(" ▸", Style::default().fg(DIM)),
            Span::styled(
                format!("   ·  Selected: {}, {} {}",
                    Self::weekday_abbr(self.selected),
                    Self::month_name(self.selected.month()),
                    self.selected.year()),
                Style::default().fg(MUTED),
            ),
        ]);
        frame.render_widget(Paragraph::new(nav_line), rows[0]);

        let mini_grid_cols = 3usize;
        let mini_grid_rows = 4usize;
        let col_w = rows[1].width / mini_grid_cols as u16;
        let row_h = rows[1].height / mini_grid_rows as u16;

        for row_i in 0..mini_grid_rows {
            for col_i in 0..mini_grid_cols {
                let month_idx = (row_i * mini_grid_cols + col_i + 1) as u32;
                if month_idx > 12 { break; }
                let mini_area = Rect {
                    x: rows[1].x + col_i as u16 * col_w,
                    y: rows[1].y + row_i as u16 * row_h,
                    width:  col_w,
                    height: row_h,
                };
                self.render_mini_month(frame, mini_area, self.view_year, month_idx);
            }
        }

        let foot_spans = vec![
            Span::styled("  ←→", Style::default().fg(PRIMARY)),
            Span::styled(" month  ", Style::default().fg(DIM)),
            Span::styled("↑↓", Style::default().fg(PRIMARY)),
            Span::styled(" quarter  ", Style::default().fg(DIM)),
            Span::styled("PgUp PgDn", Style::default().fg(PRIMARY)),
            Span::styled(" year  ", Style::default().fg(DIM)),
            Span::styled("Enter", Style::default().fg(GREEN)),
            Span::styled(" or ", Style::default().fg(DIM)),
            Span::styled("m", Style::default().fg(GREEN)),
            Span::styled(" month view  ", Style::default().fg(DIM)),
            Span::styled("t", Style::default().fg(CYAN)),
            Span::styled(" today  ", Style::default().fg(DIM)),
            Span::styled("Esc", Style::default().fg(RED)),
            Span::styled(" exit", Style::default().fg(DIM)),
        ];
        frame.render_widget(Paragraph::new(Line::from(foot_spans)), rows[2]);
    }

    fn render_mini_month(&self, frame: &mut Frame, area: Rect, year: i32, month: u32) {
        let is_current_m  = year == self.today.year()    && month == self.today.month();
        let is_selected_m = year == self.selected.year() && month == self.selected.month();

        let border_clr = if is_selected_m { PRIMARY } else if is_current_m { GREEN } else { BORDER };
        let title_clr  = if is_selected_m { PRIMARY } else if is_current_m { GREEN } else { MUTED };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_clr))
            .title(format!(" {} ", Self::month_name(month)))
            .title_style(Style::default().fg(title_clr).add_modifier(Modifier::BOLD))
            .style(Style::default().bg(if is_selected_m { PANEL } else { BG }));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.width < 14 || inner.height < 3 { return; }

        let header_line = Line::from(vec![
            Span::styled("Su ", Style::default().fg(ORANGE)),
            Span::styled("Mo Tu We Th Fr ", Style::default().fg(MUTED)),
            Span::styled("Sa", Style::default().fg(ORANGE)),
        ]);

        let grid = Self::build_full_grid(year, month);
        let mut lines = vec![header_line];

        for row in &grid {
            let mut spans: Vec<Span> = Vec::new();
            for (col_i, &date) in row.iter().enumerate() {
                let in_month   = date.month() == month && date.year() == year;
                let is_today   = date == self.today;
                let is_sel     = date == self.selected;
                let is_weekend = col_i == 0 || col_i == 6;
                let has_events = in_month && self.has_events(date);

                let label    = format!("{:2}", date.day());
                let trailing = if col_i < 6 { " " } else { "" };
                let full     = format!("{}{}", label, trailing);

                let style = if is_today && is_sel {
                    Style::default().fg(BG).bg(GREEN).add_modifier(Modifier::BOLD)
                } else if is_today {
                    Style::default().fg(GREEN).add_modifier(Modifier::BOLD)
                } else if is_sel {
                    Style::default().fg(BG).bg(PRIMARY).add_modifier(Modifier::BOLD)
                } else if !in_month {
                    Style::default().fg(Color::Rgb(45, 50, 70))
                } else if is_weekend {
                    Style::default().fg(ORANGE)
                } else {
                    Style::default().fg(TEXT)
                };

                let style = if has_events {
                    style.add_modifier(Modifier::UNDERLINED)
                } else {
                    style
                };

                spans.push(Span::styled(full, style));
            }
            lines.push(Line::from(spans));
        }

        frame.render_widget(Paragraph::new(lines), inner);
    }
}

// ── Utilities ─────────────────────────────────────────────────────────────────

fn days_in_month(year: i32, month: u32) -> u32 {
    let (ny, nm) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    NaiveDate::from_ymd_opt(ny, nm, 1).unwrap()
        .signed_duration_since(NaiveDate::from_ymd_opt(year, month, 1).unwrap())
        .num_days() as u32
}

fn center_str(s: &str, width: usize) -> String {
    if s.len() >= width { return s[..width].to_string(); }
    let total_pad = width - s.len();
    let left  = total_pad / 2;
    let right = total_pad - left;
    format!("{}{}{}", " ".repeat(left), s, " ".repeat(right))
}

fn truncate_str(s: &str, max: usize) -> String {
    if max == 0 { return String::new(); }
    let char_count = s.chars().count();
    if char_count <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", truncated)
    }
}

fn info_row(label: &str, value: &str, width: usize) -> Line<'static> {
    let label = label.to_string();
    let value = value.to_string();
    let total_len = label.len() + 1 + value.len();
    let pad = if total_len + 2 < width { width - total_len - 2 } else { 1 };
    Line::from(vec![
        Span::styled(format!(" {}", label), Style::default().fg(DIM)),
        Span::styled(" ".repeat(pad), Style::default()),
        Span::styled(value, Style::default().fg(TEXT).add_modifier(Modifier::BOLD)),
    ])
}
