//! Audio decoding with Symphonia (pure Rust, no FFmpeg, no `unsafe`).
//!
//! Untrusted media is decoded defensively:
//! * duration is capped while decoding (memory bomb protection),
//! * corrupt packets are skipped but a stream of only garbage is an error,
//! * decoder panics are caught and converted to errors so a malicious file
//!   can never take down a worker thread.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;

use crate::audio::{MAX_SAMPLE_RATE, MIN_SAMPLE_RATE, StereoBuffer};
use crate::error::{CoreError, RejectReason, Result};
use crate::security::Limits;

/// Give up after this many consecutive undecodable packets.
const MAX_CONSECUTIVE_ERRORS: u32 = 64;

/// Decodes a file from disk into stereo `f32`.
///
/// # Errors
/// [`CoreError::Decode`] for unsupported/corrupt media,
/// [`RejectReason::TooLong`] when exceeding `limits.max_duration_secs`.
pub fn decode_file(path: &Path, limits: &Limits) -> Result<StereoBuffer> {
    let file = crate::security::open_regular(path)?;
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    decode_source(Box::new(file), ext.as_deref(), limits)
}

/// Decodes any media source (file, in-memory cursor, …).
///
/// # Errors
/// See [`decode_file`].
pub fn decode_source(
    source: Box<dyn MediaSource>,
    extension_hint: Option<&str>,
    limits: &Limits,
) -> Result<StereoBuffer> {
    catch_unwind(AssertUnwindSafe(|| decode_inner(source, extension_hint, limits)))
        .unwrap_or_else(|_| Err(CoreError::Decode("decoder panicked on malformed input".into())))
}

fn decode_inner(source: Box<dyn MediaSource>, extension_hint: Option<&str>, limits: &Limits) -> Result<StereoBuffer> {
    let mss = MediaSourceStream::new(source, Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = extension_hint {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| CoreError::Decode(format!("unrecognised format: {e}")))?;

    let track = format.default_track(TrackType::Audio).ok_or_else(|| CoreError::Decode("no audio track".into()))?;
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| CoreError::Decode("missing audio codec parameters".into()))?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(|e| CoreError::Decode(format!("unsupported codec: {e}")))?;

    let mut left: Vec<f32> = Vec::new();
    let mut right: Vec<f32> = Vec::new();
    let mut interleaved: Vec<f32> = Vec::new();
    let mut sample_rate: Option<u32> = None;
    let mut max_frames = usize::MAX;
    let mut consecutive_errors = 0u32;

    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            // Chained streams / truncated tails: keep what we have.
            Err(SymError::ResetRequired | SymError::IoError(_)) => break,
            Err(e) => return Err(CoreError::Decode(e.to_string())),
        };
        if packet.track_id != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(buf) => {
                consecutive_errors = 0;
                buf
            }
            Err(SymError::DecodeError(_) | SymError::IoError(_)) => {
                consecutive_errors += 1;
                if consecutive_errors > MAX_CONSECUTIVE_ERRORS {
                    return Err(CoreError::Decode("too many corrupt packets".into()));
                }
                continue;
            }
            Err(e) => return Err(CoreError::Decode(e.to_string())),
        };

        let spec = decoded.spec();
        let rate = spec.rate();
        let channels = spec.channels().count();
        if channels == 0 || !(MIN_SAMPLE_RATE..=MAX_SAMPLE_RATE).contains(&rate) {
            return Err(CoreError::Decode("invalid stream parameters".into()));
        }
        match sample_rate {
            None => {
                sample_rate = Some(rate);
                // Cap by a rate-independent budget (duration × 48 kHz) so a tiny
                // file of high-rate silence cannot allocate gigabytes.
                max_frames = (limits.max_duration_secs as usize).saturating_mul(rate.min(48_000) as usize);
                left.reserve(rate as usize * 60);
                right.reserve(rate as usize * 60);
            }
            Some(r) if r != rate => {
                return Err(CoreError::Decode("sample rate changes mid-stream".into()));
            }
            Some(_) => {}
        }

        interleaved.resize(decoded.samples_interleaved(), 0.0);
        decoded.copy_to_slice_interleaved(&mut interleaved);
        downmix_into(&interleaved, channels, &mut left, &mut right);

        if left.len() > max_frames {
            return Err(RejectReason::TooLong.into());
        }
    }

    let rate = sample_rate.ok_or_else(|| CoreError::Decode("no decodable audio".into()))?;
    if left.is_empty() {
        return Err(CoreError::Decode("no decodable audio".into()));
    }
    sanitize_samples(&mut left);
    sanitize_samples(&mut right);
    StereoBuffer::new(rate, left, right)
}

