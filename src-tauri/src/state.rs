use std::sync::Arc;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppStatus {
    Idle,
    Recording,
    Transcribing,
    LoadingModel,
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotkeyMode {
    PushToTalk,
    Toggle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputMethod {
    Paste,
    Type,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub hotkey: String,
    pub mode: HotkeyMode,
    pub model: String,
    pub language: Option<String>,
    pub output_method: OutputMethod,
    pub input_device: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            hotkey: "F9".to_string(),
            mode: HotkeyMode::PushToTalk,
            model: "ggml-base".to_string(),
            language: None,
            output_method: OutputMethod::Paste,
            input_device: None,
        }
    }
}

// ---------------------------------------------------------------------------
// AppState (shared across Tauri commands via tauri::State)
// ---------------------------------------------------------------------------

pub struct AppState {
    pub status: AppStatus,
    pub config: AppConfig,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        Self {
            status: AppStatus::Idle,
            config,
        }
    }
}

/// Thread-safe handle to AppState used as a Tauri managed resource.
pub type SharedState = Arc<Mutex<AppState>>;
