//! Application error type shared by commands and background tasks.

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

    #[error("network error: {0}")]
    Network(String),

    #[error("encryption error: {0}")]
    Crypto(String),

    #[error("not a valid Canvas address")]
    InvalidUrl,

    #[error("this address does not look like a Canvas site")]
    NotCanvas,

    #[error("not signed in to Canvas")]
    NotSignedIn,

    #[error("the Canvas session has expired")]
    SessionExpired,

    #[error("Canvas returned HTTP {0}")]
    CanvasStatus(u16),

    #[error("a refresh is already running")]
    Busy,
}

impl AppError {
    /// Stable identifier the frontend uses to pick a localized message.
    fn kind(&self) -> &'static str {
        match self {
            Self::Io(_) => "io",
            Self::Json(_) => "data",
            Self::Autostart(_) => "autostart",
            Self::Tauri(_) => "internal",
            Self::Network(_) => "network",
            Self::Crypto(_) => "crypto",
            Self::InvalidUrl => "invalidUrl",
            Self::NotCanvas => "notCanvas",
            Self::NotSignedIn => "notSignedIn",
            Self::SessionExpired => "sessionExpired",
            Self::CanvasStatus(_) => "canvas",
            Self::Busy => "busy",
        }
    }
}

impl From<tauri_plugin_autostart::Error> for AppError {
    fn from(e: tauri_plugin_autostart::Error) -> Self {
        Self::Autostart(e.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        // Request URLs can carry course or assignment IDs; never surface them.
        Self::Network(e.without_url().to_string())
    }
}

#[derive(Serialize)]
struct ErrorPayload<'a> {
    kind: &'a str,
    message: String,
}

/// Errors cross the IPC boundary as `{ kind, message }`. Messages must never
/// contain credentials or personal data; every variant above only wraps
/// system-level details.
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ErrorPayload {
            kind: self.kind(),
            message: self.to_string(),
        }
        .serialize(serializer)
    }
}

pub type AppResult<T> = Result<T, AppError>;
