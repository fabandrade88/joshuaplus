//! Click track synthesised on the fly from detected beats.
//!
//! The click is never rendered to disk: the mixer adds pre-computed click
//! samples at beat positions, so changing the sound, level or count-in is
//! instant and costs no extra storage.

use serde::{Deserialize, Serialize};

use crate::tempo::BeatAnalysis;

/// Selectable metronome voices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClickSound {
    /// Bright two-partial click (the classic metronome).
    #[default]
    Classic,
    /// Hollow wooden knock.
    Woodblock,
    /// Pure digital beep.
    Beep,
    /// Metallic cowbell.
    Cowbell,
    /// Drum sticks / rimshot.
    Sticks,
    /// Kick on beat one, hi-hat on the others.
    Drums,
}

impl ClickSound {
    pub const ALL: [Self; 6] = [Self::Classic, Self::Woodblock, Self::Beep, Self::Cowbell, Self::Sticks, Self::Drums];

    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
}

/// Pre-rendered click sounds at the session sample rate.
#[derive(Debug, Clone, PartialEq)]
pub struct ClickSounds {
    pub accent: Vec<f32>,
    pub regular: Vec<f32>,
}

/// Deterministic white noise (no RNG dependency, identical on every run).
fn noise(i: usize) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    let x = (i as u32).wrapping_mul(2_654_435_761).rotate_left(13).wrapping_mul(1_597_334_677);
    (x >> 8) as f32 / 8_388_608.0 - 1.0
}

/// Renders `secs` of `f(t, i)` at `sr`, normalised to `peak`.
fn render(sr: u32, secs: f32, peak: f32, f: impl Fn(f32, usize) -> f32) -> Vec<f32> {
    let sr_f = sr as f32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let len = (sr_f * secs) as usize;
    let mut out: Vec<f32> = (0..len).map(|i| f(i as f32 / sr_f, i)).collect();
    let max = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if max > 0.0 {
        for s in &mut out {
            *s *= peak / max;
        }
    }
    // 1 ms fade-in avoids a DC step at the onset.
    let fade = (sr / 1000).max(1) as usize;
    for (i, s) in out.iter_mut().take(fade).enumerate() {
        *s *= i as f32 / fade as f32;
    }
    out
}

impl ClickSounds {
    /// Synthesises one voice; accents are higher and louder.
    #[must_use]
    pub fn synth(sample_rate: u32, kind: ClickSound) -> Self {
        let tau = std::f32::consts::TAU;
        let sr = sample_rate;
        let pair = |accent: bool| -> Vec<f32> {
            let (peak, up) = if accent { (0.95, 1.0) } else { (0.6, 0.0) };
            match kind {
                ClickSound::Classic => {
                    let f = if accent { 1_760.0 } else { 1_320.0 };
                    render(sr, 0.06, peak, |t, _| {
                        (-t * 70.0).exp() * (0.8 * (tau * f * t).sin() + 0.2 * (2.0 * tau * f * t).sin())
                    })
                }
                ClickSound::Woodblock => {
                    let f = 800.0 + up * 250.0;
                    render(sr, 0.08, peak, |t, _| {
                        (-t * 60.0).exp() * (tau * f * t).sin() + 0.5 * (-t * 120.0).exp() * (tau * f * 2.76 * t).sin()
                    })
                }
                ClickSound::Beep => {
                    let f = if accent { 2_000.0 } else { 1_000.0 };
                    render(sr, 0.05, peak, |t, _| {
                        let env = if t < 0.04 { 1.0 } else { (1.0 - (t - 0.04) / 0.01).max(0.0) };
                        env * (tau * f * t).sin()
                    })
                }
                ClickSound::Cowbell => {
                    let k = 1.0 + up * 0.12;
                    render(sr, 0.18, peak, |t, _| {
                        let sq = |f: f32| (tau * f * k * t).sin().signum() * 0.5 + 0.5 * (tau * f * k * t).sin();
                        (-t * 18.0).exp() * (0.6 * sq(540.0) + 0.4 * sq(800.0))
                    })
                }
                ClickSound::Sticks => {
                    let f = 2_200.0 + up * 600.0;
                    render(sr, 0.05, peak, |t, i| (-t * 160.0).exp() * (0.6 * noise(i) + 0.8 * (tau * f * t).sin()))
                }
                ClickSound::Drums => {
                    if accent {
                        // Kick: pitch sweep 140 → 45 Hz.
                        render(sr, 0.25, peak, |t, _| {
                            let freq_phase = 45.0 * t + (140.0 - 45.0) / 30.0 * (1.0 - (-t * 30.0).exp());
                            (-t * 12.0).exp() * (tau * freq_phase).sin()
                        })
                    } else {
                        // Closed hi-hat: crude high-pass of noise.
                        render(sr, 0.05, peak, |t, i| (-t * 90.0).exp() * (noise(i) - noise(i + 1)))
                    }
                }
            }
        };
        Self { accent: pair(true), regular: pair(false) }
    }

