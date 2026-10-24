//! Errors crossing the IPC boundary. Only a stable `code` (translated by the
//! UI) and a short message are exposed — never paths or internal details
//! beyond what the user already knows.

use serde::Serialize;

use joshua_core::CoreError;
use joshua_engine::EngineError;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ApiError {
    pub code: &'static str,
    pub message: String,
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }

    pub fn no_workspace() -> Self {
        Self::new("no_workspace", "choose a workspace folder first")
    }

    pub fn no_song() -> Self {
        Self::new("no_song", "no song loaded")
    }

    pub fn no_audio() -> Self {
        Self::new("no_audio_device", "audio output unavailable")
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid_argument", message)
    }
}

impl From<CoreError> for ApiError {
    fn from(e: CoreError) -> Self {
        // I/O messages may contain absolute paths; keep them generic.
        let message = match &e {
            CoreError::Io(io) => io.kind().to_string(),
            other => other.to_string(),
        };
        Self { code: e.code(), message }
    }
}

impl From<EngineError> for ApiError {
    fn from(e: EngineError) -> Self {
        Self::new("audio_engine", e.to_string())
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
