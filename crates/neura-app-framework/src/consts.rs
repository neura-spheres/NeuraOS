//! OS-wide constants — the single source of truth for every value that would
//! otherwise be copy-pasted across the codebase.
//!
//! Covers: OS identity, shell identity, default runtime values, all app IDs,
//! VFS path builders, timezone table, and timezone helpers.
//!
//! One change here propagates to the entire OS automatically.

use chrono::FixedOffset;

// ── OS identity ─────────────────────────────────────────────────────────────

/// Marketing / display name of the operating system.
pub const OS_NAME: &str = "NeuraOS";

/// OS version — always derived from the workspace `Cargo.toml` version field.
/// Never hardcode a version string; use this constant everywhere.
pub const OS_VERSION: &str = env!("CARGO_PKG_VERSION");

/// One-line tagline used in help text, neofetch, about screens, etc.
pub const OS_TAGLINE: &str = "AI-Native CLI Operating System";

/// Default HTTP `User-Agent` header sent by network components.
pub const USER_AGENT: &str = concat!("NeuraOS/", env!("CARGO_PKG_VERSION"));

// ── Shell identity ──────────────────────────────────────────────────────────

/// Shell executable / binary name.
pub const SHELL_NAME: &str = "neura-sh";

/// Shell version string.
pub const SHELL_VERSION: &str = "0.1";

/// Shell name + version, formatted for display (e.g. in neofetch).
pub const SHELL_DISPLAY: &str = concat!("neura-sh ", "0.1");

// ── System defaults ─────────────────────────────────────────────────────────

/// Default hostname used when no config file is present.
pub const DEFAULT_HOSTNAME: &str = "neuraos";

/// Name of the default UI theme.
pub const DEFAULT_THEME: &str = "tokyo_night";

/// Maximum shell command-history entries retained in memory.
pub const MAX_HISTORY: usize = 1_000;

/// Default AI inference temperature (0.0 – 1.0).
pub const DEFAULT_AI_TEMPERATURE: f32 = 0.7;

/// Default AI max-tokens limit per request.
pub const DEFAULT_AI_MAX_TOKENS: u32 = 4_096;

// ── Timezone ────────────────────────────────────────────────────────────────

/// Default timezone offset from UTC in minutes (0 = UTC).
pub const DEFAULT_TZ_OFFSET_MINS: i32 = 0;

/// Default timezone label string shown in the UI.
pub const DEFAULT_TZ_LABEL: &str = "UTC";

/// All named timezones available in the Clock app and Settings.
///
/// Each entry is `(human-readable label, UTC offset in minutes)`.
/// This table is the single authoritative source — do not duplicate it.
pub const WORLD_ZONES: &[(&str, i32)] = &[
    ("UTC",                       0),
    ("London (GMT/BST)",          0),
    ("Paris (CET)",              60),
    ("Berlin (CET)",             60),
    ("Cairo (EET)",             120),
    ("Moscow (MSK)",            180),
    ("Dubai (GST)",             240),
    ("Karachi (PKT)",           300),
    ("India (IST)",             330),
    ("Dhaka (BST)",             360),
    ("Bangkok (ICT)",           420),
    ("Jakarta / Medan (WIB)",   420),
    ("Bali / Makassar (WITA)",  480),
    ("Papua / Ambon (WIT)",     540),
    ("Singapore (SGT)",         480),
    ("Beijing (CST)",           480),
    ("Tokyo (JST)",             540),
    ("Seoul (KST)",             540),
    ("Sydney (AEST)",           600),
    ("Auckland (NZST)",         720),
    ("São Paulo (BRT)",        -180),
    ("New York (EST)",         -300),
    ("Chicago (CST)",          -360),
    ("Denver (MST)",           -420),
    ("Los Angeles (PST)",      -480),
    ("Anchorage (AKST)",       -540),
    ("Honolulu (HST)",         -600),
];

/// Convert a UTC offset in minutes to a `chrono::FixedOffset`.
///
/// Falls back to UTC silently if the offset is out of range (shouldn't
/// happen with values sourced from [`WORLD_ZONES`]).
#[inline]
pub fn timezone_from_offset(offset_mins: i32) -> FixedOffset {
    FixedOffset::east_opt(offset_mins * 60)
        .unwrap_or_else(|| FixedOffset::east_opt(0).unwrap())
}

/// Build the compact `"UTC+HH:MM"` / `"UTC-HH:MM"` tag shown in date output.
///
/// Examples: `timezone_utc_tag(420)` → `"UTC+07:00"`,
///           `timezone_utc_tag(-300)` → `"UTC-05:00"`.
#[inline]
pub fn timezone_utc_tag(offset_mins: i32) -> String {
    let sign = if offset_mins >= 0 { "+" } else { "-" };
    let abs  = offset_mins.abs();
    format!("UTC{}{:02}:{:02}", sign, abs / 60, abs % 60)
}

// ── App IDs ─────────────────────────────────────────────────────────────────

/// String identifiers for every built-in NeuraOS application.
///
/// Use these constants wherever an app ID string is needed instead of bare
/// string literals.  Renaming or adding an app requires only one edit here.
pub mod app_id {
    // Core productivity
    pub const NOTES:    &str = "notes";
    pub const TASKS:    &str = "tasks";
    pub const CONTACTS: &str = "contacts";
    pub const CALENDAR: &str = "calendar";