    #[must_use]
    pub fn get(&self, accent: bool) -> &[f32] {
        if accent { &self.accent } else { &self.regular }
    }

    fn longest(&self) -> usize {
        self.accent.len().max(self.regular.len())
    }
}

/// Click events (frame positions + accent flags) for one song, with every
/// voice pre-rendered so the user can switch sounds while playing.
#[derive(Debug, Clone, PartialEq)]
pub struct ClickTrack {
    positions: Vec<i64>,
    accents: Vec<bool>,
    voices: Vec<ClickSounds>,
    beats_per_bar: u8,
}

impl ClickTrack {
    /// Builds a click track at `sample_rate` from a beat analysis.
    #[must_use]
    pub fn from_analysis(analysis: &BeatAnalysis, sample_rate: u32) -> Self {
        #[allow(clippy::cast_possible_truncation)]
        let positions = analysis.beats.iter().map(|b| (b * f64::from(sample_rate)).round() as i64).collect();
        let accents = (0..analysis.beats.len()).map(|i| analysis.is_downbeat(i)).collect();
        Self {
            positions,
            accents,
            voices: ClickSound::ALL.iter().map(|k| ClickSounds::synth(sample_rate, *k)).collect(),
            beats_per_bar: analysis.beats_per_bar.max(1),
        }
    }

    #[must_use]
    pub fn positions(&self) -> &[i64] {
        &self.positions
    }

    #[must_use]
    pub const fn beats_per_bar(&self) -> u8 {
        self.beats_per_bar
    }

    /// Voice by [`ClickSound::index`] (falls back to the first one).
    #[must_use]
    pub fn voice(&self, index: usize) -> &ClickSounds {
        &self.voices[index.min(self.voices.len() - 1)]
    }

    /// Adds clicks overlapping `[start, start + left.len())` into the buffers.
    pub fn render_add(&self, start: i64, left: &mut [f32], right: &mut [f32], gains: [f32; 2], voice: usize) {
        let sounds = self.voice(voice);
        let len = left.len().min(right.len()) as i64;
        let longest = sounds.longest() as i64;
        let first = self.positions.partition_point(|p| p.saturating_add(longest) <= start);
        for (pos, accent) in self.positions[first..].iter().zip(&self.accents[first..]) {
            if *pos >= start.saturating_add(len) {
                break;
            }
            mix_sound(sounds.get(*accent), pos.saturating_sub(start), left, right, gains[0], gains[1]);
        }
    }
}

/// Maximum number of count-in clicks (4 bars of 16).
pub const MAX_COUNT_IN_CLICKS: usize = 64;

/// A planned count-in: clicks on the song's own beat grid, extended
/// backwards from the next beat after the start position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountIn {
    /// Click positions on the music timeline (frames; may be < start or < 0).
    pub clicks: Vec<(i64, bool)>,
    /// Frames of silence-with-clicks before the music starts.
    pub preroll: i64,
}

