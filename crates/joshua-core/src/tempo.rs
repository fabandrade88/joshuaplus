//! Tempo estimation and beat tracking.
//!
//! 1. **Onset strength**: spectral flux of a log-compressed magnitude
//!    spectrogram (positive changes only), adaptively detrended.
//! 2. **Tempo**: autocorrelation of the onset envelope weighted by a
//!    log-normal prior centred on 120 BPM (avoids octave errors).
//! 3. **Beats**: dynamic programming (Ellis, 2007) that balances onset
//!    strength against deviations from the estimated period, so the grid
//!    follows a band that drifts slightly in tempo.
//! 4. **Downbeat**: the bar phase whose beats carry the most low-frequency
//!    onset energy (kick drum / bass hits).

use rustfft::num_complex::Complex32;
use serde::{Deserialize, Serialize};

use crate::audio::StereoBuffer;
use crate::stft::{Stft, hann, windowed_frame};

const MIN_BPM: f64 = 40.0;
const MAX_BPM: f64 = 240.0;
const PRIOR_BPM: f64 = 120.0;
/// Width of the tempo prior in octaves.
const PRIOR_OCTAVES: f64 = 0.9;
/// DP penalty for deviating from the period (Ellis' "tightness").
const TIGHTNESS: f32 = 100.0;
/// Upper limit of the "low band" used for downbeat detection.
const LOW_BAND_HZ: f32 = 150.0;

/// Result of beat analysis, persisted in the project file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BeatAnalysis {
    /// Global tempo estimate.
    pub bpm: f64,
    /// Beat positions in seconds, strictly increasing.
    pub beats: Vec<f64>,
    /// Time signature numerator (beats per bar).
    pub beats_per_bar: u8,
    /// Index (`0..beats_per_bar`) of the first downbeat inside `beats`.
    pub downbeat_phase: u8,
    /// `0..=1`, how periodic the onset envelope is.
    pub confidence: f32,
    /// `false` (the norm): one constant tempo, `beats` is a perfectly even
    /// grid. `true` only for songs that genuinely change tempo; then `beats`
    /// follows the band.
    #[serde(default)]
    pub variable_tempo: bool,
}

impl BeatAnalysis {
    fn empty() -> Self {
        Self {
            bpm: PRIOR_BPM,
            beats: Vec::new(),
            beats_per_bar: 4,
            downbeat_phase: 0,
            confidence: 0.0,
            variable_tempo: false,
        }
    }

    /// `true` if beat `i` starts a bar.
    #[must_use]
    pub fn is_downbeat(&self, i: usize) -> bool {
        let bpb = usize::from(self.beats_per_bar.max(1));
        i % bpb == usize::from(self.downbeat_phase) % bpb
    }
}

/// A manual correction of the detected grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum TempoEdit {
    /// Detected tempo is half the real one (e.g. 60 instead of 120).
    Double,
    /// Detected tempo is double the real one.
    Half,
    /// Move every beat by this many seconds (−0.25..=0.25).
    Nudge(f64),
    /// The beat nearest to this time (seconds) is beat 1 of a bar.
    DownbeatAt(f64),
    /// Time signature numerator (2..=12).
    BeatsPerBar(u8),
    /// The user typed the tempo. The grid is rebuilt at exactly this BPM,
    /// anchored on the beat nearest to `anchor` (seconds, e.g. the playhead)
    /// so the part the user is listening to stays in place.
    SetBpm { bpm: f64, anchor: f64 },
    /// The user tapped along: song times (seconds) of the taps. Aligns the
    /// grid to them; adopts the tapped tempo when clearly different.
    Tap(Vec<f64>),
}

