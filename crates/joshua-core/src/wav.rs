//! WAV output. Stems are written as 24-bit PCM through a streaming writer so
//! separation never needs to hold every stem in memory. Files are written to
//! a temporary name and atomically renamed on success, so a crash never
//! leaves a half-written stem that looks valid.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use hound::{SampleFormat, WavSpec, WavWriter};

use crate::audio::StereoBuffer;
use crate::error::{CoreError, Result};

const BITS: u16 = 24;
const FULL_SCALE: f32 = 8_388_607.0; // 2^23 - 1

/// Streaming stereo 24-bit WAV writer with atomic publish.
pub struct StereoWavWriter {
    writer: Option<WavWriter<BufWriter<File>>>,
    tmp_path: PathBuf,
    final_path: PathBuf,
}

impl std::fmt::Debug for StereoWavWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StereoWavWriter").field("final_path", &self.final_path).finish_non_exhaustive()
    }
}

impl StereoWavWriter {
    /// Creates `<path>.part`; the final name appears only on [`finish`](Self::finish).
    ///
    /// # Errors
    /// I/O errors creating the file.
    pub fn create(path: &Path, sample_rate: u32) -> Result<Self> {
        let spec = WavSpec { channels: 2, sample_rate, bits_per_sample: BITS, sample_format: SampleFormat::Int };
        let mut tmp_path = path.as_os_str().to_owned();
        tmp_path.push(".part");
        let tmp_path = PathBuf::from(tmp_path);
        // Exclusive create: never write through a planted symlink.
        let _ = std::fs::remove_file(&tmp_path);
        let file = crate::security::create_exclusive(&tmp_path, false)?;
        let writer = WavWriter::new(BufWriter::new(file), spec).map_err(hound_err)?;
        Ok(Self { writer: Some(writer), tmp_path, final_path: path.to_path_buf() })
    }

    /// Appends planar samples (equal lengths).
    ///
    /// # Errors
    /// I/O errors, or mismatched lengths.
    pub fn write(&mut self, left: &[f32], right: &[f32]) -> Result<()> {
        if left.len() != right.len() {
            return Err(CoreError::InvalidAudio("channel length mismatch".into()));
        }
        let writer = self.writer.as_mut().ok_or_else(|| CoreError::InvalidAudio("writer closed".into()))?;
        for (l, r) in left.iter().zip(right) {
            writer.write_sample(to_i24(*l)).map_err(hound_err)?;
            writer.write_sample(to_i24(*r)).map_err(hound_err)?;
        }
        Ok(())
    }

    /// Finalises the header and renames the file into place.
    ///
    /// # Errors
    /// I/O errors.
    pub fn finish(mut self) -> Result<PathBuf> {
        if let Some(w) = self.writer.take() {
            w.finalize().map_err(hound_err)?;
        }
        std::fs::rename(&self.tmp_path, &self.final_path)?;
        Ok(self.final_path.clone())
    }
}

impl Drop for StereoWavWriter {
    fn drop(&mut self) {
        // Not finished → discard the partial file.
        if self.writer.take().is_some() {
            let _ = std::fs::remove_file(&self.tmp_path);
        }
    }
}

/// Writes a whole buffer in one go.
///
/// # Errors
/// I/O errors.
pub fn write_stereo_wav(path: &Path, buffer: &StereoBuffer) -> Result<PathBuf> {
    let mut w = StereoWavWriter::create(path, buffer.sample_rate())?;
    w.write(buffer.left(), buffer.right())?;
    w.finish()
}

fn to_i24(sample: f32) -> i32 {
    // Truncation is intended: the value is clamped to the 24-bit range first.
    #[allow(clippy::cast_possible_truncation)]
    let v = (sample.clamp(-1.0, 1.0) * FULL_SCALE).round() as i32;
    v
}

fn hound_err(e: hound::Error) -> CoreError {
    match e {
        hound::Error::IoError(io) => CoreError::Io(io),
        other => CoreError::InvalidAudio(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::decode_file;
    use crate::security::Limits;

    #[test]
    fn round_trip_preserves_audio_within_24bit_precision() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.wav");
        let left: Vec<f32> = (0..4410).map(|i| (i as f32 * 0.01).sin() * 0.8).collect();
        let right: Vec<f32> = left.iter().map(|s| -s).collect();
        let buf = StereoBuffer::new(44_100, left, right).unwrap();
        write_stereo_wav(&path, &buf).unwrap();
        let back = decode_file(&path, &Limits::default()).unwrap();
        assert_eq!(back.frames(), buf.frames());
        let max_err = back.left().iter().zip(buf.left()).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
        assert!(max_err < 1e-6, "max error {max_err}");
        assert!(!dir.path().join("x.wav.part").exists());
    }

    #[test]
    fn clips_out_of_range_samples() {
        assert_eq!(to_i24(2.0), 8_388_607);
        assert_eq!(to_i24(-2.0), -8_388_607);
    }

    #[test]
    fn unfinished_writer_leaves_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("y.wav");
        {
            let mut w = StereoWavWriter::create(&path, 48_000).unwrap();
            w.write(&[0.1], &[0.1]).unwrap();
        }
        assert!(!path.exists());
        assert!(!dir.path().join("y.wav.part").exists());
    }
}
