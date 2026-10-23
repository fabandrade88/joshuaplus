//! In-memory audio representation.

use crate::error::{CoreError, Result};

/// Lowest and highest sample rates accepted anywhere in the application.
pub const MIN_SAMPLE_RATE: u32 = 8_000;
pub const MAX_SAMPLE_RATE: u32 = 384_000;

/// Planar stereo `f32` audio. Every source is normalised to stereo on
/// decode so that the rest of the pipeline never branches on channel count.
#[derive(Debug, Clone, PartialEq)]
pub struct StereoBuffer {
    sample_rate: u32,
    left: Vec<f32>,
    right: Vec<f32>,
}

impl StereoBuffer {
    /// Builds a buffer, validating sample rate and channel lengths.
    ///
    /// # Errors
    /// Returns [`CoreError::InvalidAudio`] if the sample rate is out of range
    /// or the channels have different lengths.
    pub fn new(sample_rate: u32, left: Vec<f32>, right: Vec<f32>) -> Result<Self> {
        if !(MIN_SAMPLE_RATE..=MAX_SAMPLE_RATE).contains(&sample_rate) {
            return Err(CoreError::InvalidAudio(format!("sample rate {sample_rate} out of range")));
        }
        if left.len() != right.len() {
            return Err(CoreError::InvalidAudio("channel length mismatch".into()));
        }
        Ok(Self { sample_rate, left, right })
    }

    /// Mono source duplicated on both channels.
    ///
    /// # Errors
    /// Same as [`StereoBuffer::new`].
    pub fn from_mono(sample_rate: u32, mono: Vec<f32>) -> Result<Self> {
        let right = mono.clone();
        Self::new(sample_rate, mono, right)
    }

    /// Silent buffer of `frames` length.
    ///
    /// # Errors
    /// Same as [`StereoBuffer::new`].
    pub fn silent(sample_rate: u32, frames: usize) -> Result<Self> {
        Self::new(sample_rate, vec![0.0; frames], vec![0.0; frames])
    }

    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    #[must_use]
    pub fn frames(&self) -> usize {
        self.left.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.left.is_empty()
    }

    #[must_use]
    pub fn duration_secs(&self) -> f64 {
        self.frames() as f64 / f64::from(self.sample_rate)
    }

    #[must_use]
    pub fn left(&self) -> &[f32] {
        &self.left
    }

    #[must_use]
    pub fn right(&self) -> &[f32] {
        &self.right
    }

    /// Mono downmix `(L + R) / 2`.
    #[must_use]
    pub fn to_mono(&self) -> Vec<f32> {
        self.left.iter().zip(&self.right).map(|(l, r)| 0.5 * (l + r)).collect()
    }

    /// Consumes the buffer returning `(sample_rate, left, right)`.
    #[must_use]
    pub fn into_parts(self) -> (u32, Vec<f32>, Vec<f32>) {
        (self.sample_rate, self.left, self.right)
    }
}

/// Converts decibels to a linear gain factor. `-inf`/very low values map to 0.
#[must_use]
pub fn db_to_gain(db: f32) -> f32 {
    if db <= -96.0 { 0.0 } else { 10f32.powf(db / 20.0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_sample_rate_and_mismatched_channels() {
        assert!(StereoBuffer::new(100, vec![], vec![]).is_err());
        assert!(StereoBuffer::new(1_000_000, vec![], vec![]).is_err());
        assert!(StereoBuffer::new(44_100, vec![0.0], vec![]).is_err());
    }

    #[test]
    fn mono_downmix_and_duration() {
        let buf = StereoBuffer::new(48_000, vec![1.0; 48_000], vec![0.0; 48_000]).unwrap();
        assert!((buf.duration_secs() - 1.0).abs() < 1e-9);
        assert!(buf.to_mono().iter().all(|s| (*s - 0.5).abs() < 1e-6));
    }

    #[test]
    fn db_conversion() {
        assert!((db_to_gain(0.0) - 1.0).abs() < 1e-6);
        assert!((db_to_gain(-6.0) - 0.501).abs() < 1e-3);
        assert_eq!(db_to_gain(-120.0), 0.0);
    }
}