impl BeatAnalysis {
    /// Applies a user correction, returning the edited analysis, or `None`
    /// when the result would be invalid (tempo out of range, no beats).
    #[must_use]
    pub fn edited(&self, edit: TempoEdit) -> Option<Self> {
        let mut a = self.clone();
        let bpb = usize::from(a.beats_per_bar.max(1));
        let phase = usize::from(a.downbeat_phase) % bpb;
        match edit {
            TempoEdit::Double => {
                if a.beats.len() as f64 * 2.0 > MAX_GRID_BEATS {
                    return None;
                }
                let mut beats = Vec::with_capacity(a.beats.len() * 2);
                for w in a.beats.windows(2) {
                    beats.push(w[0]);
                    beats.push(0.5 * (w[0] + w[1]));
                }
                beats.extend(a.beats.last());
                a.beats = beats;
                a.bpm *= 2.0;
                a.downbeat_phase = u8::try_from((2 * phase) % bpb).ok()?;
            }
            TempoEdit::Half => {
                let start = phase % 2;
                a.beats = a.beats.iter().skip(start).step_by(2).copied().collect();
                a.bpm /= 2.0;
                a.downbeat_phase = u8::try_from(((phase - start) / 2) % bpb).ok()?;
            }
            TempoEdit::Nudge(delta) => {
                if !delta.is_finite() || delta.abs() > 0.25 {
                    return None;
                }
                let before = a.beats.len();
                a.beats = a.beats.iter().map(|b| b + delta).filter(|b| *b >= 0.0).collect();
                // Keep bar alignment when the first beat fell off the start.
                let dropped = before - a.beats.len();
                a.downbeat_phase = u8::try_from((phase + bpb - dropped % bpb) % bpb).ok()?;
            }
            TempoEdit::DownbeatAt(t) => {
                if !t.is_finite() || a.beats.is_empty() {
                    return None;
                }
                let k = a.beats.iter().enumerate().min_by(|x, y| (x.1 - t).abs().total_cmp(&(y.1 - t).abs()))?.0;
                a.downbeat_phase = u8::try_from(k % bpb).ok()?;
            }
            TempoEdit::SetBpm { bpm, anchor } => {
                if !bpm.is_finite() || !(20.0..=400.0).contains(&bpm) || !anchor.is_finite() || a.beats.len() < 2 {
                    return None;
                }
                let (i, &at) =
                    a.beats.iter().enumerate().min_by(|x, y| (x.1 - anchor).abs().total_cmp(&(y.1 - anchor).abs()))?;
                let bar_pos = (i + bpb - phase) % bpb; // position of the anchor beat inside its bar
                let period = 60.0 / bpm;
                let (first, last) = (a.beats[0], a.beats[a.beats.len() - 1]);
                if (last - first) / period > MAX_GRID_BEATS {
                    return None;
                }
                #[allow(clippy::cast_possible_truncation)]
                let (k0, k1) = (((first - at) / period).ceil() as i64, ((last - at) / period).floor() as i64);
                a.beats = (k0..=k1).map(|k| at + k as f64 * period).filter(|t| *t >= 0.0).collect();
                let j = a.beats.iter().position(|b| (b - at).abs() < 1e-9)?;
                a.downbeat_phase = u8::try_from((j + bpb - bar_pos) % bpb).ok()?;
                a.bpm = bpm;
                a.variable_tempo = false;
            }
            TempoEdit::Tap(taps) => {
                let taps: Vec<f64> = taps.into_iter().filter(|t| t.is_finite() && *t >= 0.0).take(64).collect();
                if taps.len() < 4 || a.beats.len() < 2 {
                    return None;
                }
                let current = (a.beats[a.beats.len() - 1] - a.beats[0]) / (a.beats.len() - 1) as f64;
                // Tapped tempo from consecutive intervals (median is robust to a stray tap).
                let mut d: Vec<f64> = taps.windows(2).map(|w| w[1] - w[0]).filter(|x| *x > 0.15).collect();
                d.sort_by(f64::total_cmp);
                let tapped = *d.get(d.len() / 2)?;
                let period = if (tapped / current - 1.0).abs() < 0.06 { current } else { tapped };
                // Phase: circular mean of the taps modulo the period.
                let (sx, sy) = taps.iter().fold((0.0, 0.0), |(x, y), t| {
                    let ang = std::f64::consts::TAU * (t / period);
                    (x + ang.cos(), y + ang.sin())
                });
                let phase = sy.atan2(sx).rem_euclid(std::f64::consts::TAU) / std::f64::consts::TAU * period;
                let (first, last) = (a.beats[0], a.beats[a.beats.len() - 1]);
                if (last - first) / period > MAX_GRID_BEATS {
                    return None;
                }
                #[allow(clippy::cast_possible_truncation)]
                let (k0, k1) = (((first - phase) / period).ceil() as i64, ((last - phase) / period).floor() as i64);
                a.beats = (k0..=k1).map(|k| phase + k as f64 * period).filter(|t| *t >= 0.0).collect();
                a.bpm = 60.0 / period;
                a.variable_tempo = false;
                // The tap closest to the start of the tapping is beat 1.
                let k = a
                    .beats
                    .iter()
                    .enumerate()
                    .min_by(|x, y| (x.1 - taps[0]).abs().total_cmp(&(y.1 - taps[0]).abs()))?
                    .0;
                a.downbeat_phase = u8::try_from(k % bpb).ok()?;
            }
            TempoEdit::BeatsPerBar(n) => {
                if !(2..=12).contains(&n) {
                    return None;
                }
                a.beats_per_bar = n;
                a.downbeat_phase = u8::try_from(phase % usize::from(n)).ok()?;
            }
        }
        (a.beats.len() >= 2 && (20.0..=400.0).contains(&a.bpm) && a.beats.windows(2).all(|w| w[1] > w[0])).then_some(a)
    }
}

/// Largest beat grid an edit may produce (≈ 14 h at 120 BPM).
const MAX_GRID_BEATS: f64 = 100_000.0;

/// Onset envelopes sampled at `fps` frames per second.
struct Onsets {
    fps: f64,
    /// Full-band onset strength (normalised).
    full: Vec<f32>,
    /// Low-band onset strength (raw).
    low: Vec<f32>,
    /// Raw full-band flux used for activity detection.
    raw: Vec<f32>,
}

/// Diagnostic view of the analysis (used by `examples/tempo_debug.rs`).
#[doc(hidden)]
#[must_use]
pub fn debug_windows(audio: &StereoBuffer) -> Vec<(f64, f64, f32)> {
    let Some(on) = onset_envelopes(audio) else { return Vec::new() };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (win, hop) = ((LOCAL_WINDOW_SECS * on.fps) as usize, (LOCAL_HOP_SECS * on.fps) as usize);
    let mut out = Vec::new();
    let mut start = 0;
    while start + win <= on.full.len() {
        if let Some((p, c)) = estimate_period(&on.full[start..start + win], on.fps) {
            out.push((start as f64 / on.fps, 60.0 * on.fps / p, c));
        }
        start += hop;
    }
    if let Some((p, c)) = estimate_period(&on.full, on.fps) {
        out.insert(0, (-1.0, 60.0 * on.fps / p, c));
    }
    out
}

/// Diagnostic: full/low onset energy folded over one period (`bins` bins)
/// starting at `t0` seconds.
#[doc(hidden)]
#[must_use]
pub fn debug_fold(audio: &StereoBuffer, period_secs: f64, t0: f64, bins: usize) -> Vec<(f32, f32)> {
    let Some(on) = onset_envelopes(audio) else { return Vec::new() };
    let mut acc = vec![(0.0f32, 0.0f32); bins];
    for (i, (f, l)) in on.full.iter().zip(&on.low).enumerate() {
        let t = i as f64 / on.fps - t0;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let b = ((t / period_secs).rem_euclid(1.0) * bins as f64) as usize % bins;
        acc[b].0 += f;
        acc[b].1 += l;
    }
    acc
}

/// Analyses tempo and beats of a buffer.
#[must_use]
pub fn analyze(audio: &StereoBuffer) -> BeatAnalysis {
    analyze_with(audio, TempoMode::Auto)
}

