//! Offline chord recognition for playing along: the song's harmony, beat by
//! beat, as major and minor triads.
//!
//! The harmonic stems (everything but drums and vocals) are folded into a
//! chromagram (energy per pitch class, 65 Hz – 2 kHz), averaged between
//! beats of the song's grid, and matched against triad templates; the bass
//! stem votes for the root. A Viterbi pass then picks the most likely chord
//! sequence, where changing chord costs a little (less on a bar line), so
//! passing notes do not flicker the display. Spelling (C♯ or D♭) follows
//! the estimated key.

use rustfft::num_complex::Complex32;
use serde::{Deserialize, Serialize};

use crate::stft::{Stft, hann, windowed_frame};
use crate::tempo::BeatAnalysis;

const FFT: usize = 8192;
const LOW_HZ: f64 = 65.0;
const HIGH_HZ: f64 = 2100.0;
const BASS_LOW_HZ: f64 = 40.0;
const BASS_HIGH_HZ: f64 = 260.0;
/// Cost of a chord change (in template-match units), and on a bar line.
const CHANGE_COST: f32 = 0.18;
const CHANGE_COST_ON_BAR: f32 = 0.08;
/// A power chord must match this much better than a triad to be named one.
const POWER_MARGIN: f32 = 0.04;
/// Weight of the bass note voting for a chord's root.
const BASS_WEIGHT: f32 = 0.25;
/// Beats quieter than this (relative to the median) are "no chord".
const SILENT_BELOW: f32 = 0.02;
/// Match a chord must beat for anything but "no chord" to win.
const NO_CHORD_SCORE: f32 = 0.45;

/// Bumped when the detector changes, so stored results are recomputed.
pub const CHORDS_VERSION: u32 = 1;

const SHARP: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const FLAT: [&str; 12] = ["C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Quality {
    Major,
    Minor,
    /// Root and fifth only (`E5`): what distorted guitars mostly play.
    Power,
}

/// A chord held from `start` to `end` (seconds).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChordSpan {
    pub start: f64,
    pub end: f64,
    /// Pitch class of the root (0 = C).
    pub root: u8,
    pub quality: Quality,
    /// Display name, spelled for the key (`F#m`, `Bb`).
    pub name: String,
}

/// What is stored in `chords.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChordChart {
    pub version: u32,
    /// Estimated key, e.g. `G` or `Em`.
    pub key: String,
    pub chords: Vec<ChordSpan>,
    /// Grid the chart was made on; a new grid means a new chart.
    pub bpm: f64,
    pub first_beat: f64,
}

impl ChordChart {
    /// Made by this detector on this beat grid.
    #[must_use]
    pub fn is_current(&self, analysis: &BeatAnalysis) -> bool {
        self.version == CHORDS_VERSION
            && (self.bpm - analysis.bpm).abs() < 1e-6
            && (self.first_beat - analysis.beats.first().copied().unwrap_or(0.0)).abs() < 1e-6
    }
}

/// Chroma frames (12 pitch classes) of a mono signal, `hop` samples apart,
/// over `lo..hi` Hz.
fn chroma(mono: &[f32], rate: f64, hop: usize, frames: usize, lo: f64, hi: f64) -> Vec<[f32; 12]> {
    let window = hann(FFT);
    let mut stft = Stft::new(FFT);
    let mut frame = vec![0.0f32; FFT];
    let mut spectrum = vec![Complex32::default(); stft.bins()];
    let bin_hz = rate / FFT as f64;
    let class: Vec<Option<usize>> = (0..stft.bins())
        .map(|b| {
            let hz = b as f64 * bin_hz;
            (lo..hi).contains(&hz).then(|| {
                let midi = 69.0 + 12.0 * (hz / 440.0).log2();
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let pc = (midi.round() as i64).rem_euclid(12) as usize;
                pc
            })
        })
        .collect();
    (0..frames)
        .map(|k| {
            let mut out = [0.0f32; 12];
            #[allow(clippy::cast_possible_wrap)]
            let start = (k * hop) as isize - (FFT / 2) as isize;
            if start >= mono.len() as isize {
                return out;
            }
            windowed_frame(mono, start, &window, &mut frame);
            stft.forward(&frame, &mut spectrum);
            for (s, c) in spectrum.iter().zip(&class) {
                if let Some(c) = c {
                    // Magnitude, not power: quieter chord tones still count.
                    out[*c] += s.norm();
                }
            }
            out
        })
        .collect()
}

