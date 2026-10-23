//! # joshua-ai
//!
//! Joshua+'s built-in neural stem separator. It runs HT-Demucs v4 (6 stems)
//! natively through ONNX Runtime — no Python, no cloud, no account — and uses
//! DirectML on Windows or CUDA when enabled, otherwise all CPU cores.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod cloud;
pub mod model;
pub mod separator;

pub use model::{HTDEMUCS_6S, ModelSpec, ModelStore};
pub use separator::NeuralSeparator;

/// HTTP client with timeouts: a stalled server can never hang the import
/// queue. `long_body` allows large downloads (the model) to take longer.
pub(crate) fn http_agent(long_body: bool) -> ureq::Agent {
    use std::time::Duration;
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_response(Some(Duration::from_secs(60)))
        .timeout_global(Some(Duration::from_secs(if long_body { 45 * 60 } else { 10 * 60 })))
        .max_redirects(5)
        .build()
        .new_agent()
}

/// Errors of the AI back-end.
#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("model download failed: {0}")]
    Download(String),
    #[error("inference failed: {0}")]
    Inference(String),
    #[error("cancelled")]
    Cancelled,
}

impl From<ort::Error> for AiError {
    fn from(e: ort::Error) -> Self {
        Self::Inference(e.to_string())
    }
}

impl From<AiError> for joshua_core::CoreError {
    fn from(e: AiError) -> Self {
        match e {
            AiError::Cancelled => Self::Cancelled,
            AiError::Io(io) => Self::Io(io),
            other => Self::Separation(other.to_string()),
        }
    }
}
