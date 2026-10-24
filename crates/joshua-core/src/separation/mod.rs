//! Stem separation back-ends.
//!
//! The [`Separator`] trait decouples the import pipeline from *how* stems
//! are produced. Shipped implementations:
//!
//! * [`spectral::SpectralSeparator`] – built-in, offline, dependency-free DSP
//!   (HPSS + stereo-image analysis). Always available, fast, lower quality.
//! * `joshua_ai::NeuralSeparator` – the built-in studio engine (HT-Demucs,
//!   6 stems, ONNX Runtime). Default; highest offline quality.
//! * `joshua_ai::cloud::LalalSeparator` – optional cloud service for users
//!   who want extra stems (acoustic vs electric guitar).

pub mod spectral;

use std::path::Path;
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};

use crate::audio::StereoBuffer;
use crate::error::Result;

/// Instrument stems understood by the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum StemKind {
    Vocals,
    Drums,
    Bass,
    /// Electric guitar (or all guitars when the engine cannot tell them apart).
    Guitar,
    /// Acoustic guitar ("violão").
    Acoustic,
    Piano,
    Other,
}

impl StemKind {
    pub const ALL: [Self; 7] =
        [Self::Vocals, Self::Drums, Self::Bass, Self::Guitar, Self::Acoustic, Self::Piano, Self::Other];

    /// Lower-case identifier, also the stem file stem (`vocals.wav`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Vocals => "vocals",
            Self::Drums => "drums",
            Self::Bass => "bass",
            Self::Guitar => "guitar",
            Self::Acoustic => "acoustic",
            Self::Piano => "piano",
            Self::Other => "other",
        }
    }

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == name)
    }

    /// Relative path of the stem inside a song directory.
    #[must_use]
    pub fn file_name(self) -> String {
        format!("stems/{}.wav", self.as_str())
    }
}

/// Everything a separator needs for one song.
pub struct SeparationJob<'a> {
    /// The imported source file (inside the song directory).
    pub source_path: &'a Path,
    /// Decoded audio of the source.
    pub audio: &'a StereoBuffer,
    /// Directory (`<song>/stems`) where stems must be written.
    pub stems_dir: &'a Path,
    /// Scratch directory exclusively owned by this job.
    pub scratch_dir: &'a Path,
    /// Progress callback, `0.0..=1.0`.
    pub progress: &'a (dyn Fn(f32) + Sync),
    /// Cooperative cancellation flag.
    pub cancel: &'a AtomicBool,
}

impl std::fmt::Debug for SeparationJob<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SeparationJob")
            .field("source_path", &self.source_path)
            .field("stems_dir", &self.stems_dir)
            .finish_non_exhaustive()
    }
}

/// Receives separated audio in order: `(stem_index, left, right)`.
pub type StemSink<'a> = dyn FnMut(usize, &[f32], &[f32]) -> Result<()> + 'a;

/// A stem file produced by a separator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StemOutput {
    pub kind: StemKind,
    /// Path relative to the song directory, e.g. `stems/vocals.wav`.
    pub relative_path: String,
}

/// A stem separation strategy.
pub trait Separator: Send + Sync {
    /// Stable identifier stored in project files (`"spectral"`, `"neural:htdemucs_6s"`, `"cloud:lalal"`).
    fn id(&self) -> String;

    /// Stems this back-end produces, in display order.
    fn stems(&self) -> Vec<StemKind>;

    /// Runs separation, writing one WAV per stem into `job.stems_dir`.
    ///
    /// # Errors
    /// Back-end specific failures, or [`crate::CoreError::Cancelled`].
    fn separate(&self, job: &SeparationJob<'_>) -> Result<Vec<StemOutput>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stem_names_round_trip_and_are_safe_paths() {
        for k in StemKind::ALL {
            assert_eq!(StemKind::parse(k.as_str()), Some(k));
            assert!(crate::security::is_safe_relative_name(&k.file_name()));
        }
        assert_eq!(StemKind::parse("../vocals"), None);
    }
}
