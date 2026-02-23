//! Tokyo Night — the single colour palette for all NeuraOS UI components.
//!
//! Import everything into a module with:
//!   use neura_app_framework::palette::*;
//!
//! Every app that uses ratatui colours must import from here instead of
//! defining its own local constants. Changing a colour requires one edit
//! in this file and the entire OS updates automatically.

use ratatui::style::Color;

// ── Core surface ────────────────────────────────────────────────────────────

/// Deep background (main terminal / app canvas).
pub const BG: Color = Color::Rgb(26, 27, 38);

/// Raised panel / popup / sidebar background.
pub const PANEL: Color = Color::Rgb(31, 35, 53);

/// Terminal canvas (slightly darker than BG, used by NeuraTerminal).
pub const TERMINAL_BG: Color = Color::Rgb(15, 17, 28);

/// Chat message bubble background.
pub const MSG_BG: Color = Color::Rgb(30, 33, 52);

// ── Text ────────────────────────────────────────────────────────────────────

/// Default foreground / body text.
pub const FG: Color = Color::Rgb(192, 202, 245);
/// Alias for FG (used by most apps as `TEXT`).
pub const TEXT: Color = Color::Rgb(192, 202, 245);

/// Muted secondary text (slightly brighter than DIM).
pub const MUTED: Color = Color::Rgb(169, 177, 214);

/// Statusbar secondary text (slightly cooler/darker than MUTED).
pub const STATUSBAR_MUTED: Color = Color::Rgb(130, 140, 170);

/// Very dim / inactive / comment / disabled text.
pub const DIM: Color = Color::Rgb(100, 100, 120);

// ── Accent / interactive ────────────────────────────────────────────────────

/// Primary accent — blue, used for focused borders, links, tabs.
pub const PRIMARY: Color = Color::Rgb(122, 162, 247);
/// Alias for PRIMARY (used by files / browser apps).
pub const ACCENT: Color = Color::Rgb(122, 162, 247);
/// Directory entry colour (same as PRIMARY).
pub const DIR_C: Color = Color::Rgb(122, 162, 247);

/// Cyan / info highlight.
pub const CYAN: Color = Color::Rgb(125, 207, 255);
/// Alias for CYAN.
pub const INFO: Color = Color::Rgb(125, 207, 255);

/// Purple / AI / special actions.
pub const MAGENTA: Color = Color::Rgb(187, 154, 247);
/// Alias for MAGENTA.
pub const PURPLE: Color = Color::Rgb(187, 154, 247);
/// Tool-call indicator in chat (alias for MAGENTA).
pub const TOOL_CLR: Color = Color::Rgb(187, 154, 247);

// ── Semantic ────────────────────────────────────────────────────────────────

/// Success / positive / confirmed — green.
pub const GREEN: Color = Color::Rgb(158, 206, 106);
/// Alias for GREEN (files / settings apps).
pub const OK: Color = Color::Rgb(158, 206, 106);
/// Alias for GREEN.
pub const SUCCESS: Color = Color::Rgb(158, 206, 106);
/// Shell prompt colour (alias for GREEN).
pub const PROMPT: Color = Color::Rgb(158, 206, 106);

/// Warning / in-progress — orange / amber.
pub const ORANGE: Color = Color::Rgb(224, 175, 104);
/// Alias for ORANGE (files / settings apps).
pub const WARN: Color = Color::Rgb(224, 175, 104);
/// Alias for ORANGE.
pub const WARNING: Color = Color::Rgb(224, 175, 104);
/// Yellow (weather app alias, same value as ORANGE).
pub const YELLOW: Color = Color::Rgb(224, 175, 104);

/// Error / danger — red / rose.
pub const RED: Color = Color::Rgb(247, 118, 142);
/// Alias for RED (files / settings apps).
pub const ERR: Color = Color::Rgb(247, 118, 142);
/// Alias for RED.
pub const ERROR: Color = Color::Rgb(247, 118, 142);

// ── Borders ─────────────────────────────────────────────────────────────────

/// Unfocused / idle border.
pub const BORDER: Color = Color::Rgb(59, 66, 97);
/// Focused border (same as PRIMARY).
pub const BORDER_FOCUSED: Color = Color::Rgb(122, 162, 247);

// ── Selection / cursor ──────────────────────────────────────────────────────

/// Selected-row background (most apps).
pub const SEL_BG: Color = Color::Rgb(44, 50, 75);
/// Selected-row background variant (files / auth screens).
pub const SEL_BG2: Color = Color::Rgb(40, 44, 65);
/// Selected-row foreground (same as PRIMARY).
pub const SEL_FG: Color = Color::Rgb(122, 162, 247);

// ── Editor-specific ─────────────────────────────────────────────────────────

/// Line-number gutter colour in NeuraDev.
pub const LINE_NUM: Color = Color::Rgb(80, 90, 120);

// ── Statusbar ───────────────────────────────────────────────────────────────

/// Statusbar / toolbar background.
pub const STATUSBAR_BG: Color = Color::Rgb(31, 35, 53);