/// How the tempo is modelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TempoMode {
    /// Constant unless the song clearly changes tempo (default).
    Auto,
    /// Force one tempo for the whole song.
    Constant,
    /// Follow the band beat by beat (rubato, live recordings without click).
    Variable,
}

/// Like [`analyze`], with the tempo model chosen by the user.
#[must_use]
pub fn analyze_with(audio: &StereoBuffer, mode: TempoMode) -> BeatAnalysis {
    let Some(onsets) = onset_envelopes(audio) else { return BeatAnalysis::empty() };
    let Some((period, confidence)) = estimate_period(&onsets.full, onsets.fps) else {
        return BeatAnalysis::empty();
    };
    let fps = onsets.fps;
    let periods = match mode {
        TempoMode::Auto => local_periods(&onsets.full, fps, period),
        // Tightness of the tracker still keeps beats near the global period.
        TempoMode::Constant | TempoMode::Variable => vec![period; onsets.full.len()],
    };
    let variable_tempo = match mode {
        TempoMode::Auto => periods.iter().any(|p| (p / period - 1.0).abs() > 1e-9),
        TempoMode::Constant => false,
        TempoMode::Variable => true,
    };
    let frames = track_beats(&onsets.full, &periods);
    let frames = fix_offbeat_phase(frames, &onsets.low, period);
    let frames = trim_to_activity(frames, &onsets.raw, period);
    if frames.len() < 2 {
        return BeatAnalysis { confidence, ..BeatAnalysis::empty() };
    }
    let tracked: Vec<f64> = frames.iter().map(|&f| f as f64 / fps).collect();
    let (beats, bpm) = if variable_tempo {
        let bpm = refine_bpm(&tracked).unwrap_or(60.0 * fps / period);
        (tracked, bpm)
    } else {
        // One tempo for the whole song: a perfectly even grid.
        let (grid, p) = constant_grid(&tracked, &onsets, period / fps);
        if grid.len() >= 2 { (grid, 60.0 / p) } else { (tracked, 60.0 / p) }
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let beat_frames: Vec<usize> = beats.iter().map(|b| (b * fps).round().max(0.0) as usize).collect();
    let downbeat_phase = downbeat_phase(&beat_frames, &onsets.low, 4);
    BeatAnalysis { bpm, beats, beats_per_bar: 4, downbeat_phase, confidence, variable_tempo }
}

/// Least-squares line `t = t0 + k·period` through `(k, t)` points.
fn fit_line(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    let n = points.len() as f64;
    if points.len() < 2 {
        return None;
    }
    let mk = points.iter().map(|p| p.0).sum::<f64>() / n;
    let mt = points.iter().map(|p| p.1).sum::<f64>() / n;
    let (mut num, mut den) = (0.0, 0.0);
    for (k, t) in points {
        num += (k - mk) * (t - mt);
        den += (k - mk) * (k - mk);
    }
    (den > 0.0).then(|| (mt - num / den * mk, num / den))
}

/// Builds a constant-tempo grid over the tracked span. Returns `(grid, period_secs)`.
///
/// 1. Every tracked beat gets its beat *number* by rounding to the expected
///    period, so dropped or doubled beats from the tracker cannot bend the
///    tempo; a robust line fit (outliers removed) gives period and phase.
/// 2. Period and phase are then fine-tuned so the grid lands on the
///    strongest onsets across the entire song.
fn constant_grid(tracked: &[f64], onsets: &Onsets, period_guess: f64) -> (Vec<f64>, f64) {
    let fps = onsets.fps;
    let (first, last) = (tracked[0], tracked[tracked.len() - 1]);
    let (mut t0, mut period) = (first, period_guess);
    for _ in 0..4 {
        let points: Vec<(f64, f64)> = tracked
            .iter()
            .map(|t| (((t - t0) / period).round(), *t))
            .filter(|(k, t)| (t - (t0 + k * period)).abs() < 0.2 * period)
            .collect();
        match fit_line(&points) {
            Some((a, p)) if p.is_finite() && (p / period_guess - 1.0).abs() < 0.1 => (t0, period) = (a, p),
            _ => break,
        }
    }

    // Full-band onsets only: kicks are often syncopated (on the "e" or "and"),
    // so weighting the low band would pull the grid off the beat, while
    // snare + cymbal accents land on it.
    let smooth = gaussian_smooth(&onsets.full, 1.0);
    let sample = |t: f64| -> f32 {
        let x = t * fps;
        if x < 0.0 {
            return 0.0;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let i = x as usize;
        #[allow(clippy::cast_possible_truncation)]
        let frac = (x - i as f64) as f32;
        let a = smooth.get(i).copied().unwrap_or(0.0);
        let b = smooth.get(i + 1).copied().unwrap_or(0.0);
        a + (b - a) * frac
    };
    let score = |t0: f64, p: f64| -> f32 {
        #[allow(clippy::cast_possible_truncation)]
        let (k0, k1) = (((first - t0) / p).round() as i64, ((last - t0) / p).round() as i64);
        (k0..=k1).map(|k| sample(t0 + k as f64 * p)).sum()
    };
    // Phase over a whole beat (the tracker may sit on an off-beat), then
    // period and phase refined together, coarse to fine.
    let mut best = (t0, period, score(t0, period));
    for oi in 0..100 {
        let t = t0 + f64::from(oi) * 0.01 * period;
        let sc = score(t, period);
        if sc > best.2 {
            best = (t, period, sc);
        }
    }
    for (span, steps, phase_span) in [(0.01, 40, 0.06), (0.001, 20, 0.02)] {
        let (bt0, bp, _) = best;
        for pi in -steps..=steps {
            let p = bp * (1.0 + span * f64::from(pi) / f64::from(steps));
            for oi in -10..=10 {
                let t = bt0 + f64::from(oi) * 0.1 * phase_span * bp;
                let sc = score(t, p);
                if sc > best.2 {
                    best = (t, p, sc);
                }
            }
        }
    }
    let (t0, period, _) = best;
    #[allow(clippy::cast_possible_truncation)]
    let (k0, k1) = (((first - t0) / period).round() as i64, ((last - t0) / period).round() as i64);
    let grid: Vec<f64> = (k0..=k1).map(|k| t0 + k as f64 * period).filter(|t| *t >= 0.0).collect();
    (grid, period)
}

fn onset_envelopes(audio: &StereoBuffer) -> Option<Onsets> {
    let sr = audio.sample_rate() as usize;
    let decim = (sr / 22_050).max(1);
    let mono: Vec<f32> = audio.to_mono().chunks(decim).map(|c| c.iter().sum::<f32>() / c.len() as f32).collect();
    let sr_d = sr as f32 / decim as f32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = ((sr_d * 0.046) as usize).next_power_of_two().clamp(256, 4096);
    let hop = n / 4;
    if mono.len() < n * 4 {
        return None;
    }
    let fps = f64::from(sr_d) / hop as f64;
    let window = hann(n);
    let mut stft = Stft::new(n);
    let bins = stft.bins();
    let bands = log_bands(bins, n, sr_d);
    let low_bands = bands.iter().filter(|(_, hi)| (*hi as f32) * sr_d / n as f32 <= LOW_BAND_HZ).count().max(1);

    let frames = mono.len() / hop + 1;
    let mut frame = vec![0.0; n];
    let mut spec = vec![Complex32::default(); bins];
    let mut prev = vec![0.0f32; bands.len()];
    let mut cur = vec![0.0f32; bands.len()];
    let mut raw = Vec::with_capacity(frames);
    let mut low = Vec::with_capacity(frames);
    for t in 0..frames {
        let start = (t * hop) as isize - (n / 2) as isize;
        windowed_frame(&mono, start, &window, &mut frame);
        stft.forward(&frame, &mut spec);
        // Log-spaced bands weight every octave equally, so a kick drum (a few
        // low bins) counts as much as a hi-hat (hundreds of high bins).
        for (c, (lo, hi)) in cur.iter_mut().zip(&bands) {
            let mean = spec[*lo..*hi].iter().map(|s| s.norm()).sum::<f32>() / (hi - lo) as f32;
            *c = (1.0 + 100.0 * mean).ln();
        }
        let flux = |range: std::ops::Range<usize>| -> f32 {
            cur[range.clone()].iter().zip(&prev[range]).map(|(c, p)| (c - p).max(0.0)).sum()
        };
        let f = if t == 0 { 0.0 } else { flux(0..bands.len()) };
        let l = if t == 0 { 0.0 } else { flux(0..low_bands) };
        raw.push(f);
        low.push(l);
        std::mem::swap(&mut prev, &mut cur);
    }

    // Adaptive detrend: subtract a ~0.5 s moving average, half-wave rectify.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let w = ((fps * 0.25) as usize).max(1);
    let avg = moving_average(&raw, w);
    let mut full: Vec<f32> = raw.iter().zip(&avg).map(|(r, a)| (r - a).max(0.0)).collect();
    let mean = full.iter().sum::<f32>() / full.len() as f32;
    let std = (full.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / full.len() as f32).sqrt();
    if std < 1e-6 {
        return None;
    }
    for x in &mut full {
        *x /= std;
    }
    Some(Onsets { fps, full, low, raw })
}

/// Number of log-spaced analysis bands.
const BANDS: usize = 40;

/// Bin ranges `[lo, hi)` of log-spaced bands between 30 Hz and 11 kHz.
fn log_bands(bins: usize, n: usize, sr: f32) -> Vec<(usize, usize)> {
    let (f_lo, f_hi) = (30.0f32, (sr / 2.0).min(11_000.0));
    let to_bin = |f: f32| -> usize {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let b = (f * n as f32 / sr).round() as usize;
        b.clamp(1, bins - 1)
    };
    let mut out: Vec<(usize, usize)> = Vec::with_capacity(BANDS);
    let mut lo = to_bin(f_lo);
    for i in 1..=BANDS {
        let f = f_lo * (f_hi / f_lo).powf(i as f32 / BANDS as f32);
        let hi = to_bin(f).max(lo + 1).min(bins);
        if hi > lo {
            out.push((lo, hi));
            lo = hi;
        }
    }
    out
}

/// Moves the grid by half a beat when the low end (kick/bass) clearly hits
/// between the tracked beats — the classic "locked to the hi-hat" error.
fn fix_offbeat_phase(beats: Vec<usize>, low: &[f32], period: f64) -> Vec<usize> {
    let strength = |f: usize| -> f32 {
        let lo = f.saturating_sub(2);
        let hi = (f + 3).min(low.len());
        low.get(lo..hi).map_or(0.0, |s| s.iter().copied().fold(0.0, f32::max))
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let half = (period / 2.0).round() as usize;
    let on: f32 = beats.iter().map(|b| strength(*b)).sum();
    let off: f32 = beats.iter().map(|b| strength(b + half)).sum();
    if off > on * 1.5 && off > 0.0 {
        beats.into_iter().map(|b| b + half).filter(|b| *b < low.len()).collect()
    } else {
        beats
    }
}

/// Centred moving average with half-width `w`.
fn moving_average(x: &[f32], w: usize) -> Vec<f32> {
    let mut prefix = Vec::with_capacity(x.len() + 1);
    prefix.push(0.0f64);
    for v in x {
        prefix.push(prefix.last().copied().unwrap_or(0.0) + f64::from(*v));
    }
    (0..x.len())
        .map(|i| {
            let lo = i.saturating_sub(w);
            let hi = (i + w + 1).min(x.len());
            #[allow(clippy::cast_possible_truncation)]
            let v = ((prefix[hi] - prefix[lo]) / (hi - lo) as f64) as f32;
            v
        })
        .collect()
}

/// Returns `(period_in_frames, confidence)`.
fn estimate_period(env: &[f32], fps: f64) -> Option<(f64, f32)> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let lag_min = ((60.0 * fps / MAX_BPM).floor() as usize).max(1);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let lag_max = ((60.0 * fps / MIN_BPM).ceil() as usize).min(env.len().saturating_sub(1));
    if lag_max <= lag_min + 2 {
        return None;
    }
    // Smooth narrow onset peaks so non-integer periods still correlate.
    let env = &gaussian_smooth(env, 1.5);
    let ac = |lag: usize| -> f64 {
        let n = env.len() - lag;
        env[..n].iter().zip(&env[lag..]).map(|(a, b)| f64::from(*a) * f64::from(*b)).sum::<f64>() / n as f64
    };
    let zero = ac(0);
    if zero <= 0.0 {
        return None;
    }
    let scores: Vec<f64> = (0..=lag_max + 1)
        .map(|lag| {
            if lag < lag_min || lag > lag_max {
                return 0.0;
            }
            let bpm = 60.0 * fps / lag as f64;
            let prior = (-0.5 * ((bpm / PRIOR_BPM).log2() / PRIOR_OCTAVES).powi(2)).exp();
            ac(lag).max(0.0) * prior
        })
        .collect();
    let (best, &best_score) = scores.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1))?;
    if best_score <= 0.0 {
        return None;
    }
    // Parabolic interpolation for sub-frame precision.
    let (a, b, c) = (scores[best - 1], scores[best], scores[best + 1]);
    let denom = a - 2.0 * b + c;
    let offset = if denom.abs() > 1e-12 { (0.5 * (a - c) / denom).clamp(-0.5, 0.5) } else { 0.0 };
    #[allow(clippy::cast_possible_truncation)]
    let confidence = (ac(best) / zero).clamp(0.0, 1.0) as f32;
    Some((best as f64 + offset, confidence))
}