/// Plans `bars` bars of count-in before playing from `start` (frames).
///
/// The clicks continue the song's grid backwards, so the last count-in click
/// is exactly one beat before the next real beat and the accent pattern
/// lines up with the song's bars.
#[must_use]
pub fn plan_count_in(analysis: &BeatAnalysis, sample_rate: u32, start: i64, bars: u8) -> Option<CountIn> {
    let bpb = i64::from(analysis.beats_per_bar.max(1));
    let n = (i64::from(bars) * bpb).min(MAX_COUNT_IN_CLICKS as i64);
    if n == 0 {
        return None;
    }
    let rate = f64::from(sample_rate);
    #[allow(clippy::cast_possible_truncation)]
    let to_frames = |secs: f64| (secs * rate).round() as i64;
    let beats = &analysis.beats;
    let (next_idx, next_time, period) = if beats.len() >= 2 {
        let start_secs = start as f64 / rate;
        let i = beats.partition_point(|b| *b < start_secs - 1e-6);
        let p = |k: usize| beats[k.clamp(1, beats.len() - 1)] - beats[k.clamp(1, beats.len() - 1) - 1];
        if i < beats.len() {
            (i as i64, beats[i], p(i))
        } else {
            // Past the last beat: extrapolate the grid.
            let period = p(beats.len() - 1);
            let last = beats[beats.len() - 1];
            #[allow(clippy::cast_possible_truncation)]
            let k = ((start_secs - last) / period).ceil().max(1.0) as i64;
            (beats.len() as i64 - 1 + k, last + k as f64 * period, period)
        }
    } else {
        (0, start as f64 / rate, 60.0 / analysis.bpm.max(1.0))
    };
    let phase = i64::from(analysis.downbeat_phase);
    let clicks: Vec<(i64, bool)> = (1..=n)
        .rev()
        .map(|j| {
            let idx = next_idx - j;
            let accent = (idx - phase).rem_euclid(bpb) == 0;
            (to_frames(next_time - j as f64 * period), accent)
        })
        .collect();
    let earliest = clicks.first().map_or(start, |c| c.0);
    Some(CountIn { preroll: (start - earliest).max(0), clicks })
}

