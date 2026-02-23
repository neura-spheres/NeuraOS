use std::any::Any;
use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui::style::{Style, Modifier};
use crossterm::event::{KeyCode, KeyEvent};
use serde_json::Value;
use neura_app_framework::app_trait::App;
use neura_app_framework::palette::*;

#[derive(Debug, Clone)]
struct HistoryEntry {
    expression: String,
    result: String,
}

pub struct CalcApp {
    input: String,
    history: Vec<HistoryEntry>,
    error: Option<String>,
    initialized: bool,
}

impl CalcApp {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            history: Vec::new(),
            error: None,
            initialized: false,
        }
    }

    fn evaluate(&self, expr: &str) -> Result<f64, String> {
        let tokens = Self::tokenize(expr)?;
        let mut pos = 0;
        let result = Self::parse_expr(&tokens, &mut pos)?;
        if pos < tokens.len() {
            return Err(format!("Unexpected token: {:?}", tokens[pos]));
        }
        Ok(result)
    }

    fn tokenize(expr: &str) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();
        let chars: Vec<char> = expr.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            match chars[i] {
                ' ' | '\t' => { i += 1; }
                '+' => { tokens.push(Token::Plus); i += 1; }
                '-' => { tokens.push(Token::Minus); i += 1; }
                '*' => { tokens.push(Token::Star); i += 1; }
                '/' => { tokens.push(Token::Slash); i += 1; }
                '(' => { tokens.push(Token::LParen); i += 1; }
                ')' => { tokens.push(Token::RParen); i += 1; }
                c if c.is_ascii_digit() || c == '.' => {
                    let start = i;
                    let mut has_dot = c == '.';
                    i += 1;
                    while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                        if chars[i] == '.' {
                            if has_dot {
                                return Err("Invalid number: multiple decimal points".to_string());
                            }
                            has_dot = true;
                        }
                        i += 1;
                    }
                    let num_str: String = chars[start..i].iter().collect();
                    let num = num_str.parse::<f64>()
                        .map_err(|_| format!("Invalid number: {}", num_str))?;
                    tokens.push(Token::Number(num));
                }
                c => {
                    return Err(format!("Unexpected character: '{}'", c));
                }
            }
        }

        Ok(tokens)
    }

    fn parse_expr(tokens: &[Token], pos: &mut usize) -> Result<f64, String> {
        let mut left = Self::parse_term(tokens, pos)?;

        while *pos < tokens.len() {
            match tokens[*pos] {
                Token::Plus => {
                    *pos += 1;
                    let right = Self::parse_term(tokens, pos)?;
                    left += right;
                }
                Token::Minus => {
                    *pos += 1;
                    let right = Self::parse_term(tokens, pos)?;
                    left -= right;
                }
                _ => break,
            }
        }

        Ok(left)
    }

    fn parse_term(tokens: &[Token], pos: &mut usize) -> Result<f64, String> {
        let mut left = Self::parse_unary(tokens, pos)?;

        while *pos < tokens.len() {
            match tokens[*pos] {
                Token::Star => {
                    *pos += 1;
                    let right = Self::parse_unary(tokens, pos)?;
                    left *= right;
                }
                Token::Slash => {
                    *pos += 1;
                    let right = Self::parse_unary(tokens, pos)?;
                    if right == 0.0 {
                        return Err("Division by zero".to_string());
                    }
                    left /= right;
                }
                _ => break,
            }
        }

        Ok(left)
    }

    fn parse_unary(tokens: &[Token], pos: &mut usize) -> Result<f64, String> {
        if *pos < tokens.len() {
            match tokens[*pos] {
                Token::Minus => {
                    *pos += 1;
                    let val = Self::parse_unary(tokens, pos)?;
                    return Ok(-val);
                }
                Token::Plus => {
                    *pos += 1;
                    return Self::parse_unary(tokens, pos);
                }
                _ => {}
            }
        }
        Self::parse_primary(tokens, pos)
    }

    fn parse_primary(tokens: &[Token], pos: &mut usize) -> Result<f64, String> {
        if *pos >= tokens.len() {
            return Err("Unexpected end of expression".to_string());
        }

        match tokens[*pos] {
            Token::Number(n) => {
                *pos += 1;
                Ok(n)
            }
            Token::LParen => {
                *pos += 1;
                let result = Self::parse_expr(tokens, pos)?;
                if *pos >= tokens.len() {
                    return Err("Missing closing parenthesis".to_string());
                }
                match tokens[*pos] {
                    Token::RParen => {
                        *pos += 1;
                        Ok(result)
                    }
                    _ => Err("Expected closing parenthesis".to_string()),
                }
            }
            ref t => Err(format!("Unexpected token: {:?}", t)),
        }
    }

    fn do_evaluate(&mut self) {
        let expr = self.input.trim().to_string();
        if expr.is_empty() {
            return;
        }

        match self.evaluate(&expr) {
            Ok(result) => {
                let result_str = Self::format_result(result);
                self.history.push(HistoryEntry {
                    expression: expr,
                    result: result_str,
                });
                self.input.clear();
                self.error = None;
            }
            Err(e) => {
                self.error = Some(e);
            }
        }
    }

    fn format_result(value: f64) -> String {
        if value.fract() == 0.0 && value.abs() < 1e15 {
            format!("{}", value as i64)
        } else {
            // Limit decimal places for cleaner display
            let formatted = format!("{:.10}", value);
            let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
            trimmed.to_string()
        }
    }
}