/// Convolves with a Gaussian of standard deviation `sigma` (frames).
fn gaussian_smooth(x: &[f32], sigma: f64) -> Vec<f32> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let radius = (sigma * 3.0).ceil() as usize;
    let kernel: Vec<f32> = (0..=2 * radius)
        .map(|i| {
            let d = i as f64 - radius as f64;
            #[allow(clippy::cast_possible_truncation)]
            let v = (-0.5 * (d / sigma).powi(2)).exp() as f32;
            v
        })
        .collect();
    (0..x.len())
        .map(|t| {
            kernel
                .iter()
                .enumerate()
                .filter_map(|(i, k)| (t + i).checked_sub(radius).and_then(|j| x.get(j)).map(|e| e * k))
                .sum()
        })
        .collect()
}

/// Window used to measure the local tempo (seconds) and its hop.
const LOCAL_WINDOW_SECS: f64 = 10.0;
const LOCAL_HOP_SECS: f64 = 5.0;

/// Ratios between metrical levels of one tempo (half/double time, triplet
/// and dotted feels).
const METRIC_RATIOS: [f64; 9] = [1.0, 2.0, 0.5, 1.5, 2.0 / 3.0, 0.75, 4.0 / 3.0, 3.0, 1.0 / 3.0];

/// How strongly a window still pulses at the global period (or a multiple of
/// it, e.g. one hit per bar in a breakdown): best normalised autocorrelation
/// at those lags relative to the window's strongest periodicity.
fn pulse_support(window: &[f32], global: f64) -> f64 {
    let ac = |lag: usize| -> f64 {
        if lag == 0 || lag >= window.len() {
            return 0.0;
        }
        let n = window.len() - lag;
        window[..n].iter().zip(&window[lag..]).map(|(a, b)| f64::from(*a) * f64::from(*b)).sum::<f64>() / n as f64
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let lag = |m: f64| (global * m).round() as usize;
    let max_lag = lag(4.5).min(window.len().saturating_sub(1));
    let strongest = (lag(0.4).max(1)..=max_lag).map(ac).fold(0.0, f64::max);
    if strongest <= 0.0 {
        return 1.0;
    }
    let at_pulse = [1.0, 2.0, 4.0].into_iter().map(|m| {
        let l = lag(m);
        ac(l.saturating_sub(1)).max(ac(l)).max(ac(l + 1))
    });
    at_pulse.fold(0.0, f64::max) / strongest
}

/// Per-frame beat period. Constant (= `global`) unless windowed tempo
/// estimates clearly disagree with the global one, i.e. the song really
/// changes tempo. Octave flips of a window (×2, ×½) are folded back.
fn local_periods(env: &[f32], fps: f64, global: f64) -> Vec<f64> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (win, hop) = ((LOCAL_WINDOW_SECS * fps) as usize, (LOCAL_HOP_SECS * fps) as usize);
    let constant = vec![global; env.len()];
    if env.len() < win * 2 || hop == 0 {
        return constant;
    }
    let smooth = gaussian_smooth(env, 1.5);
    let mut centres = Vec::new();
    let mut estimates = Vec::new();
    let mut changed = Vec::new();
    let mut start = 0;
    while start + win <= env.len() {
        let window = &env[start..start + win];
        if let Some((p, confidence)) = estimate_period(window, fps) {
            // Riffs, half-time breakdowns and triplet feels show up as simple
            // ratios of the song's pulse; fold them back instead of treating
            // them as a new tempo.
            let metric = METRIC_RATIOS.iter().find(|r| ((p / global) / *r - 1.0).abs() < 0.025);
            let keeps_pulse = pulse_support(&smooth[start..start + win], global) >= 0.6;
            let is_change = metric.is_none() && !keeps_pulse && confidence >= 0.3;
            centres.push(start + win / 2);
            estimates.push(if is_change { p } else { global });
            changed.push(is_change);
        }
        start += hop;
    }
    // A real tempo change lasts: at least 3 consecutive windows (≥ 20 s).
    let longest_run = changed
        .iter()
        .fold((0usize, 0usize), |(run, best), c| if *c { (run + 1, best.max(run + 1)) } else { (0, best) })
        .1;
    if estimates.len() < 3 || longest_run < 3 {
        return constant;
    }
    // Median-smooth the window estimates, then hold/interpolate per frame.
    let smoothed: Vec<f64> = (0..estimates.len())
        .map(|i| {
            let mut w: Vec<f64> = estimates[i.saturating_sub(1)..(i + 2).min(estimates.len())].to_vec();
            w.sort_by(f64::total_cmp);
            w[w.len() / 2]
        })
        .collect();
    (0..env.len())
        .map(|t| {
            let i = centres.partition_point(|c| *c <= t);
            match (i.checked_sub(1), centres.get(i)) {
                (Some(a), Some(&cb)) => {
                    let ca = centres[a];
                    let f = (t - ca) as f64 / (cb - ca).max(1) as f64;
                    smoothed[a] + (smoothed[i] - smoothed[a]) * f
                }
                (Some(a), None) => smoothed[a],
                (None, _) => smoothed[0],
            }
        })
        .collect()
}

