//! Offline sample-rate conversion (used when the audio device runs at a
//! different rate than the song).

use rubato::audioadapter_buffers::direct::SequentialSliceOfVecs;
use rubato::{Fft, FixedSync, Resampler};

use crate::audio::StereoBuffer;
use crate::error::{CoreError, Result};

/// Converts `buffer` to `target_rate` with an FFT resampler.
/// Returns the input untouched when the rates already match.
///
/// # Errors
/// [`CoreError::InvalidAudio`] if the resampler cannot be built.
pub fn resample(buffer: StereoBuffer, target_rate: u32) -> Result<StereoBuffer> {
    if buffer.sample_rate() == target_rate || buffer.is_empty() {
        let (_, l, r) = buffer.into_parts();
        return StereoBuffer::new(target_rate, l, r);
    }
    let frames = buffer.frames();
    let (rate, left, right) = buffer.into_parts();
    let err = |e: &dyn std::fmt::Display| CoreError::InvalidAudio(format!("resampler: {e}"));
    let mut resampler =
        Fft::<f32>::new(rate as usize, target_rate as usize, 1024, 2, FixedSync::Both).map_err(|e| err(&e))?;
    let input = [left, right];
    let adapter = SequentialSliceOfVecs::new(&input, 2, frames).map_err(|e| err(&e))?;
    let out = resampler.process_all(&adapter, frames, None).map_err(|e| err(&e))?;
    let data = out.take_data();
    let mut l = Vec::with_capacity(data.len() / 2);
    let mut r = Vec::with_capacity(data.len() / 2);
    for frame in data.chunks_exact(2) {
        l.push(frame[0]);
        r.push(frame[1]);
    }
    StereoBuffer::new(target_rate, l, r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_length_and_preserves_tone() {
        let sine: Vec<f32> = (0..44_100).map(|i| (std::f32::consts::TAU * 441.0 * i as f32 / 44_100.0).sin()).collect();
        let buf = StereoBuffer::from_mono(44_100, sine).unwrap();
        let out = resample(buf, 48_000).unwrap();
        assert_eq!(out.sample_rate(), 48_000);
        assert!((out.frames() as i64 - 48_000).abs() <= 2, "frames {}", out.frames());
        // Zero crossings ≈ 2 × 441 per second.
        let crossings = out.left().windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        assert!((crossings as i64 - 441).abs() <= 2, "crossings {crossings}");
    }

    #[test]
    fn same_rate_is_identity() {
        let buf = StereoBuffer::new(48_000, vec![0.1, 0.2], vec![0.3, 0.4]).unwrap();
        assert_eq!(resample(buf.clone(), 48_000).unwrap(), buf);
    }
}
