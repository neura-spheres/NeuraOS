use std::any::Any;
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui::style::{Style, Modifier};
use crossterm::event::{KeyCode, KeyEvent};
use serde_json::Value;
use neura_app_framework::app_trait::App;

use neura_app_framework::palette::*;

#[derive(Debug, Clone)]
pub struct WeatherData {
    pub location: String,
    pub temp_c: f32,
    pub feels_like: f32,
    pub condition: String,
    pub humidity: u32,
    pub wind_kph: f32,
    pub wind_dir: String,
    pub visibility_km: f32,
    pub uv_index: f32,
    pub forecast: Vec<ForecastDay>,
    pub fetched_at: String,
}

#[derive(Debug, Clone)]
pub struct ForecastDay {
    pub date: String,
    pub max_c: f32,
    pub min_c: f32,
    pub condition: String,
    pub rain_pct: u32,
}

#[derive(Debug, Clone, PartialEq)]
enum State {
    Loading,
    Loaded,
    Error(String),
    InputLocation,
}

pub struct WeatherApp {
    location: String,
    data: Option<WeatherData>,
    state: State,
    scroll: usize,
    location_input: String,
    pending_fetch: bool,
}

impl WeatherApp {
    pub fn new() -> Self {
        Self {
            location: "auto".to_string(),
            data: None,
            state: State::Loading,
            scroll: 0,
            location_input: String::new(),
            pending_fetch: true,
        }
    }

    pub fn needs_fetch(&self) -> bool { self.pending_fetch }

    pub async fn async_fetch(&mut self) {
        self.pending_fetch = false;
        self.state = State::Loading;

        let url = if self.location == "auto" {
            "https://wttr.in/?format=j1".to_string()
        } else {
            format!("https://wttr.in/{}?format=j1", urlencoding(&self.location))
        };

        match fetch_weather(&url).await {
            Ok(data) => {
                self.data = Some(data);
                self.state = State::Loaded;
            }
            Err(e) => {
                self.state = State::Error(e);
            }
        }
    }

    fn condition_icon(condition: &str) -> &'static str {
        let lower = condition.to_lowercase();
        if lower.contains("sun") || lower.contains("clear") { "☀" }
        else if lower.contains("cloud") && lower.contains("partly") { "⛅" }
        else if lower.contains("cloud") || lower.contains("overcast") { "☁" }
        else if lower.contains("rain") || lower.contains("drizzle") { "🌧" }
        else if lower.contains("thunder") || lower.contains("storm") { "⛈" }
        else if lower.contains("snow") || lower.contains("sleet") { "❄" }
        else if lower.contains("fog") || lower.contains("mist") { "🌫" }
        else if lower.contains("wind") { "💨" }
        else { "🌡" }
    }

    fn temp_color(temp_c: f32) -> Color {
        if temp_c <= 0.0 { CYAN }
        else if temp_c <= 15.0 { Color::Rgb(100, 180, 255) }
        else if temp_c <= 25.0 { GREEN }
        else if temp_c <= 35.0 { ORANGE }
        else { Color::Rgb(247, 118, 142) }
    }
}

fn urlencoding(s: &str) -> String {
    s.chars().map(|c| match c {
        ' ' => "+".to_string(),
        c if c.is_alphanumeric() || "-_.~".contains(c) => c.to_string(),
        c => format!("%{:02X}", c as u32),
    }).collect()
}

async fn fetch_weather(url: &str) -> Result<WeatherData, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("NeuraOS/0.1")
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let response = client.get(url)
        .send()
        .await
        .map_err(|e| format!("Network error: {}. Check your internet connection.", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}: weather service unavailable", response.status()));
    }

    let json: Value = response.json().await
        .map_err(|e| format!("Parse error: {}", e))?;

    parse_wttr_json(&json)
}

