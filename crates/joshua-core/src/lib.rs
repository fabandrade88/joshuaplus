//! # joshua-core
//!
//! Platform-independent heart of Joshua+. Everything in this crate is pure
//! Rust, free of `unsafe`, and does not talk to audio devices or the UI, which
//! keeps it fully unit-testable:
//!
//! * [`security`] – input validation (paths, names, file signatures, limits).
//! * [`decode`] / [`wav`] – audio file I/O.
//! * [`tempo`] – BPM detection and beat tracking.
//! * [`separation`] – stem separation back-ends (built-in DSP, Demucs).
//! * [`metronome`] – click track synthesis aligned to detected beats.
//! * [`mixer`] – real-time safe mixing core used by the audio engine.
//! * [`project`] / [`workspace`] / [`import`] – on-disk song projects.
#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod audio;
pub mod decode;
pub mod error;
pub mod export;
pub mod folders;
pub mod import;
pub mod metadata;
pub mod metronome;
pub mod mixer;
pub mod peaks;
pub mod project;
pub mod recording;
pub mod resample;
pub mod room;
pub mod security;
pub mod separation;
pub mod stft;
pub mod tempo;
pub mod wav;
pub mod workspace;

pub use audio::StereoBuffer;
pub use error::{CoreError, RejectReason, Result};
