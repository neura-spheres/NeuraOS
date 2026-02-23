use serde::{Serialize, Deserialize};
use neura_app_framework::consts::{OS_VERSION, DEFAULT_HOSTNAME, DEFAULT_AI_TEMPERATURE};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    pub system: SystemSection,
    pub ai: AiSection,
    pub ui: UiSection,
    pub storage: StorageSection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSection {
    pub hostname: String,
    pub version: String,
    pub log_level: String,
    pub auto_save_interval_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiSection {
    pub provider: String,
    pub model: String,
    pub api_key_env: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub rate_limit_rpm: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSection {
    pub theme: String,
    pub show_statusbar: bool,
    pub show_clock: bool,
    pub default_workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSection {
    pub max_cache_mb: u64,
    pub journal_max_entries: usize,
    pub auto_backup: bool,
    pub backup_interval_hours: u64,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            system: SystemSection {
                hostname: DEFAULT_HOSTNAME.to_string(),
                version: OS_VERSION.to_string(),
                log_level: "info".to_string(),
                auto_save_interval_secs: 30,
            },
            ai: AiSection {
                provider: "gemini".to_string(),
                model: "gemini-2.5-flash-lite".to_string(),
                api_key_env: "GEMINI_API_KEY".to_string(),
                max_tokens: 8192,
                temperature: DEFAULT_AI_TEMPERATURE,
                rate_limit_rpm: 60,
            },
            ui: UiSection {
                theme: "default".to_string(),
                show_statusbar: true,
                show_clock: true,
                default_workspace: "main".to_string(),
            },
            storage: StorageSection {
                max_cache_mb: 512,
                journal_max_entries: 10_000,
                auto_backup: true,
                backup_interval_hours: 24,
            },
        }
    }
}