fn parse_wttr_json(json: &Value) -> Result<WeatherData, String> {
    let current = json.get("current_condition")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .ok_or("Missing current_condition")?;

    let nearest_area = json.get("nearest_area")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first());

    let location = nearest_area
        .and_then(|a| a.get("areaName"))
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.get("value"))
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown location")
        .to_string();

    let country = nearest_area
        .and_then(|a| a.get("country"))
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.get("value"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let full_location = if country.is_empty() { location } else { format!("{}, {}", location, country) };

    let temp_c: f32 = current.get("temp_C")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);

    let feels_like: f32 = current.get("FeelsLikeC")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(temp_c);

    let condition = current.get("weatherDesc")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.get("value"))
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown")
        .to_string();

    let humidity: u32 = current.get("humidity")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let wind_kph: f32 = current.get("windspeedKmph")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);

    let wind_dir = current.get("winddir16Point")
        .and_then(|v| v.as_str())
        .unwrap_or("N")
        .to_string();

    let visibility_km: f32 = current.get("visibility")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);

    let uv_index: f32 = current.get("uvIndex")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);

    // Parse 3-day forecast
    let mut forecast = Vec::new();
    if let Some(weather_arr) = json.get("weather").and_then(|v| v.as_array()) {
        for day in weather_arr {
            let date = day.get("date").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let max_c: f32 = day.get("maxtempC").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0.0);
            let min_c: f32 = day.get("mintempC").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0.0);
            let rain_pct: u32 = day.get("hourly")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|h| h.get("chanceofrain"))
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let cond = day.get("hourly")
                .and_then(|v| v.as_array())
                .and_then(|a| a.get(4)) // midday
                .and_then(|h| h.get("weatherDesc"))
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.get("value"))
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string();
            forecast.push(ForecastDay { date, max_c, min_c, condition: cond, rain_pct });
        }
    }

    let fetched_at = chrono::Utc::now().format("%H:%M UTC").to_string();

    Ok(WeatherData {
        location: full_location,
        temp_c,
        feels_like,
        condition,
        humidity,
        wind_kph,
        wind_dir,
        visibility_km,
        uv_index,
        forecast,
        fetched_at,
    })
}

impl App for WeatherApp {
    fn id(&self) -> &str { "weather" }
    fn name(&self) -> &str { "NeuraWeather" }

    fn init(&mut self) -> anyhow::Result<()> { Ok(()) }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match &self.state {
            State::InputLocation => {
                match key.code {
                    KeyCode::Esc => {
                        self.state = if self.data.is_some() { State::Loaded } else { State::Loading };
                        self.location_input.clear();
                    }
                    KeyCode::Enter => {
                        if !self.location_input.trim().is_empty() {
                            self.location = self.location_input.trim().to_string();
                        } else {
                            self.location = "auto".to_string();
                        }
                        self.location_input.clear();
                        self.pending_fetch = true;
                        self.state = State::Loading;
                    }
                    KeyCode::Char(c) => { self.location_input.push(c); }
                    KeyCode::Backspace => { self.location_input.pop(); }
                    _ => {}
                }
                true
            }
            _ => {
                match key.code {
                    KeyCode::Esc => return false,
                    KeyCode::Char('r') | KeyCode::F(5) => {
                        self.pending_fetch = true;
                        self.state = State::Loading;
                    }
                    KeyCode::Char('l') | KeyCode::Char('s') => {
                        self.state = State::InputLocation;
                        self.location_input.clear();
                    }
                    KeyCode::Up | KeyCode::Char('k') => { self.scroll = self.scroll.saturating_sub(1); }
                    KeyCode::Down | KeyCode::Char('j') => { self.scroll += 1; }
                    _ => {}
                }
                true
            }
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        match &self.state {
            State::Loading => {
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(BORDER))
                    .title(" NeuraWeather ")
                    .title_style(Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD));
                let inner = block.inner(area);
                frame.render_widget(block, area);

                let loading = Paragraph::new("\n\n\n  ⛅ Fetching weather data...\n\n  Please wait, connecting to weather service...")
                    .style(Style::default().fg(CYAN))
                    .alignment(Alignment::Center);
                frame.render_widget(loading, inner);
            }
            State::Error(msg) => {
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(BORDER))
                    .title(" NeuraWeather - Error ")
                    .title_style(Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD));
                let inner = block.inner(area);
                frame.render_widget(block, area);

                let err_text = format!(
                    "\n\n\n  ⚠ Weather data unavailable\n\n  {}\n\n  Press [r] to retry | [s] to change location | [Esc] to go back",
                    msg
                );
                let err = Paragraph::new(err_text)
                    .style(Style::default().fg(ORANGE))
                    .alignment(Alignment::Center);
                frame.render_widget(err, inner);
            }
            State::InputLocation => {
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(PRIMARY))
                    .title(" NeuraWeather - Enter Location ")
                    .title_style(Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD));
                let inner = block.inner(area);
                frame.render_widget(block, area);

                let input_text = format!(
                    "\n\n  Enter city name or leave blank for auto-detect:\n\n  > {}_\n\n  [Enter] confirm | [Esc] cancel",
                    self.location_input
                );
                let input = Paragraph::new(input_text)
                    .style(Style::default().fg(TEXT))
                    .alignment(Alignment::Left);
                frame.render_widget(input, inner);
            }
            State::Loaded => {
                if let Some(data) = &self.data {
                    self.render_weather(frame, area, data);
                }
            }
        }
    }

    fn on_resume(&mut self) {
        if self.data.is_none() {
            self.pending_fetch = true;
        }
    }

    fn on_pause(&mut self) {}
    fn on_close(&mut self) {}

    fn save_state(&self) -> Option<Value> {
        Some(serde_json::json!({ "location": self.location }))
    }

    fn load_state(&mut self, state: Value) {
        if let Some(loc) = state.get("location").and_then(|v| v.as_str()) {
            self.location = loc.to_string();
        }
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}

