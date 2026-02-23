use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::renderer::{Desktop, DesktopMode, HomeFocus, HomeSection};

/// Result of handling a key event.
pub enum InputAction {
    None,
    ExecuteCommand(String),
    Exit,
    HistoryPrev,
    HistoryNext,
    /// Toggle between HomeScreen and Shell modes.
    ToggleShell,
    /// Pin or unpin the given app ID from the home screen.
    PinToggle(String),
    /// Open a specific app by ID (from home grid Enter press).
    OpenApp(String),
}

pub struct InputHandler;

impl InputHandler {
    /// Process a key event and return what action to take.
    pub fn handle(desktop: &mut Desktop, key: KeyEvent) -> InputAction {
        // ── Global shortcuts (work in every non-app mode) ─────────────────────
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('p') => {
                    if desktop.mode == DesktopMode::CommandPalette {
                        // Close palette and return to the correct base mode
                        desktop.mode = if desktop.palette_from_home {
                            DesktopMode::HomeScreen
                        } else {
                            DesktopMode::Shell
                        };
                    } else {
                        desktop.palette_from_home = matches!(desktop.mode, DesktopMode::HomeScreen);
                        desktop.mode = DesktopMode::CommandPalette;
                        desktop.palette_input.clear();
                        desktop.palette_selected = 0;
                    }
                    return InputAction::None;
                }
                KeyCode::Char('h') => {
                    desktop.show_help = !desktop.show_help;
                    return InputAction::None;
                }
                KeyCode::Char('l') => {
                    desktop.shell_history.clear();
                    desktop.shell_scroll = 0;
                    return InputAction::None;
                }
                KeyCode::Char('c') => {
                    if desktop.mode == DesktopMode::CommandPalette || desktop.show_help {
                        desktop.mode = if desktop.palette_from_home {
                            DesktopMode::HomeScreen
                        } else {
                            DesktopMode::Shell
                        };
                        desktop.show_help = false;
                    } else if desktop.home_focus == HomeFocus::AppGrid {
                        // Return grid focus to console
                        desktop.home_focus = HomeFocus::Console;
                    } else {
                        desktop.shell_input.clear();
                        desktop.shell_cursor = 0;
                    }
                    return InputAction::None;
                }
                KeyCode::Char('d') => {
                    return InputAction::Exit;
                }
                _ => {}
            }
        }

        // F12 — toggle Home Screen / Full Shell
        if key.code == KeyCode::F(12) {
            return InputAction::ToggleShell;
        }

        // ── Command palette mode ──────────────────────────────────────────────
        if desktop.mode == DesktopMode::CommandPalette {
            match key.code {
                KeyCode::Esc => {
                    desktop.mode = if desktop.palette_from_home {
                        DesktopMode::HomeScreen
                    } else {
                        DesktopMode::Shell
                    };
                }
                KeyCode::Char(c) => {
                    desktop.palette_input.push(c);
                    desktop.palette_selected = 0;
                }
                KeyCode::Backspace => {
                    desktop.palette_input.pop();
                    desktop.palette_selected = 0;
                }
                KeyCode::Up => {
                    if desktop.palette_selected > 0 {
                        desktop.palette_selected -= 1;
                    }
                }
                KeyCode::Down => {
                    let count = desktop.filtered_palette_commands().len();
                    if desktop.palette_selected + 1 < count {
                        desktop.palette_selected += 1;
                    }
                }
                KeyCode::Enter => {
                    let filtered = desktop.filtered_palette_commands();
                    if let Some((cmd, _)) = filtered.get(desktop.palette_selected) {
                        let command = cmd.to_string();
                        desktop.mode = if desktop.palette_from_home {
                            DesktopMode::HomeScreen
                        } else {
                            DesktopMode::Shell
                        };
                        desktop.palette_input.clear();
                        desktop.palette_selected = 0;
                        return InputAction::ExecuteCommand(command);
                    }
                }
                _ => {}
            }
            return InputAction::None;
        }

        // ── Escape key ────────────────────────────────────────────────────────
        if key.code == KeyCode::Esc {
            if desktop.show_help {
                desktop.show_help = false;
            } else if desktop.home_focus == HomeFocus::AppGrid {
                // Return focus to console from app grid
                desktop.home_focus = HomeFocus::Console;
            } else if matches!(desktop.mode, DesktopMode::AppView(_)) {
                desktop.mode = if desktop.home_is_base {
                    DesktopMode::HomeScreen
                } else {
                    DesktopMode::Shell
                };
            }
            return InputAction::None;
        }

        // ── Home screen mode ──────────────────────────────────────────────────
        if desktop.mode == DesktopMode::HomeScreen {
            return Self::handle_home_screen(desktop, key);
        }

        // ── Shell mode fallback (scroll + input) ──────────────────────────────
        Self::handle_shell_input(desktop, key)
    }

    /// Handle input specifically when the desktop is in HomeScreen mode.
    fn handle_home_screen(desktop: &mut Desktop, key: KeyEvent) -> InputAction {
        // Page scroll still works in home screen (for the console/shell history)
        match key.code {
            KeyCode::PageUp => {
                desktop.clamp_shell_scroll();
                desktop.shell_scroll = desktop.shell_scroll.saturating_sub(10);
                return InputAction::None;
            }
            KeyCode::PageDown => {
                desktop.shell_scroll = desktop.shell_scroll.saturating_add(10);
                desktop.clamp_shell_scroll();
                return InputAction::None;
            }
            _ => {}
        }

        match desktop.home_focus {
            HomeFocus::AppGrid => Self::handle_app_grid(desktop, key),
            HomeFocus::Console => Self::handle_home_console(desktop, key),
        }
    }

    /// Navigate the app grid on the home screen.
    fn handle_app_grid(desktop: &mut Desktop, key: KeyEvent) -> InputAction {
        // Esc → back to console (also handled globally but kept here for clarity)
        if key.code == KeyCode::Esc {
            desktop.home_focus = HomeFocus::Console;
            return InputAction::None;
        }

        let max_idx = match desktop.home_section {
            HomeSection::Pinned  => desktop.pinned_apps.len().saturating_sub(1),
            HomeSection::AllApps => desktop.all_apps_list.len().saturating_sub(1),
        };

        match key.code {
            // Navigate within current section
            KeyCode::Left => {
                if desktop.home_app_idx > 0 {
                    desktop.home_app_idx -= 1;
                } else {
                    desktop.home_app_idx = max_idx;
                }
            }
            KeyCode::Right | KeyCode::Char(' ') => {
                if desktop.home_app_idx < max_idx {
                    desktop.home_app_idx += 1;
                } else {
                    desktop.home_app_idx = 0;
                }
            }
            // Tab: cycle section
            KeyCode::Tab => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    // Shift+Tab: Pinned → Console → AllApps → Pinned
                    match desktop.home_section {
                        HomeSection::Pinned => {
                            desktop.home_focus = HomeFocus::Console;
                        }
                        HomeSection::AllApps => {
                            desktop.home_section = HomeSection::Pinned;
                            desktop.home_app_idx = 0;
                        }
                    }
                } else {
                    // Tab: Pinned → AllApps → Console
                    match desktop.home_section {
                        HomeSection::Pinned => {
                            desktop.home_section = HomeSection::AllApps;
                            desktop.home_app_idx = 0;
                        }
                        HomeSection::AllApps => {
                            desktop.home_focus = HomeFocus::Console;
                        }
                    }
                }
            }
            // Enter: open selected app
            KeyCode::Enter => {
                if let Some(app_id) = desktop.home_selected_app_id() {
                    return InputAction::OpenApp(app_id);
                }
            }
            // 'p': pin / unpin
            KeyCode::Char('p') => {
                if let Some(app_id) = desktop.home_selected_app_id() {
                    return InputAction::PinToggle(app_id);
                }
            }
            // Number keys 1-9: quick-open pinned apps
            KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
                let idx = (c as usize) - ('1' as usize);
                if let Some(app_id) = desktop.pinned_apps.get(idx).cloned() {
                    return InputAction::OpenApp(app_id);
                }
            }
            // Any other printable char: drop to console and type it
            KeyCode::Char(c) => {
                desktop.home_focus = HomeFocus::Console;
                desktop.shell_input.insert(desktop.shell_cursor, c);
                desktop.shell_cursor += 1;
            }
            // Backspace: drop to console
            KeyCode::Backspace => {
                desktop.home_focus = HomeFocus::Console;
            }
            _ => {}
        }

        InputAction::None
    }

    /// Handle keystrokes in the home screen console bar.
    fn handle_home_console(desktop: &mut Desktop, key: KeyEvent) -> InputAction {
        match key.code {
            // Tab with nothing typed: enter app grid (Pinned section)
            KeyCode::Tab if desktop.shell_input.is_empty() && desktop.suggestions.is_empty() => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    // Shift+Tab: go to AllApps section directly
                    if !desktop.all_apps_list.is_empty() {
                        desktop.home_focus = HomeFocus::AppGrid;
                        desktop.home_section = HomeSection::AllApps;
                        desktop.home_app_idx = 0;
                    }
                } else {
                    // Tab: go to Pinned section
                    if !desktop.pinned_apps.is_empty() {
                        desktop.home_focus = HomeFocus::AppGrid;
                        desktop.home_section = HomeSection::Pinned;
                        desktop.home_app_idx = 0;
                    }
                }
                return InputAction::None;
            }
            // Tab with suggestions: accept suggestion
            KeyCode::Tab => {
                if !desktop.suggestions.is_empty() {
                    let idx = desktop.suggestion_selected.min(desktop.suggestions.len() - 1);
                    desktop.shell_input = desktop.suggestions[idx].clone();
                    desktop.shell_cursor = desktop.shell_input.len();
                    desktop.suggestions.clear();
                    desktop.suggestion_selected = 0;
                }
                return InputAction::None;
            }
            // Enter: execute command
            KeyCode::Enter => {
                let input = desktop.shell_input.clone();
                if !input.is_empty() {
                    desktop.shell_input.clear();
                    desktop.shell_cursor = 0;
                    desktop.suggestions.clear();
                    desktop.suggestion_selected = 0;
                    return InputAction::ExecuteCommand(input);
                }
            }
            // Number keys 1-9 with empty input: quick open pinned apps
            KeyCode::Char(c)
                if c.is_ascii_digit() && c != '0' && desktop.shell_input.is_empty() =>
            {
                let idx = (c as usize) - ('1' as usize);
                if let Some(app_id) = desktop.pinned_apps.get(idx).cloned() {
                    return InputAction::OpenApp(app_id);
                }
                // Not a valid pinned app index, fall through to type the char
                desktop.shell_input.insert(desktop.shell_cursor, c);
                desktop.shell_cursor += 1;
            }
            KeyCode::Char(c) => {
                desktop.shell_input.insert(desktop.shell_cursor, c);
                desktop.shell_cursor += 1;
            }
            KeyCode::Backspace => {
                if desktop.shell_cursor > 0 {
                    desktop.shell_cursor -= 1;
                    desktop.shell_input.remove(desktop.shell_cursor);
                }
            }
            KeyCode::Delete => {
                if desktop.shell_cursor < desktop.shell_input.len() {
                    desktop.shell_input.remove(desktop.shell_cursor);
                }
            }
            KeyCode::Left => {
                if desktop.shell_cursor > 0 { desktop.shell_cursor -= 1; }
            }
            KeyCode::Right => {
                if desktop.shell_cursor < desktop.shell_input.len() { desktop.shell_cursor += 1; }
            }
            KeyCode::Up => {
                if !desktop.suggestions.is_empty() {
                    if desktop.suggestion_selected == 0 {
                        desktop.suggestion_selected = desktop.suggestions.len() - 1;
                    } else {
                        desktop.suggestion_selected -= 1;
                    }
                } else {
                    return InputAction::HistoryPrev;
                }
            }
            KeyCode::Down => {
                if !desktop.suggestions.is_empty() {
                    desktop.suggestion_selected =
                        (desktop.suggestion_selected + 1) % desktop.suggestions.len();
                } else {
                    return InputAction::HistoryNext;
                }
            }
            KeyCode::Home => { desktop.shell_cursor = 0; }
            KeyCode::End  => { desktop.shell_cursor = desktop.shell_input.len(); }
            _ => {}
        }

        InputAction::None
    }

    /// Handle input in full shell mode.
    fn handle_shell_input(desktop: &mut Desktop, key: KeyEvent) -> InputAction {
        // Page scroll
        match key.code {
            KeyCode::PageUp => {
                desktop.clamp_shell_scroll();
                desktop.shell_scroll = desktop.shell_scroll.saturating_sub(10);
                return InputAction::None;
            }
            KeyCode::PageDown => {
                desktop.shell_scroll = desktop.shell_scroll.saturating_add(10);
                desktop.clamp_shell_scroll();
                return InputAction::None;
            }
            _ => {}
        }

        match key.code {
            KeyCode::Enter => {
                let input = desktop.shell_input.clone();
                if !input.is_empty() {
                    desktop.shell_input.clear();
                    desktop.shell_cursor = 0;
                    desktop.suggestions.clear();
                    desktop.suggestion_selected = 0;
                    return InputAction::ExecuteCommand(input);
                }
            }
            KeyCode::Char(c) => {
                desktop.shell_input.insert(desktop.shell_cursor, c);
                desktop.shell_cursor += 1;
            }
            KeyCode::Backspace => {
                if desktop.shell_cursor > 0 {
                    desktop.shell_cursor -= 1;
                    desktop.shell_input.remove(desktop.shell_cursor);
                }
            }
            KeyCode::Delete => {
                if desktop.shell_cursor < desktop.shell_input.len() {
                    desktop.shell_input.remove(desktop.shell_cursor);
                }
            }
            KeyCode::Left => {
                if desktop.shell_cursor > 0 { desktop.shell_cursor -= 1; }
            }
            KeyCode::Right => {
                if desktop.shell_cursor < desktop.shell_input.len() { desktop.shell_cursor += 1; }
            }
            KeyCode::Up => {
                if !desktop.suggestions.is_empty() {
                    if desktop.suggestion_selected == 0 {
                        desktop.suggestion_selected = desktop.suggestions.len() - 1;
                    } else {
                        desktop.suggestion_selected -= 1;
                    }
                } else {
                    return InputAction::HistoryPrev;
                }
            }
            KeyCode::Down => {
                if !desktop.suggestions.is_empty() {
                    desktop.suggestion_selected =
                        (desktop.suggestion_selected + 1) % desktop.suggestions.len();
                } else {
                    return InputAction::HistoryNext;
                }
            }
            KeyCode::Home => { desktop.shell_cursor = 0; }
            KeyCode::End  => { desktop.shell_cursor = desktop.shell_input.len(); }
            KeyCode::Tab  => {
                if !desktop.suggestions.is_empty() {
                    let idx = desktop.suggestion_selected.min(desktop.suggestions.len() - 1);
                    desktop.shell_input = desktop.suggestions[idx].clone();
                    desktop.shell_cursor = desktop.shell_input.len();
                    desktop.suggestions.clear();
                    desktop.suggestion_selected = 0;
                }
            }
            _ => {}
        }

        InputAction::None
    }
}