/// Dynamic-programming beat tracker; returns beat frame indices.
fn track_beats(env: &[f32], periods: &[f64]) -> Vec<usize> {
    let t_len = env.len().min(periods.len());
    let mean_period = periods.iter().sum::<f64>() / periods.len().max(1) as f64;
    let local = gaussian_smooth(env, (mean_period / 32.0).max(0.5));

    let mut cum = vec![0.0f32; t_len];
    let mut back: Vec<Option<usize>> = vec![None; t_len];
    for t in 0..t_len {
        let period = periods[t];
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (min_back, max_back) = (((period / 2.0).round() as usize).max(1), (period * 2.0).round() as usize);
        let mut best: Option<(f32, usize)> = None;
        for d in min_back..=max_back.min(t) {
            let tau = t - d;
            #[allow(clippy::cast_possible_truncation)]
            let penalty = TIGHTNESS * ((d as f64 / period).ln().powi(2)) as f32;
            let s = cum[tau] - penalty;
            if best.is_none_or(|(b, _)| s > b) {
                best = Some((s, tau));
            }
        }
        match best {
            Some((s, tau)) if s > 0.0 => {
                cum[t] = local[t] + s;
                back[t] = Some(tau);
            }
            _ => cum[t] = local[t],
        }
    }
    let period = periods.last().copied().unwrap_or(mean_period);

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let tail = (period.round() as usize).clamp(1, t_len);
    let Some(mut t) = (t_len - tail..t_len).max_by(|a, b| cum[*a].total_cmp(&cum[*b])) else {
        return Vec::new();
    };
    let mut beats = vec![t];
    while let Some(prev) = back[t] {
        beats.push(prev);
        t = prev;
    }
    beats.reverse();
    beats
}