impl WeatherApp {
    fn render_weather(&self, frame: &mut Frame, area: Rect, data: &WeatherData) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),   // header
                Constraint::Length(9),   // main weather
                Constraint::Length(7),   // forecast
                Constraint::Length(1),   // help
            ])
            .split(area);

        // ── Header ──
        let header_text = format!(
            " {} NeuraWeather  |  {}  |  Updated: {}",
            Self::condition_icon(&data.condition), data.location, data.fetched_at
        );
        let header = Paragraph::new(header_text)
            .style(Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(BORDER)));
        frame.render_widget(header, chunks[0]);

        // ── Current Weather ──
        let temp_color = Self::temp_color(data.temp_c);
        let icon = Self::condition_icon(&data.condition);
        let weather_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .title(" Current Conditions ")
            .title_style(Style::default().fg(PRIMARY));
        let weather_inner = weather_block.inner(chunks[1]);
        frame.render_widget(weather_block, chunks[1]);

        if weather_inner.width > 40 {
            let left_w = weather_inner.width / 2;
            let left_area = Rect { x: weather_inner.x, y: weather_inner.y, width: left_w, height: weather_inner.height };
            let right_area = Rect { x: weather_inner.x + left_w, y: weather_inner.y, width: weather_inner.width - left_w, height: weather_inner.height };

            // Big temperature display
            let temp_text = Text::from(vec![
                Line::from(""),
                Line::from(vec![Span::styled(format!("   {}  ", icon), Style::default().fg(YELLOW))]),
                Line::from(vec![
                    Span::styled(format!("  {}°C  ", data.temp_c), Style::default().fg(temp_color).add_modifier(Modifier::BOLD)),
                ]),
                Line::from(vec![Span::styled(format!("  {}  ", data.condition), Style::default().fg(TEXT))]),
                Line::from(vec![Span::styled(format!("  Feels like {}°C  ", data.feels_like), Style::default().fg(MUTED))]),
            ]);
            frame.render_widget(Paragraph::new(temp_text), left_area);

            // Details
            let details = Text::from(vec![
                Line::from(""),
                Line::from(vec![Span::styled(format!("  💧 Humidity: {}%", data.humidity), Style::default().fg(CYAN))]),
                Line::from(vec![Span::styled(format!("  💨 Wind: {} km/h {}", data.wind_kph, data.wind_dir), Style::default().fg(TEXT))]),
                Line::from(vec![Span::styled(format!("  👁 Visibility: {} km", data.visibility_km), Style::default().fg(TEXT))]),
                Line::from(vec![Span::styled(format!("  ☀ UV Index: {}", data.uv_index), Style::default().fg(ORANGE))]),
            ]);
            frame.render_widget(Paragraph::new(details), right_area);
        }

        // ── Forecast ──
        let forecast_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .title(" 3-Day Forecast ")
            .title_style(Style::default().fg(PRIMARY));
        let forecast_inner = forecast_block.inner(chunks[2]);
        frame.render_widget(forecast_block, chunks[2]);

        if !data.forecast.is_empty() {
            let col_w = forecast_inner.width / data.forecast.len().max(1) as u16;
            for (i, day) in data.forecast.iter().enumerate() {
                let x = forecast_inner.x + i as u16 * col_w;
                let day_area = Rect { x, y: forecast_inner.y, width: col_w.saturating_sub(1), height: forecast_inner.height };
                let max_color = Self::temp_color(day.max_c);
                let min_color = Self::temp_color(day.min_c);
                let icon = Self::condition_icon(&day.condition);
                let day_text = Text::from(vec![
                    Line::from(vec![Span::styled(format!(" {}", day.date), Style::default().fg(MUTED))]),
                    Line::from(vec![Span::styled(format!("   {}", icon), Style::default().fg(YELLOW))]),
                    Line::from(vec![
                        Span::styled(format!(" ↑{}°", day.max_c), Style::default().fg(max_color)),
                        Span::styled(format!(" ↓{}°", day.min_c), Style::default().fg(min_color)),
                    ]),
                    Line::from(vec![Span::styled(format!(" 🌧{}%", day.rain_pct), Style::default().fg(CYAN))]),
                ]);
                frame.render_widget(Paragraph::new(day_text), day_area);
            }
        }

        // ── Help ──
        let help = Paragraph::new("  [r/F5] refresh  [s/l] change location  [Esc] back")
            .style(Style::default().fg(MUTED));
        frame.render_widget(help, chunks[3]);
    }
}