/// Beat-length segments (start, end) covering the song.
fn segments(analysis: &BeatAnalysis, duration: f64) -> Vec<(f64, f64)> {
    let mut edges: Vec<f64> = analysis.beats.iter().copied().filter(|t| *t > 0.0 && *t < duration).collect();
    if edges.len() < 2 {
        let step = 0.5;
        edges = (1..).map(|i| f64::from(i) * step).take_while(|t| *t < duration).collect();
    }
    let mut out = Vec::with_capacity(edges.len() + 1);
    let mut prev = 0.0;
    for t in edges {
        out.push((prev, t));
        prev = t;
    }
    out.push((prev, duration));
    out.retain(|(a, b)| b > a);
    out
}

fn template(root: usize, quality: Quality) -> [f32; 12] {
    let mut t = [0.0f32; 12];
    t[root] = 1.0;
    t[(root + 7) % 12] = 1.0;
    match quality {
        Quality::Major => t[(root + 4) % 12] = 1.0,
        Quality::Minor => t[(root + 3) % 12] = 1.0,
        Quality::Power => {}
    }
    t
}

fn normalised(v: &[f32; 12]) -> [f32; 12] {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n <= 0.0 { [0.0; 12] } else { v.map(|x| x / n) }
}

/// Chords of a major key, as (interval from the tonic, quality); power
/// chords fit any degree. The tonic chord counts double.
const MAJOR_DEGREES: [(usize, Quality); 6] = [
    (0, Quality::Major),
    (2, Quality::Minor),
    (4, Quality::Minor),
    (5, Quality::Major),
    (7, Quality::Major),
    (9, Quality::Minor),
];
/// Natural minor plus the major V.
const MINOR_DEGREES: [(usize, Quality); 6] = [
    (0, Quality::Minor),
    (3, Quality::Major),
    (5, Quality::Minor),
    (7, Quality::Major),
    (8, Quality::Major),
    (10, Quality::Major),
];

/// (tonic, mode) of the key that explains the most chord time: distorted
/// guitars smear the raw chroma, the chord sequence is cleaner.
fn estimate_key(chords: &[ChordSpan]) -> (usize, Quality) {
    let mut best = (0, Quality::Major, f64::MIN);
    for tonic in 0..12 {
        for (mode, degrees) in [(Quality::Major, &MAJOR_DEGREES), (Quality::Minor, &MINOR_DEGREES)] {
            let score: f64 = chords
                .iter()
                .map(|c| {
                    let interval = (usize::from(c.root) + 12 - tonic) % 12;
                    let fits =
                        degrees.iter().any(|(i, q)| *i == interval && (c.quality == *q || c.quality == Quality::Power));
                    let tonic_bonus =
                        if interval == 0 && (c.quality == mode || c.quality == Quality::Power) { 1.0 } else { 0.0 };
                    if fits { (c.end - c.start) * (1.0 + tonic_bonus) } else { 0.0 }
                })
                .sum();
            if score > best.2 {
                best = (tonic, mode, score);
            }
        }
    }
    (best.0, best.1)
}

/// Keys written with flats (by the major key they share notes with).
fn uses_flats(tonic: usize, quality: Quality) -> bool {
    let major = if quality == Quality::Major { tonic } else { (tonic + 3) % 12 };
    matches!(major, 5 | 10 | 3 | 8 | 1)
}

fn spell(root: usize, quality: Quality, flats: bool) -> String {
    let n = if flats { FLAT[root] } else { SHARP[root] };
    match quality {
        Quality::Major => n.to_owned(),
        Quality::Minor => format!("{n}m"),
        Quality::Power => format!("{n}5"),
    }
}

