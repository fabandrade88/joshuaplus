//! Aligning a recorded input to the song timeline.
//!
//! While recording, every input callback logs a [`Marker`]: how many frames
//! the input has delivered so far and the song position the output had
//! reached at that moment. Inputs on another device run on their own clock,
//! and callbacks jitter by a block or two, so a single marker is not enough.
//! [`fit_alignment`] fits a straight line through all markers (least squares
//! over the longest stretch of steady playback), which gives both the offset
//! and the exact clock ratio. [`render_take`] then resamples the input onto
//! the song's frame grid, shifted by the round-trip latency: the musician
//! hears a frame `output latency` after it is rendered, and the input
//! delivers what they played `input latency` after it happened.

use crate::audio::StereoBuffer;
use crate::error::Result;

/// One input callback while recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marker {
    /// Input frames delivered so far (after this callback).
    pub input_frames: u64,
    /// Song position (output frames) at that moment.
    pub song_pos: i64,
}

/// `song_frame = offset + ratio × input_frame` (before latency correction).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Alignment {
    pub offset: f64,
    pub ratio: f64,
}

/// Largest clock mismatch accepted between devices (±0.5 %).
const MAX_DRIFT: f64 = 0.005;

/// Fits the input→song mapping. `nominal_ratio` is output rate / input rate.
/// Markers taken while the song was not advancing (paused, count-in) and
/// jumps (seek, loop) are ignored: the longest steady stretch wins.
/// Returns `None` when the song never played during the recording.
#[must_use]
pub fn fit_alignment(markers: &[Marker], nominal_ratio: f64) -> Option<Alignment> {
    if !(nominal_ratio.is_finite() && nominal_ratio > 0.0) {
        return None;
    }
    // Split into segments of steady playback.
    let mut best: &[Marker] = &[];
    let mut start = 0;
    for i in 1..=markers.len() {
        let steady = i < markers.len() && {
            let (a, b) = (markers[i - 1], markers[i]);
            let din = b.input_frames.saturating_sub(a.input_frames) as f64;
            let dout = (b.song_pos - a.song_pos) as f64;
            // Advancing, and roughly at the expected speed (a seek or loop
            // jumps by far more than one callback's jitter).
            b.input_frames > a.input_frames
                && dout > 0.0
                && (dout - din * nominal_ratio).abs() <= din * nominal_ratio * 0.5 + 4_096.0
        };
        if !steady {
            // markers[start..i] is a run of steady steps (plus its first point).
            if i - start > best.len() {
                best = &markers[start..i];
            }
            start = i;
        }
    }
    if best.len() < 2 {
        return None;
    }
    let n = best.len() as f64;
    let (mx, my) = best.iter().fold((0.0, 0.0), |(x, y), m| (x + m.input_frames as f64, y + m.song_pos as f64));
    let (mx, my) = (mx / n, my / n);
    let (mut sxy, mut sxx) = (0.0, 0.0);
    for m in best {
        let dx = m.input_frames as f64 - mx;
        sxy += dx * (m.song_pos as f64 - my);
        sxx += dx * dx;
    }
    let fitted = if sxx > 0.0 { sxy / sxx } else { nominal_ratio };
    // Short takes give a noisy slope: trust the nominal ratio within limits.
    let ratio =
        if fitted.is_finite() && (fitted / nominal_ratio - 1.0).abs() <= MAX_DRIFT { fitted } else { nominal_ratio };
    Some(Alignment { offset: my - ratio * mx, ratio })
}

/// A take ready to save: audio on the song's frame grid starting at `start`.
#[derive(Debug)]
pub struct AlignedTake {
    /// Song frame of the first sample (≥ 0).
    pub start: i64,
    pub audio: StereoBuffer,
}

/// Resamples `input` onto the song grid using `align`, shifted earlier by
/// `latency_frames` (round-trip latency, in song frames), and clipped to
/// `0..song_length`. Returns `None` if nothing of the take falls inside.
///
/// # Errors
/// Only if the output buffer cannot be built.
pub fn render_take(
    input: &StereoBuffer,
    align: Alignment,
    latency_frames: f64,
    song_rate: u32,
    song_length: i64,
) -> Result<Option<AlignedTake>> {
    let frames = input.frames();
    if frames < 4 || !align.ratio.is_finite() || align.ratio <= 0.0 || !latency_frames.is_finite() {
        return Ok(None);
    }
    // Song frame where input frame k belongs.
    let song_at = |k: f64| align.offset + align.ratio * k - latency_frames;
    let first = song_at(0.0).ceil().max(0.0);
    let last = song_at((frames - 1) as f64).floor().min((song_length - 1) as f64);
    if last < first {
        return Ok(None);
    }
    let (start, end) = (first as i64, last as i64);
    let len = usize::try_from(end - start + 1).unwrap_or(0);
    let (l, r) = (input.left(), input.right());
    let mut out_l = Vec::with_capacity(len);
    let mut out_r = Vec::with_capacity(len);
    for j in 0..len {
        let s = (start + j as i64) as f64;
        let k = (s + latency_frames - align.offset) / align.ratio;
        out_l.push(cubic_at(l, k));
        out_r.push(cubic_at(r, k));
    }
    Ok(Some(AlignedTake { start, audio: StereoBuffer::new(song_rate, out_l, out_r)? }))
}