/// Drops beats in the silent intro/outro (keeps half a period of slack).
fn trim_to_activity(beats: Vec<usize>, raw: &[f32], period: f64) -> Vec<usize> {
    let peak = raw.iter().copied().fold(0.0f32, f32::max);
    if peak <= 0.0 {
        return Vec::new();
    }
    let threshold = peak * 0.1;
    let first = raw.iter().position(|v| *v > threshold).unwrap_or(0);
    let last = raw.iter().rposition(|v| *v > threshold).unwrap_or(raw.len());
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let slack = (period * 0.5) as usize;
    beats.into_iter().filter(|b| *b + slack >= first && *b <= last + slack).collect()
}

/// BPM from a least-squares fit of beat time against beat index. Unlike a
/// median of intervals it is not limited by the frame resolution.
fn refine_bpm(beats: &[f64]) -> Option<f64> {
    if beats.len() < 8 {
        return None;
    }
    let n = beats.len() as f64;
    let mean_i = (n - 1.0) / 2.0;
    let mean_t = beats.iter().sum::<f64>() / n;
    let (mut num, mut den) = (0.0, 0.0);
    for (i, t) in beats.iter().enumerate() {
        let di = i as f64 - mean_i;
        num += di * (t - mean_t);
        den += di * di;
    }
    let period = num / den;
    (period > 0.0).then(|| 60.0 / period)
}