/// Detects the chords of a song.
///
/// `harmony` is the mono mix of the harmonic stems, `bass` the bass stem
/// (mono, may be empty), both at `rate` and starting at song time 0.
#[must_use]
pub fn detect(harmony: &[f32], bass: &[f32], rate: u32, analysis: &BeatAnalysis, duration: f64) -> ChordChart {
    let rate_f = f64::from(rate);
    let hop = FFT / 4;
    let frames = harmony.len().max(bass.len()) / hop + 1;
    let chroma_h = chroma(harmony, rate_f, hop, frames, LOW_HZ, HIGH_HZ);
    let chroma_b =
        if bass.is_empty() { Vec::new() } else { chroma(bass, rate_f, hop, frames, BASS_LOW_HZ, BASS_HIGH_HZ) };
    let frame_time = |k: usize| k as f64 * hop as f64 / rate_f;

    // Beat-synchronous chroma.
    let segs = segments(analysis, duration);
    let mut seg_h = vec![[0.0f32; 12]; segs.len()];
    let mut seg_b = vec![[0.0f32; 12]; segs.len()];
    let mut j = 0;
    for (k, frame) in chroma_h.iter().enumerate() {
        let t = frame_time(k);
        while j + 1 < segs.len() && t >= segs[j].1 {
            j += 1;
        }
        if t < segs[j].0 || t >= segs[j].1 {
            continue;
        }
        for c in 0..12 {
            seg_h[j][c] += frame[c];
            if let Some(b) = chroma_b.get(k) {
                seg_b[j][c] += b[c];
            }
        }
    }
    let energy: Vec<f32> = seg_h.iter().map(|v| v.iter().sum::<f32>()).collect();
    let mut sorted = energy.clone();
    sorted.sort_by(f32::total_cmp);
    let median = sorted.get(sorted.len() / 2).copied().unwrap_or(0.0);

    // Emission scores: 24 triads + "no chord" (index 24).
    let templates: Vec<(usize, Quality, [f32; 12])> = (0..12)
        .flat_map(|r| [Quality::Major, Quality::Minor, Quality::Power].map(|q| (r, q, normalised(&template(r, q)))))
        .collect();
    let states = templates.len() + 1;
    let scores: Vec<Vec<f32>> = seg_h
        .iter()
        .zip(&seg_b)
        .zip(&energy)
        .map(|((h, b), e)| {
            let mut s = vec![0.0f32; states];
            if *e <= median * SILENT_BELOW || *e <= 0.0 {
                s[states - 1] = 1.0;
                return s;
            }
            let h = normalised(h);
            let bsum: f32 = b.iter().sum();
            for (i, (root, q, t)) in templates.iter().enumerate() {
                let cos: f32 = h.iter().zip(t).map(|(x, y)| x * y).sum();
                let bass_root = if bsum > 0.0 { b[*root] / bsum } else { 0.0 };
                let margin = if *q == Quality::Power { POWER_MARGIN } else { 0.0 };
                s[i] = cos + BASS_WEIGHT * bass_root - margin;
            }
            s[states - 1] = NO_CHORD_SCORE;
            s
        })
        .collect();

    // Viterbi (maximising score minus change costs).
    let n = scores.len();
    let mut back = vec![vec![0usize; states]; n];
    let mut acc = scores.first().cloned().unwrap_or_default();
    for t in 1..n {
        let on_bar =
            analysis.beats.iter().position(|b| (*b - segs[t].0).abs() < 1e-9).is_some_and(|i| analysis.is_downbeat(i));
        let cost = if on_bar { CHANGE_COST_ON_BAR } else { CHANGE_COST };
        let (best_prev, best_val) =
            acc.iter().enumerate().fold((0, f32::MIN), |m, (i, v)| if *v > m.1 { (i, *v) } else { m });
        let mut next = vec![0.0f32; states];
        for s in 0..states {
            let (from, v) = if acc[s] >= best_val - cost { (s, acc[s]) } else { (best_prev, best_val - cost) };
            back[t][s] = from;
            next[s] = v + scores[t][s];
        }
        acc = next;
    }
    let mut path = vec![0usize; n];
    if n > 0 {
        path[n - 1] = acc.iter().enumerate().fold((0, f32::MIN), |m, (i, v)| if *v > m.1 { (i, *v) } else { m }).0;
        for t in (1..n).rev() {
            path[t - 1] = back[t][path[t]];
        }
    }

    let mut chords: Vec<ChordSpan> = Vec::new();
    for (t, s) in path.iter().enumerate() {
        let (start, end) = segs[t];
        let Some((root, quality, _)) = templates.get(*s) else { continue };
        match chords.last_mut() {
            Some(last)
                if usize::from(last.root) == *root && last.quality == *quality && (last.end - start).abs() < 1e-9 =>
            {
                last.end = end;
            }
            _ => chords.push(ChordSpan {
                start,
                end,
                #[allow(clippy::cast_possible_truncation)]
                root: *root as u8,
                quality: *quality,
                name: String::new(),
            }),
        }
    }
    let (tonic, mode) = estimate_key(&chords);
    let flats = uses_flats(tonic, mode);
    for c in &mut chords {
        c.name = spell(usize::from(c.root), c.quality, flats);
    }
    ChordChart {
        version: CHORDS_VERSION,
        key: spell(tonic, mode, flats),
        chords,
        bpm: analysis.bpm,
        first_beat: analysis.beats.first().copied().unwrap_or(0.0),
    }
}