/// Adds `sound` starting at `offset` (relative to the buffers, may be negative).
pub fn mix_sound(sound: &[f32], offset: i64, left: &mut [f32], right: &mut [f32], gain_l: f32, gain_r: f32) {
    let skip = if offset < 0 { usize::try_from(offset.unsigned_abs()).unwrap_or(usize::MAX) } else { 0 };
    let dst_start = usize::try_from(offset.max(0)).unwrap_or(usize::MAX);
    if skip >= sound.len() || dst_start >= left.len() {
        return;
    }
    let src = &sound[skip..];
    for ((l, r), s) in left[dst_start..].iter_mut().zip(right[dst_start..].iter_mut()).zip(src) {
        *l += s * gain_l;
        *r += s * gain_r;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analysis(beats: Vec<f64>) -> BeatAnalysis {
        BeatAnalysis { bpm: 120.0, beats, beats_per_bar: 4, downbeat_phase: 1, confidence: 1.0, variable_tempo: false }
    }

    #[test]
    fn clicks_land_exactly_on_beats_with_accents() {
        let sr = 1_000;
        let track = ClickTrack::from_analysis(&analysis(vec![0.1, 0.6, 1.1]), sr);
        assert_eq!(track.positions(), &[100, 600, 1100]);
        let mut l = vec![0.0; 2000];
        let mut r = vec![0.0; 2000];
        track.render_add(0, &mut l, &mut r, [1.0, 1.0], 0);
        let first_nonzero = l.iter().position(|s| s.abs() > 0.0).unwrap();
        assert!((100..103).contains(&first_nonzero));
        assert!(l[..100].iter().all(|s| *s == 0.0));
        // Beat index 1 is the downbeat (phase 1) → louder accent click.
        let peak = |a: usize| l[a..a + 60].iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak(600) > peak(100));
    }

    #[test]
    fn block_rendering_matches_whole_rendering() {
        let sr = 8_000;
        let track = ClickTrack::from_analysis(&analysis(vec![0.0, 0.25, 0.5, 0.75]), sr);
        let mut whole_l = vec![0.0; 8_000];
        let mut whole_r = vec![0.0; 8_000];
        track.render_add(0, &mut whole_l, &mut whole_r, [0.5, 0.25], 0);
        let mut blocks = Vec::new();
        for start in (0..8_000).step_by(37) {
            let n = 37.min(8_000 - start);
            let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
            track.render_add(start as i64, &mut l, &mut r, [0.5, 0.25], 0);
            blocks.extend(l);
        }
        assert_eq!(blocks, whole_l);
        assert!(whole_r.iter().zip(&whole_l).all(|(r, l)| (r * 2.0 - l).abs() < 1e-6));
    }

    #[test]
    fn count_in_continues_the_song_grid() {
        // Beats every 0.5 s from 1.0 s; downbeat on beat index 1.
        let a = analysis((0..20).map(|k| 1.0 + f64::from(k) * 0.5).collect());
        let plan = plan_count_in(&a, 1_000, 0, 1).unwrap();
        let frames: Vec<i64> = plan.clicks.iter().map(|c| c.0).collect();
        assert_eq!(frames, vec![-1_000, -500, 0, 500], "last click one beat before the first beat");
        assert_eq!(plan.preroll, 1_000);
        // Index -3 ≡ phase 1 (mod 4) is the bar's downbeat.
        assert_eq!(plan.clicks.iter().map(|c| c.1).collect::<Vec<_>>(), vec![false, true, false, false]);

        let mid = plan_count_in(&a, 1_000, 1_700, 2).unwrap();
        assert_eq!(mid.clicks.len(), 8);
        assert_eq!(mid.clicks.last().unwrap().0, 1_500, "lands one beat before 2.0 s");
        assert_eq!(mid.preroll, 1_700 - mid.clicks[0].0);
        assert!(plan_count_in(&a, 1_000, 0, 0).is_none());
        // Past the end: grid is extrapolated.
        let end = plan_count_in(&a, 1_000, 20_000, 1).unwrap();
        assert_eq!(end.clicks.last().unwrap().0 % 500, 0);
    }

    #[test]
    fn every_voice_is_audible_and_safe() {
        for kind in ClickSound::ALL {
            let v = ClickSounds::synth(48_000, kind);
            let peak = |x: &[f32]| x.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            assert!(!v.accent.is_empty() && !v.regular.is_empty(), "{kind:?}");
            assert!(peak(&v.accent) <= 1.0 && peak(&v.regular) <= 1.0, "{kind:?} clips");
            assert!(peak(&v.accent) > peak(&v.regular), "{kind:?} accent must stand out");
            assert!(v.accent.iter().chain(&v.regular).all(|s| s.is_finite()));
        }
        assert_eq!(ClickSound::Drums.index(), 5);
    }

    #[test]
    fn mix_sound_handles_out_of_range_offsets() {
        let (mut l, mut r) = (vec![0.0; 4], vec![0.0; 4]);
        mix_sound(&[1.0, 1.0, 1.0], -2, &mut l, &mut r, 1.0, 1.0);
        assert_eq!(l, vec![1.0, 0.0, 0.0, 0.0]);
        mix_sound(&[1.0], 10, &mut l, &mut r, 1.0, 1.0);
        mix_sound(&[1.0], i64::MIN, &mut l, &mut r, 1.0, 1.0);
        mix_sound(&[1.0], i64::MAX, &mut l, &mut r, 1.0, 1.0);
        assert_eq!(l, vec![1.0, 0.0, 0.0, 0.0]);
    }
}