fn downbeat_phase(beats: &[usize], low: &[f32], beats_per_bar: usize) -> u8 {
    let strength = |f: usize| -> f32 {
        let lo = f.saturating_sub(2);
        let hi = (f + 3).min(low.len());
        low.get(lo..hi).map_or(0.0, |s| s.iter().copied().fold(0.0, f32::max))
    };
    let mut sums = vec![0.0f32; beats_per_bar];
    for (i, b) in beats.iter().enumerate() {
        sums[i % beats_per_bar] += strength(*b);
    }
    let best = sums.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).map_or(0, |(i, _)| i);
    u8::try_from(best).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 44_100;

    /// Synthesises a click track; beat 0 of each bar gets a low "kick".
    fn click_track(bpm: f64, secs: f64, lead_in: f64, kick_on: usize) -> (StereoBuffer, Vec<f64>) {
        let frames = (secs * f64::from(SR)) as usize;
        let mut x = vec![0.0f32; frames];
        let mut truth = Vec::new();
        let mut t = lead_in;
        let mut i = 0usize;
        while t < secs - 0.1 {
            truth.push(t);
            let start = (t * f64::from(SR)) as usize;
            let kick = i % 4 == kick_on;
            for j in 0..(0.08 * f64::from(SR)) as usize {
                let tt = j as f32 / SR as f32;
                let env = (-tt * 60.0).exp();
                let hi = ((j as u32).wrapping_mul(2_654_435_761) >> 16) as f32 / 32_768.0 - 1.0;
                let mut s = 0.3 * hi * (-tt * 300.0).exp();
                if kick {
                    s += 0.9 * (std::f32::consts::TAU * 55.0 * tt).sin() * env;
                }
                if let Some(v) = x.get_mut(start + j) {
                    *v += s;
                }
            }
            t += 60.0 / bpm;
            i += 1;
        }
        (StereoBuffer::from_mono(SR, x).unwrap(), truth)
    }

    fn assert_tracks(bpm: f64) {
        let (audio, truth) = click_track(bpm, 20.0, 0.5, 0);
        let a = analyze(&audio);
        assert!((a.bpm - bpm).abs() < 1.5, "expected {bpm} got {}", a.bpm);
        assert!(a.confidence > 0.1);
        // Every detected beat must sit within 30 ms of a true beat.
        let worst =
            a.beats.iter().map(|b| truth.iter().map(|t| (t - b).abs()).fold(f64::MAX, f64::min)).fold(0.0, f64::max);
        assert!(worst < 0.03, "beat error {worst}s at {bpm} BPM");
        assert!(a.beats.len() + 2 >= truth.len(), "missed beats: {} vs {}", a.beats.len(), truth.len());
    }

    #[test]
    fn detects_common_tempos() {
        for bpm in [72.0, 90.0, 120.0, 128.0, 150.0] {
            assert_tracks(bpm);
        }
    }

    #[test]
    fn finds_the_downbeat() {
        for kick_on in 0..4 {
            let (audio, truth) = click_track(110.0, 20.0, 0.3, kick_on);
            let a = analyze(&audio);
            let first_downbeat = a.beats.iter().enumerate().find(|(i, _)| a.is_downbeat(*i)).map(|(_, b)| *b).unwrap();
            let truth_idx = truth.iter().position(|t| (t - first_downbeat).abs() < 0.05).unwrap();
            assert_eq!(truth_idx % 4, kick_on, "downbeat misplaced");
        }
    }

    #[test]
    fn locks_to_the_kick_not_the_offbeat_hihat() {
        let sr = SR as usize;
        let period = 0.6; // 100 BPM
        let x: Vec<f32> = (0..sr * 20)
            .map(|i| {
                let t = i as f32 / SR as f32;
                let on = t % period;
                let off = (t + period / 2.0) % period;
                let noise = ((i as u32).wrapping_mul(2_654_435_761) >> 16) as f32 / 32_768.0 - 1.0;
                0.7 * (std::f32::consts::TAU * 55.0 * t).sin() * (-on * 18.0).exp() + 0.35 * noise * (-off * 90.0).exp()
            })
            .collect();
        let a = analyze(&StereoBuffer::from_mono(SR, x).unwrap());
        assert!((a.bpm - 100.0).abs() < 1.0, "bpm {}", a.bpm);
        let phase_err = a.beats.iter().map(|b| {
            let r = b % f64::from(period);
            r.min(f64::from(period) - r)
        });
        let worst = phase_err.fold(0.0, f64::max);
        assert!(worst < 0.04, "beats should sit on the kick, worst offset {worst}s");
    }

    /// Click track whose hits wander by up to ±`jitter` seconds, like a human drummer.
    fn humanised(bpm: f64, secs: f64, jitter: f64) -> StereoBuffer {
        let mut x = vec![0.0f32; (secs * f64::from(SR)) as usize];
        let mut t = 0.5;
        let mut i = 0u32;
        while t < secs - 0.2 {
            let offset = jitter * ((f64::from(i.wrapping_mul(2_654_435_761) % 1000) / 500.0) - 1.0);
            let start = ((t + offset) * f64::from(SR)) as usize;
            for j in 0..2000 {
                let tt = j as f32 / SR as f32;
                if let Some(v) = x.get_mut(start + j) {
                    *v += 0.8 * (std::f32::consts::TAU * 80.0 * tt).sin() * (-tt * 40.0).exp();
                }
            }
            t += 60.0 / bpm;
            i += 1;
        }
        StereoBuffer::from_mono(SR, x).unwrap()
    }

    #[test]
    fn humanised_playing_still_gives_a_perfectly_steady_click() {
        let a = analyze(&humanised(112.0, 40.0, 0.015));
        assert!(!a.variable_tempo, "constant song must use a constant grid");
        assert!((a.bpm - 112.0).abs() < 0.3, "bpm {}", a.bpm);
        let d: Vec<f64> = a.beats.windows(2).map(|w| w[1] - w[0]).collect();
        let spread = d.iter().copied().fold(f64::MIN, f64::max) - d.iter().copied().fold(f64::MAX, f64::min);
        assert!(spread < 1e-9, "grid intervals must be identical, spread {spread}");
        assert!(a.beats.len() >= 70, "grid should span the song, got {}", a.beats.len());
    }

    /// Hits at `times` (seconds) with a kick-like body.
    fn hits(times: &[f64], secs: f64) -> StereoBuffer {
        let mut x = vec![0.0f32; (secs * f64::from(SR)) as usize];
        for t in times {
            let start = (t * f64::from(SR)) as usize;
            for j in 0..2000 {
                let tt = j as f32 / SR as f32;
                if let Some(v) = x.get_mut(start + j) {
                    *v += 0.8 * (std::f32::consts::TAU * 80.0 * tt).sin() * (-tt * 40.0).exp();
                }
            }
        }
        StereoBuffer::from_mono(SR, x).unwrap()
    }

    #[test]
    fn breakdowns_and_dotted_feels_do_not_change_the_tempo() {
        // 120 BPM verse, a 20 s breakdown hitting dotted quarters (×2/3 feel),
        // then the verse again — like a rock/metal song.
        let p = 0.5;
        let mut times: Vec<f64> = (0..40).map(|k| 0.5 + f64::from(k) * p).collect();
        let b0 = times[times.len() - 1] + p;
        times.extend((0..27).map(|k| b0 + f64::from(k) * p * 1.5));
        let c0 = b0 + 40.0 * p;
        times.extend((0..40).map(|k| c0 + f64::from(k) * p));
        let a = analyze(&hits(&times, c0 + 21.0));
        assert!(!a.variable_tempo, "a breakdown is not a tempo change");
        assert!((a.bpm - 120.0).abs() < 0.3, "bpm {}", a.bpm);
        // Grid lines sit on the verse hits.
        let off = |t: f64| a.beats.iter().map(|b| (b - t).abs()).fold(f64::MAX, f64::min);
        assert!(times[..40].iter().chain(&times[67..]).all(|t| off(*t) < 0.03));
    }

    #[test]
    fn real_tempo_changes_are_followed() {
        // 20 s at 90 BPM, then 20 s at 130 BPM.
        let a = humanised(90.0, 20.0, 0.0);
        let b = humanised(130.0, 20.0, 0.0);
        let (_, mut l, _) = a.into_parts();
        l.extend_from_slice(b.left());
        let analysis = analyze(&StereoBuffer::from_mono(SR, l).unwrap());
        assert!(analysis.variable_tempo, "tempo change must be detected");
        let early: Vec<f64> = analysis.beats.windows(2).filter(|w| w[1] < 18.0).map(|w| w[1] - w[0]).collect();
        let late: Vec<f64> = analysis.beats.windows(2).filter(|w| w[0] > 22.0).map(|w| w[1] - w[0]).collect();
        let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
        assert!((60.0 / mean(&early) - 90.0).abs() < 3.0, "early {}", 60.0 / mean(&early));
        assert!((60.0 / mean(&late) - 130.0).abs() < 4.0, "late {}", 60.0 / mean(&late));
    }

    #[test]
    fn manual_tempo_edits() {
        let a = BeatAnalysis {
            bpm: 60.0,
            beats: (0..9).map(f64::from).collect(),
            beats_per_bar: 4,
            downbeat_phase: 1,
            confidence: 1.0,
            variable_tempo: false,
        };
        let d = a.edited(TempoEdit::Double).unwrap();
        assert_eq!(d.bpm, 120.0);
        assert_eq!(d.beats.len(), 17);
        assert!(d.is_downbeat(2), "old downbeat (index 1) is now index 2");
        let h = d.edited(TempoEdit::Half).unwrap();
        assert_eq!(h.beats, a.beats);
        assert_eq!(h.bpm, 60.0);
        assert!(h.is_downbeat(1));

        let n = a.edited(TempoEdit::Nudge(0.02)).unwrap();
        assert!((n.beats[0] - 0.02).abs() < 1e-12);
        let back = a.edited(TempoEdit::Nudge(-0.1)).unwrap();
        assert_eq!(back.beats.len(), 8, "beat before 0 dropped");
        assert!(back.is_downbeat(0), "bar alignment kept");
        assert!(a.edited(TempoEdit::Nudge(5.0)).is_none());

        let b = a.edited(TempoEdit::DownbeatAt(6.1)).unwrap();
        assert!(b.is_downbeat(6) && b.is_downbeat(2));
        assert_eq!(a.edited(TempoEdit::BeatsPerBar(3)).unwrap().beats_per_bar, 3);
        assert!(a.edited(TempoEdit::BeatsPerBar(1)).is_none());
        // Tap along 0.3 s late at the same tempo → grid shifts, BPM kept.
        let taps: Vec<f64> = (2..8).map(|k| f64::from(k) + 0.3).collect();
        let t = a.edited(TempoEdit::Tap(taps)).unwrap();
        assert!((t.bpm - 60.0).abs() < 1e-9);
        assert!(t.beats.iter().all(|b| {
            let r = (b - 0.3).rem_euclid(1.0);
            r.min(1.0 - r) < 1e-6
        }));
        assert!(t.is_downbeat(t.beats.iter().position(|b| (b - 2.3).abs() < 1e-6).unwrap()));
        // Tapping a clearly different tempo adopts it.
        let taps: Vec<f64> = (0..8).map(|k| 1.0 + f64::from(k) * 0.5).collect();
        assert!((a.edited(TempoEdit::Tap(taps)).unwrap().bpm - 120.0).abs() < 1e-6);
        assert!(a.edited(TempoEdit::Tap(vec![1.0, 2.0])).is_none(), "needs ≥ 4 taps");

        // Typed tempo: exact BPM, anchor beat kept, bar position kept.
        let typed = a.edited(TempoEdit::SetBpm { bpm: 64.0, anchor: 5.1 }).unwrap();
        assert_eq!(typed.bpm, 64.0);
        assert!(typed.beats.iter().any(|b| (b - 5.0).abs() < 1e-9), "anchor beat stays put");
        let d: Vec<f64> = typed.beats.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(d.iter().all(|x| (x - 60.0 / 64.0).abs() < 1e-9));
        let k = typed.beats.iter().position(|b| (b - 5.0).abs() < 1e-9).unwrap();
        assert!(typed.is_downbeat(k), "beat 5 was a downbeat (phase 1) and still is");
        assert!(a.edited(TempoEdit::SetBpm { bpm: 1000.0, anchor: 0.0 }).is_none());
        assert!(a.edited(TempoEdit::SetBpm { bpm: f64::NAN, anchor: 0.0 }).is_none());

        let fast = BeatAnalysis { bpm: 300.0, ..a.clone() };
        assert!(fast.edited(TempoEdit::Double).is_none(), "600 BPM rejected");
    }

    #[test]
    fn silence_and_short_inputs_do_not_panic() {
        let a = analyze(&StereoBuffer::silent(SR, SR as usize * 5).unwrap());
        assert!(a.beats.is_empty());
        let a = analyze(&StereoBuffer::silent(SR, 10).unwrap());
        assert!(a.beats.is_empty());
        let a = analyze(&StereoBuffer::from_mono(SR, vec![0.5; 3000]).unwrap());
        assert!(a.beats.len() < 3);
    }

    #[test]
    fn beats_are_strictly_increasing_and_finite() {
        let (audio, _) = click_track(100.0, 12.0, 1.0, 0);
        let a = analyze(&audio);
        assert!(a.beats.windows(2).all(|w| w[1] > w[0]));
        assert!(a.beats.iter().all(|b| b.is_finite() && *b >= 0.0));
        assert!(a.bpm.is_finite());
    }
}