/// Index of the chord sounding at `secs`, if any.
#[must_use]
pub fn chord_at(chords: &[ChordSpan], secs: f64) -> Option<usize> {
    let i = chords.partition_point(|c| c.start <= secs).checked_sub(1)?;
    (secs < chords[i].end).then_some(i)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 22_050;

    fn grid(bpm: f64, duration: f64) -> BeatAnalysis {
        let period = 60.0 / bpm;
        BeatAnalysis {
            bpm,
            beats: (0..).map(|i| f64::from(i) * period).take_while(|t| *t < duration).collect(),
            beats_per_bar: 4,
            downbeat_phase: 0,
            confidence: 1.0,
            variable_tempo: false,
        }
    }

    fn midi_hz(m: i32) -> f32 {
        440.0 * 2f32.powf((m - 69) as f32 / 12.0)
    }

    /// A strummed-ish triad: three notes with a few harmonics.
    fn add_chord(out: &mut [f32], bass: &mut [f32], from: f64, to: f64, root_midi: i32, minor: bool) {
        let notes = [root_midi, root_midi + if minor { 3 } else { 4 }, root_midi + 7];
        let (a, b) = ((from * f64::from(RATE)) as usize, (to * f64::from(RATE)) as usize);
        for i in a..b.min(out.len()) {
            let t = i as f32 / RATE as f32;
            let mut s = 0.0;
            for n in notes {
                for h in 1..=4 {
                    s += (std::f32::consts::TAU * midi_hz(n) * h as f32 * t).sin() * 0.2 / h as f32;
                }
            }
            out[i] += s;
            bass[i] += (std::f32::consts::TAU * midi_hz(root_midi - 24) * t).sin() * 0.5;
        }
    }

    #[test]
    fn a_pop_progression_is_recognised() {
        // C G Am F, two bars each at 120 BPM (4 s per chord), twice.
        let duration = 32.0;
        let n = (duration * f64::from(RATE)) as usize;
        let (mut h, mut b) = (vec![0.0; n], vec![0.0; n]);
        let prog = [(60, false), (55, false), (57, true), (53, false)];
        for (i, (root, minor)) in prog.iter().cycle().take(8).enumerate() {
            add_chord(&mut h, &mut b, i as f64 * 4.0, (i + 1) as f64 * 4.0, *root, *minor);
        }
        let chart = detect(&h, &b, RATE, &grid(120.0, duration), duration);
        let names: Vec<&str> = chart.chords.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["C", "G", "Am", "F", "C", "G", "Am", "F"], "{:?}", chart.chords);
        for (i, c) in chart.chords.iter().enumerate() {
            assert!((c.start - i as f64 * 4.0).abs() < 0.51, "chord {i} starts at {}", c.start);
        }
        assert_eq!(chart.key, "C");
    }

    #[test]
    fn flat_keys_are_spelled_with_flats() {
        // Bb Eb F Bb.
        let duration = 16.0;
        let n = (duration * f64::from(RATE)) as usize;
        let (mut h, mut b) = (vec![0.0; n], vec![0.0; n]);
        for (i, root) in [58, 63, 53, 58].iter().enumerate() {
            add_chord(&mut h, &mut b, i as f64 * 4.0, (i + 1) as f64 * 4.0, *root, false);
        }
        let chart = detect(&h, &b, RATE, &grid(120.0, duration), duration);
        let names: Vec<&str> = chart.chords.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Bb", "Eb", "F", "Bb"]);
        assert_eq!(chart.key, "Bb");
    }

    #[test]
    fn silence_has_no_chord() {
        let duration = 12.0;
        let n = (duration * f64::from(RATE)) as usize;
        let (mut h, mut b) = (vec![0.0; n], vec![0.0; n]);
        add_chord(&mut h, &mut b, 4.0, 8.0, 62, true);
        let chart = detect(&h, &b, RATE, &grid(120.0, duration), duration);
        assert_eq!(chart.chords.len(), 1, "{:?}", chart.chords);
        assert_eq!(chart.chords[0].name, "Dm");
        assert!((chart.chords[0].start - 4.0).abs() < 0.51 && (chart.chords[0].end - 8.0).abs() < 0.51);
        assert_eq!(chord_at(&chart.chords, 6.0), Some(0));
        assert_eq!(chord_at(&chart.chords, 2.0), None);
        assert_eq!(chord_at(&chart.chords, 10.0), None);
    }

    #[test]
    fn a_chart_belongs_to_its_grid() {
        let g = grid(120.0, 10.0);
        let chart = detect(&[], &[], RATE, &g, 10.0);
        assert!(chart.is_current(&g));
        assert!(!chart.is_current(&grid(121.0, 10.0)));
    }
}