#[derive(Debug, Clone)]
enum Token {
    Number(f64),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
}

impl App for CalcApp {
    fn id(&self) -> &str { "calc" }
    fn name(&self) -> &str { "NeuraCalc" }

    fn init(&mut self) -> anyhow::Result<()> {
        self.initialized = true;
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char(c) if "0123456789.+-*/()".contains(c) => {
                self.input.push(c);
                self.error = None;
                true
            }
            KeyCode::Char(' ') => {
                self.input.push(' ');
                true
            }
            KeyCode::Char('c') => {
                self.input.clear();
                self.error = None;
                true
            }
            KeyCode::Enter => {
                self.do_evaluate();
                true
            }
            KeyCode::Backspace => {
                self.input.pop();
                self.error = None;
                true
            }
            KeyCode::Esc => false,
            _ => true,
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(3),
                Constraint::Length(3),
                Constraint::Length(2),
            ])
            .split(area);

        let history_items: Vec<ListItem> = self.history.iter().map(|entry| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("  {} ", entry.expression),
                    Style::default().fg(TEXT),
                ),
                Span::styled(
                    "= ",
                    Style::default().fg(MUTED),
                ),
                Span::styled(
                    &entry.result,
                    Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
                ),
            ]))
        }).collect();

        let history_list = List::new(history_items)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BORDER))
                .title(" NeuraCalc ")
                .title_style(Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)));
        frame.render_widget(history_list, chunks[0]);

        // Input area
        let input_display = if let Some(ref err) = self.error {
            Line::from(vec![
                Span::styled("  > ", Style::default().fg(PRIMARY)),
                Span::styled(&self.input, Style::default().fg(TEXT)),
                Span::styled(format!("  [{}]", err), Style::default().fg(RED)),
            ])
        } else {
            Line::from(vec![
                Span::styled("  > ", Style::default().fg(PRIMARY)),
                Span::styled(&self.input, Style::default().fg(TEXT)),
                Span::styled("_", Style::default().fg(PRIMARY)),
            ])
        };

        let input_box = Paragraph::new(input_display)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(PRIMARY))
                .title(" Expression ")
                .title_style(Style::default().fg(ORANGE)));
        frame.render_widget(input_box, chunks[1]);

        // Help bar
        let help = Paragraph::new(" [0-9 . + - * / ( )] input  [Enter] evaluate  [c] clear  [Bksp] delete  [Esc] exit")
            .style(Style::default().fg(MUTED));
        frame.render_widget(help, chunks[2]);
    }

    fn on_close(&mut self) {}

    fn save_state(&self) -> Option<Value> {
        let entries: Vec<Value> = self.history.iter().map(|h| {
            serde_json::json!({
                "expression": h.expression,
                "result": h.result,
            })
        }).collect();
        Some(serde_json::json!({ "history": entries }))
    }

    fn load_state(&mut self, state: Value) {
        if let Some(history) = state.get("history").and_then(|v| v.as_array()) {
            self.history = history.iter().filter_map(|entry| {
                let expression = entry.get("expression")?.as_str()?.to_string();
                let result = entry.get("result")?.as_str()?.to_string();
                Some(HistoryEntry { expression, result })
            }).collect();
        }
    }

    fn ai_tools(&self) -> Vec<Value> {
        vec![serde_json::json!({
            "name": "calculate",
            "description": "Evaluate a mathematical expression",
            "parameters": {
                "type": "object",
                "properties": {
                    "expression": { "type": "string", "description": "Math expression to evaluate (supports +, -, *, /, parentheses, decimals)" }
                },
                "required": ["expression"]
            }
        })]
    }

    fn handle_ai_tool(&mut self, tool_name: &str, args: Value) -> Option<Value> {
        match tool_name {
            "calculate" => {
                let expr = args.get("expression")?.as_str()?.to_string();
                match self.evaluate(&expr) {
                    Ok(result) => {
                        let result_str = Self::format_result(result);
                        self.history.push(HistoryEntry {
                            expression: expr.clone(),
                            result: result_str.clone(),
                        });
                        Some(serde_json::json!({
                            "expression": expr,
                            "result": result_str,
                        }))
                    }
                    Err(e) => {
                        Some(serde_json::json!({
                            "expression": expr,
                            "error": e,
                        }))
                    }
                }
            }
            _ => None,
        }
    }

    fn as_any(&self) -> &dyn Any { self }
    fn as_any_mut(&mut self) -> &mut dyn Any { self }
}