/// 4-point Hermite interpolation of `x` at fractional index `k` (clamped).
fn cubic_at(x: &[f32], k: f64) -> f32 {
    let last = x.len() as isize - 1;
    let i = k.floor() as isize;
    let t = (k - k.floor()) as f32;
    let at = |n: isize| x[n.clamp(0, last) as usize];
    let (y0, y1, y2, y3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
    let c1 = 0.5 * (y2 - y0);
    let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
    let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
    ((c3 * t + c2) * t + c1) * t + y1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Markers as an input at `in_rate` would log them while the output
    /// plays from `start` at 48 kHz, with callback jitter.
    fn markers(in_rate: f64, start: i64, secs: f64, jitter: i64) -> Vec<Marker> {
        let mut v = Vec::new();
        let mut frames = 0u64;
        let mut k = 0i64;
        while (frames as f64) < secs * in_rate {
            frames += 512;
            let t = frames as f64 / in_rate;
            let pos = start + (t * 48_000.0) as i64 + ((k * 7919) % (2 * jitter + 1)) - jitter;
            v.push(Marker { input_frames: frames, song_pos: pos });
            k += 1;
        }
        v
    }

    #[test]
    fn recovers_offset_and_clock_ratio_through_jitter() {
        for in_rate in [47_990.0, 48_000.0, 48_012.0, 44_100.0] {
            let m = markers(in_rate, 96_000, 60.0, 300);
            let a = fit_alignment(&m, 48_000.0 / in_rate).unwrap();
            assert!((a.ratio - 48_000.0 / in_rate).abs() < 2e-6, "ratio {} at {in_rate}", a.ratio);
            // Input frame 0 maps to the song start within a few frames.
            assert!((a.offset - 96_000.0).abs() < 40.0, "offset {} at {in_rate}", a.offset);
        }
    }

    #[test]
    fn ignores_count_in_pauses_and_jumps() {
        let mut m = Vec::new();
        // Count-in: input runs, song position frozen at 48 000.
        for i in 1..=200u64 {
            m.push(Marker { input_frames: i * 512, song_pos: 48_000 });
        }
        // Steady playback from 48 000 for 30 s.
        for i in 1..=2_800u64 {
            m.push(Marker { input_frames: 102_400 + i * 512, song_pos: 48_000 + (i * 512) as i64 });
        }
        // A seek backwards, then a short stretch.
        for i in 1..=50u64 {
            m.push(Marker { input_frames: 1_536_000 + i * 512, song_pos: 10 + (i * 512) as i64 });
        }
        let a = fit_alignment(&m, 1.0).unwrap();
        assert!((a.ratio - 1.0).abs() < 1e-9);
        assert!((a.offset - (48_000.0 - 102_400.0)).abs() < 1e-6, "offset {}", a.offset);
    }

    #[test]
    fn nothing_played_means_no_alignment() {
        let frozen: Vec<_> = (1..100u64).map(|i| Marker { input_frames: i * 512, song_pos: 0 }).collect();
        assert!(fit_alignment(&frozen, 1.0).is_none());
        assert!(fit_alignment(&[], 1.0).is_none());
        assert!(fit_alignment(&frozen, f64::NAN).is_none());
    }

    #[test]
    fn take_lands_on_the_song_grid_after_latency_correction() {
        // An impulse the musician played exactly on song frame 50 000. With a
        // 100-frame offset and 300 frames of round-trip latency, the input
        // recorded it at frame 50 000 - 100 + 300.
        let n = 100_000;
        let mut l = vec![0.0f32; n];
        l[50_200] = 1.0;
        let input = StereoBuffer::new(48_000, l.clone(), l).unwrap();
        let align = Alignment { offset: 100.0, ratio: 1.0 };
        let take = render_take(&input, align, 300.0, 48_000, 1_000_000).unwrap().unwrap();
        let peak = take.audio.left().iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
        assert_eq!(take.start + peak as i64, 50_000);
        assert!(take.start >= 0);
    }

    #[test]
    fn take_is_clipped_to_the_song_and_resampled_by_the_ratio() {
        let input = StereoBuffer::new(44_100, vec![0.5; 44_100], vec![-0.5; 44_100]).unwrap();
        let align = Alignment { offset: -1_000.0, ratio: 48_000.0 / 44_100.0 };
        let take = render_take(&input, align, 0.0, 48_000, 30_000).unwrap().unwrap();
        assert_eq!(take.start, 0, "frames before the song start are dropped");
        assert_eq!(take.start + take.audio.frames() as i64, 30_000, "cut at the song end");
        assert!(take.audio.left().iter().all(|s| (s - 0.5).abs() < 1e-5));
        assert!(take.audio.right().iter().all(|s| (s + 0.5).abs() < 1e-5));
        // Entirely outside the song.
        let outside = Alignment { offset: 40_000.0, ratio: 1.0 };
        assert!(render_take(&input, outside, 0.0, 48_000, 30_000).unwrap().is_none());
    }
}
