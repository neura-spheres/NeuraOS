// Built-in applications will be added as sub-modules.
// Each app implements the neura_app_framework::App trait.

pub mod notes;
pub mod tasks;
pub mod files;
pub mod settings;
pub mod placeholder;
pub mod calc;
pub mod clock;
pub mod monitor;
pub mod calendar;
pub mod sysinfo_app;
pub mod logs;
pub mod contacts;
pub mod ollama_setup;

// OS agent tools (AI can control the whole OS via these)
pub mod agent_tools;

// New fully-functional apps
pub mod chat;
pub mod dev;
pub mod weather;
pub mod terminal_app;
pub mod media;
pub mod backup;
pub mod browser;
pub mod mail;
