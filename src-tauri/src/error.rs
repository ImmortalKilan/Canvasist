//! Application error type shared by commands and setup code.

use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("file system error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid data format: {0}")]
    Json(#[from] serde_json::Error),

    #[error("autostart error: {0}")]
    Autostart(String),

    #[error(transparent)]
    Tauri(#[from] tauri::Error),
}

impl From<tauri_plugin_autostart::Error> for AppError {
    fn from(e: tauri_plugin_autostart::Error) -> Self {
        Self::Autostart(e.to_string())
    }
}

/// Errors cross the IPC boundary as plain messages. Messages must never contain
/// credentials or personal data; every variant above only wraps system errors.
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