    // System
    pub const FILES:    &str = "files";
    pub const SETTINGS: &str = "settings";
    pub const MONITOR:  &str = "monitor";
    pub const LOGS:     &str = "logs";
    pub const SYSINFO:  &str = "sysinfo";
    pub const BACKUP:   &str = "backup";

    // Utilities
    pub const CALC:     &str = "calc";
    pub const CLOCK:    &str = "clock";

    // Communication / AI
    pub const CHAT:     &str = "chat";
    pub const MAIL:     &str = "mail";

    // Development
    pub const DEV:      &str = "dev";
    pub const TERMINAL: &str = "terminal";

    // Network / browsing
    pub const BROWSER:  &str = "browser";
    pub const WEATHER:  &str = "weather";
    pub const SSH:      &str = "ssh";
    pub const FTP:      &str = "ftp";

    // Data / storage
    pub const MEDIA:    &str = "media";
    pub const DB:       &str = "db";
    pub const SYNC:     &str = "sync";
    pub const STORE:    &str = "store";

    /// Every built-in app ID in a single slice, alphabetically sorted.
    ///
    /// Used by the shell executor to recognise bare app-name commands and by
    /// `agent_tools` to build the app registry.
    pub const ALL: &[&str] = &[
        BACKUP, BROWSER, CALC, CALENDAR, CHAT, CLOCK, CONTACTS,
        DB, DEV, FILES, FTP, LOGS, MAIL, MEDIA, MONITOR,
        NOTES, SETTINGS, SSH, STORE, SYNC, SYSINFO, TASKS,
        TERMINAL, WEATHER,
    ];
}

// ── VFS user paths ──────────────────────────────────────────────────────────

/// Functions that build VFS-absolute paths for per-user data.
///
/// Every path string under `/home/<username>/` must be produced by one of
/// these helpers rather than constructed inline.  Moving or renaming a file
/// requires a single change here.
pub mod vfs_paths {
    // ── User directories ───────────────────────────────────────────────────

    /// `/home/<username>` — user home directory root.
    #[inline] pub fn home(username: &str) -> String {
        format!("/home/{}", username)
    }

    /// `/home/<username>/notes` — directory that holds `.notes` files.
    #[inline] pub fn notes_dir(username: &str) -> String {
        format!("/home/{}/notes", username)
    }

    /// `/home/<username>/notes/<filename>.notes` — individual note file.
    #[inline] pub fn note_file(username: &str, filename: &str) -> String {
        format!("/home/{}/notes/{}.notes", username, filename)
    }

    /// `/home/<username>/tasks.task` — task list (used by NeuraTasks app).
    #[inline] pub fn tasks(username: &str) -> String {
        format!("/home/{}/tasks.task", username)
    }

    /// `/home/<username>/tasks.json` — task list (used by AI agent tools).
    #[inline] pub fn tasks_json(username: &str) -> String {
        format!("/home/{}/tasks.json", username)
    }

    /// `/home/<username>/contacts.json` — address book.
    #[inline] pub fn contacts(username: &str) -> String {
        format!("/home/{}/contacts.json", username)
    }

    /// `/home/<username>/settings.json` — per-user UI preferences.
    #[inline] pub fn settings(username: &str) -> String {
        format!("/home/{}/settings.json", username)
    }

    /// `/home/<username>/media.json` — media library metadata.
    #[inline] pub fn media(username: &str) -> String {
        format!("/home/{}/media.json", username)
    }

    /// `/home/<username>/browser_bookmarks.json` — browser bookmarks.
    #[inline] pub fn browser_bookmarks(username: &str) -> String {
        format!("/home/{}/browser_bookmarks.json", username)
    }

    /// `/home/<username>/mail_account.json` — email account configuration.
    #[inline] pub fn mail_account(username: &str) -> String {
        format!("/home/{}/mail_account.json", username)
    }

    /// `/home/<username>/mail_sent.json` — sent-messages store.
    #[inline] pub fn mail_sent(username: &str) -> String {
        format!("/home/{}/mail_sent.json", username)
    }

    /// `/home/<username>/mail_inbox_cache.json` — cached inbox messages.
    #[inline] pub fn mail_inbox(username: &str) -> String {
        format!("/home/{}/mail_inbox_cache.json", username)
    }

    /// `/home/<username>/backups` — directory containing backup snapshots.
    #[inline] pub fn backups_dir(username: &str) -> String {
        format!("/home/{}/backups", username)
    }

    // ── VFS system paths (static) ──────────────────────────────────────────

    /// `/home` — top-level home directory.
    pub const VFS_HOME:        &str = "/home";
    /// `/system` — OS system directory.
    pub const VFS_SYSTEM:      &str = "/system";
    /// `/system/config` — system-wide configuration storage.
    pub const VFS_SYSTEM_CFG:  &str = "/system/config";
    /// `/system/logs` — system log storage.
    pub const VFS_SYSTEM_LOGS: &str = "/system/logs";
    /// `/tmp` — temporary / scratch files.
    pub const VFS_TMP:         &str = "/tmp";
    /// `/apps` — installed application data.
    pub const VFS_APPS:        &str = "/apps";
}