/// Folds an interleaved frame of any channel count into stereo.
/// Even channels feed the left side and odd channels the right side, which
/// matches the common FL/FR/FC/LFE/BL/BR order closely enough for practice.
fn downmix_into(interleaved: &[f32], channels: usize, left: &mut Vec<f32>, right: &mut Vec<f32>) {
    match channels {
        1 => {
            left.extend_from_slice(interleaved);
            right.extend_from_slice(interleaved);
        }
        2 => {
            for frame in interleaved.chunks_exact(2) {
                left.push(frame[0]);
                right.push(frame[1]);
            }
        }
        n => {
            let even = n.div_ceil(2) as f32;
            let odd = (n / 2) as f32;
            for frame in interleaved.chunks_exact(n) {
                let (mut l, mut r) = (0.0, 0.0);
                for (i, s) in frame.iter().enumerate() {
                    if i % 2 == 0 { l += s } else { r += s }
                }
                left.push(l / even);
                right.push(r / odd);
            }
        }
    }
}

/// Replaces NaN/inf (possible with crafted float WAVs) and clamps extremes so
/// downstream DSP never sees non-finite values.
fn sanitize_samples(samples: &mut [f32]) {
    for s in samples {
        *s = if s.is_finite() { s.clamp(-4.0, 4.0) } else { 0.0 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn wav_bytes(channels: u16, rate: u32, samples: &[f32]) -> Vec<u8> {
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut w = hound::WavWriter::new(&mut cursor, spec).unwrap();
            for s in samples {
                w.write_sample(*s).unwrap();
            }
            w.finalize().unwrap();
        }
        cursor.into_inner()
    }

    fn decode_bytes(bytes: Vec<u8>, limits: &Limits) -> Result<StereoBuffer> {
        decode_source(Box::new(Cursor::new(bytes)), Some("wav"), limits)
    }

    #[test]
    fn decodes_stereo_wav() {
        let samples: Vec<f32> = (0..2000).map(|i| if i % 2 == 0 { 0.25 } else { -0.5 }).collect();
        let buf = decode_bytes(wav_bytes(2, 44_100, &samples), &Limits::default()).unwrap();
        assert_eq!(buf.sample_rate(), 44_100);
        assert_eq!(buf.frames(), 1000);
        assert!(buf.left().iter().all(|s| (*s - 0.25).abs() < 1e-6));
        assert!(buf.right().iter().all(|s| (*s + 0.5).abs() < 1e-6));
    }

    #[test]
    fn mono_is_duplicated_and_surround_is_folded() {
        let buf = decode_bytes(wav_bytes(1, 22_050, &[0.5; 100]), &Limits::default()).unwrap();
        assert_eq!(buf.left(), buf.right());

        let six: Vec<f32> = (0..600).map(|i| if i % 2 == 0 { 0.3 } else { 0.6 }).collect();
        let buf = decode_bytes(wav_bytes(6, 48_000, &six), &Limits::default()).unwrap();
        assert_eq!(buf.frames(), 100);
        assert!((buf.left()[0] - 0.3).abs() < 1e-6 && (buf.right()[0] - 0.6).abs() < 1e-6);
    }

    #[test]
    fn non_finite_samples_are_neutralised() {
        let bytes = wav_bytes(2, 8_000, &[f32::NAN, f32::INFINITY, 1e30, -1e30]);
        let buf = decode_bytes(bytes, &Limits::default()).unwrap();
        assert_eq!(buf.left()[0], 0.0);
        assert_eq!(buf.right()[0], 0.0);
        assert_eq!(buf.left()[1], 4.0);
        assert_eq!(buf.right()[1], -4.0);
    }

    #[test]
    fn enforces_duration_limit() {
        let limits = Limits { max_duration_secs: 1, ..Limits::default() };
        let bytes = wav_bytes(1, 8_000, &vec![0.0; 8_000 * 3]);
        assert!(matches!(decode_bytes(bytes, &limits), Err(CoreError::Rejected(RejectReason::TooLong))));
    }

    #[test]
    fn garbage_and_truncated_input_fail_gracefully() {
        let limits = Limits::default();
        assert!(decode_bytes(vec![], &limits).is_err());
        assert!(decode_bytes(b"RIFF\xff\xff\xff\xffWAVEjunkjunkjunk".to_vec(), &limits).is_err());
        let mut bytes = wav_bytes(2, 44_100, &[0.1; 64]);
        bytes.truncate(50);
        let _ = decode_bytes(bytes, &limits); // must not panic
        // Header claiming absurd channel counts / rates.
        let mut evil = wav_bytes(2, 44_100, &[0.1; 64]);
        evil[22] = 0xFF;
        evil[23] = 0xFF;
        let _ = decode_bytes(evil, &limits);
    }
}
