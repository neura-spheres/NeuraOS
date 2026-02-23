/// Parsed shell command.
#[derive(Debug, Clone)]
pub struct ParsedCommand {
    pub program: String,
    pub args: Vec<String>,
    pub pipe_to: Option<Box<ParsedCommand>>,
    pub background: bool,
    pub redirect_stdout: Option<String>,
    pub redirect_stdin: Option<String>,
}

pub struct ShellParser;

impl ShellParser {
    /// Parse a raw input line into a command chain.
    pub fn parse(input: &str) -> Option<ParsedCommand> {
        let input = input.trim();
        if input.is_empty() {
            return None;
        }

        // Handle pipe chains
        let pipe_parts: Vec<&str> = input.splitn(2, '|').collect();

        let (cmd_str, pipe_to) = if pipe_parts.len() == 2 {
            (pipe_parts[0].trim(), ShellParser::parse(pipe_parts[1].trim()))
        } else {
            (input, None)
        };

        // Check for background execution
        let (cmd_str, background) = if cmd_str.ends_with('&') {
            (cmd_str.trim_end_matches('&').trim(), true)
        } else {
            (cmd_str, false)
        };

        // Handle output redirection
        let (cmd_str, redirect_stdout) = if let Some(pos) = cmd_str.find('>') {
            let file = cmd_str[pos + 1..].trim().to_string();
            (cmd_str[..pos].trim(), Some(file))
        } else {
            (cmd_str, None)
        };

        // Handle input redirection
        let (cmd_str, redirect_stdin) = if let Some(pos) = cmd_str.find('<') {
            let file = cmd_str[pos + 1..].trim().to_string();
            (cmd_str[..pos].trim(), Some(file))
        } else {
            (cmd_str, None)
        };

        // Split into program + args (basic space split, respecting quotes)
        let tokens = tokenize(cmd_str);
        if tokens.is_empty() {
            return None;
        }

        Some(ParsedCommand {
            program: tokens[0].clone(),
            args: tokens[1..].to_vec(),
            pipe_to: pipe_to.map(Box::new),
            background,
            redirect_stdout,
            redirect_stdin,
        })
    }
}

/// Simple tokenizer that respects double-quoted strings.
fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in input.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ' ' if !in_quotes => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}
