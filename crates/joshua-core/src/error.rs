//! Error types shared by the whole core crate.

use std::fmt;

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, CoreError>;

/// Why an input was refused by the security policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    NotARegularFile,
    Symlink,
    EmptyFile,
    TooLarge,
    TooLong,
    UnsupportedExtension,
    UnrecognizedSignature,
    OutsideWorkspace,
    InvalidName,
    InvalidProject,
}

impl RejectReason {
    /// Stable machine readable code (used by the UI for translations).
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::NotARegularFile => "not_a_regular_file",
            Self::Symlink => "symlink",
            Self::EmptyFile => "empty_file",
            Self::TooLarge => "too_large",
            Self::TooLong => "too_long",
            Self::UnsupportedExtension => "unsupported_extension",
            Self::UnrecognizedSignature => "unrecognized_signature",
            Self::OutsideWorkspace => "outside_workspace",
            Self::InvalidName => "invalid_name",
            Self::InvalidProject => "invalid_project",
        }
    }
}

impl fmt::Display for RejectReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// All errors produced by the core crate.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("rejected by security policy: {0}")]
    Rejected(RejectReason),
    #[error("could not decode audio: {0}")]
    Decode(String),
    #[error("invalid audio data: {0}")]
    InvalidAudio(String),
    #[error("stem separation failed: {0}")]
    Separation(String),
    #[error("operation cancelled")]
    Cancelled,
    #[error("song not found")]
    NotFound,
    #[error("project file error: {0}")]
    Project(String),
}

impl CoreError {
    /// Stable machine readable code (used by the UI for translations).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "io",
            Self::Rejected(reason) => reason.code(),
            Self::Decode(_) => "decode",
            Self::InvalidAudio(_) => "invalid_audio",
            Self::Separation(_) => "separation",
            Self::Cancelled => "cancelled",
            Self::NotFound => "not_found",
            Self::Project(_) => "project",
        }
    }
}

impl From<RejectReason> for CoreError {
    fn from(reason: RejectReason) -> Self {
        Self::Rejected(reason)
    }
}
