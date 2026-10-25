//! Automatic balance of recorded takes against the original song.
//!
//! A take is someone playing along with the song, usually one of its
//! instruments. Each take is matched to the stem it plays — the one whose
//! energy rises and falls with the take's, band by band, and whose notes
//! start when the take's do — then set as loud as that stem was in the original mix,
//! at its position in the stereo field, with the original stem muted: the
//! song sounds like the record with the musician's part in it. A take that
//! matches nothing sits a little under the whole song instead.
//!
//! Everything works on a coarse profile of each track (band energies every
//! 20 ms), so a track's samples can be dropped once it is analysed.

use rustfft::num_complex::Complex32;

use crate::audio::StereoBuffer;
use crate::stft::{Stft, hann, windowed_frame};

/// Profile frame period.
pub const HOP_SECS: f64 = 0.02;
const FFT: usize = 2048;
const BANDS: usize = 8;
/// Band edges in Hz (octaves from the kick drum to cymbal air).
const EDGES: [f32; BANDS + 1] = [30.0, 80.0, 160.0, 320.0, 640.0, 1280.0, 2560.0, 5120.0, 12000.0];
/// Frames this far below the take's loudest one are not "playing".
const ACTIVE_BELOW_PEAK_DB: f32 = 40.0;
/// Relative gate of the loudness measure (as in EBU R128).
const GATE_DB: f32 = 10.0;
/// Below this score a take is not considered to play a stem.
const MIN_SCORE: f32 = 0.2;
/// The stem must follow the take this much better than the whole song does
/// (a microphone hearing the speakers follows everything alike)…
const OVER_SONG: f32 = 0.08;
/// …and better than the next stem by this much.
const OVER_NEXT: f32 = 0.05;
/// Loudest gain a take may get (the project's channel limit).
const MAX_TAKE_GAIN_DB: f32 = 12.0;
/// An unmatched take sits this far under the whole song.
const UNDER_SONG_DB: f32 = 6.0;
/// Onset alignment tolerance, in frames (±40 ms).
const MAX_LAG: usize = 2;
const EPS: f32 = 1e-10;

/// Band energies and stereo power of a track, frame by frame, on the song
/// timeline.
#[derive(Debug, Clone)]
pub struct Profile {
    bands: Vec<[f32; BANDS]>,
    left: Vec<f32>,
    right: Vec<f32>,
}

impl Profile {
    /// Profiles `buffer` placed `offset_secs` into a song timeline of
    /// `frames` profile frames (what lies outside the buffer is silence).
    #[must_use]
    pub fn analyse(buffer: &StereoBuffer, offset_secs: f64, frames: usize) -> Self {
        let rate = f64::from(buffer.sample_rate());
        let window = hann(FFT);
        let mut stft = Stft::new(FFT);
        let mut frame = vec![0.0f32; FFT];
        let mut spectrum = vec![Complex32::default(); stft.bins()];
        let bin_hz = buffer.sample_rate() as f32 / FFT as f32;
        let band_of: Vec<Option<usize>> = (0..stft.bins())
            .map(|b| {
                let hz = b as f32 * bin_hz;
                EDGES.windows(2).position(|e| hz >= e[0] && hz < e[1])
            })
            .collect();
        let mono = buffer.to_mono();
        let hop = HOP_SECS * rate;
        let mut out = Self { bands: vec![[0.0; BANDS]; frames], left: vec![0.0; frames], right: vec![0.0; frames] };
        for k in 0..frames {
            #[allow(clippy::cast_possible_truncation)]
            let centre = ((k as f64 * HOP_SECS - offset_secs) * rate) as isize;
            let start = centre - (FFT / 2) as isize;
            if start + FFT as isize <= 0 || start >= mono.len() as isize {
                continue;
            }
            windowed_frame(&mono, start, &window, &mut frame);
            stft.forward(&frame, &mut spectrum);
            for (s, band) in spectrum.iter().zip(&band_of) {
                if let Some(b) = band {
                    out.bands[k][*b] += s.norm_sqr();
                }
            }
            #[allow(clippy::cast_possible_truncation)]
            let span = (centre.max(0) as usize)..((centre.max(0) as f64 + hop) as usize).min(mono.len());
            out.left[k] = power(buffer.left().get(span.clone()).unwrap_or_default());
            out.right[k] = power(buffer.right().get(span).unwrap_or_default());
        }
        out
    }

    fn frames(&self) -> usize {
        self.bands.len()
    }

    fn total(&self, k: usize) -> f32 {
        self.bands.get(k).map_or(0.0, |b| b.iter().sum())
    }
}

/// Number of profile frames of a song lasting `duration_secs`.
#[must_use]
pub fn timeline_frames(duration_secs: f64) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (duration_secs.max(0.0) / HOP_SECS).ceil() as usize;
    n
}

fn power(x: &[f32]) -> f32 {
    if x.is_empty() { 0.0 } else { x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32 }
}

fn db(p: f32) -> f32 {
    10.0 * p.max(EPS).log10()
}

/// What to do with one take.
#[derive(Debug, Clone, PartialEq)]
pub struct TakePlan {
    /// Index of the stem the take plays, if any.
    pub stem: Option<usize>,
    pub gain_db: f32,
    pub pan: f32,
}

/// The balanced mix: stems back at their original level, the ones a take
/// replaces muted; takes at the level of what they replace.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub stem_muted: Vec<bool>,
    pub takes: Vec<TakePlan>,
}

/// Frames where the take is playing.
fn active_frames(take: &Profile) -> Vec<usize> {
    let peak = (0..take.frames()).map(|k| take.total(k)).fold(0.0f32, f32::max);
    if peak <= EPS {
        return Vec::new();
    }
    let floor = peak * 10f32.powf(-ACTIVE_BELOW_PEAK_DB / 10.0);
    (0..take.frames()).filter(|&k| take.total(k) >= floor).collect()
}

/// Gated mean power of `level(k)` over `frames`, in dB: the level while the
/// track plays, ignoring its pauses.
fn loudness(frames: &[usize], level: impl Fn(usize) -> f32) -> f32 {
    let p: Vec<f32> = frames.iter().map(|&k| level(k)).filter(|&p| p > EPS).collect();
    if p.is_empty() {
        return db(0.0);
    }
    let mean = p.iter().sum::<f32>() / p.len() as f32;
    let gate = mean * 10f32.powf(-GATE_DB / 10.0);
    let kept: Vec<f32> = p.into_iter().filter(|&x| x >= gate).collect();
    db(kept.iter().sum::<f32>() / kept.len().max(1) as f32)
}

fn pearson(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    if n < 3 {
        return 0.0;
    }
    let (ma, mb) = (a[..n].iter().sum::<f32>() / n as f32, b[..n].iter().sum::<f32>() / n as f32);
    let (mut sab, mut saa, mut sbb) = (0.0f32, 0.0f32, 0.0f32);
    for (x, y) in a[..n].iter().zip(&b[..n]) {
        let (dx, dy) = (x - ma, y - mb);
        sab += dx * dy;
        saa += dx * dx;
        sbb += dy * dy;
    }
    if saa <= EPS || sbb <= EPS { 0.0 } else { sab / (saa * sbb).sqrt() }
}

/// Positive change of log band energy: where notes start. The first frame
/// of each stretch the take plays is left out: everything "starts" when the
/// take does, which says nothing about what it plays.
fn onsets(p: &Profile, frames: &[usize]) -> Vec<f32> {
    frames
        .iter()
        .enumerate()
        .map(|(i, &k)| {
            if i == 0 || frames[i - 1] + 1 != k {
                return 0.0;
            }
            p.bands[k].iter().zip(p.bands[k - 1]).map(|(now, before)| (db(*now) - db(before)).max(0.0)).sum()
        })
        .collect()
}

/// Best correlation of `a` and `b` with `b` shifted up to ±`MAX_LAG` frames.
fn lagged(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    if n <= 2 * MAX_LAG + 2 {
        return 0.0;
    }
    (0..=2 * MAX_LAG)
        .map(|i| {
            if i <= MAX_LAG {
                pearson(&a[..n - (MAX_LAG - i)], &b[MAX_LAG - i..n])
            } else {
                pearson(&a[i - MAX_LAG..n], &b[..n - (i - MAX_LAG)])
            }
        })
        .fold(-1.0f32, f32::max)
}

/// How closely `energy` (band energies per frame) rises and falls with the
/// take, band by band, weighted by where the take's energy is.
fn follows(take: &Profile, frames: &[usize], energy: impl Fn(usize, usize) -> f32) -> f32 {
    let mut share = [0.0f32; BANDS];
    for &k in frames {
        for (sh, e) in share.iter_mut().zip(take.bands[k]) {
            *sh += e;
        }
    }
    let total: f32 = share.iter().sum();
    if total <= EPS {
        return 0.0;
    }
    (0..BANDS)
        .map(|b| {
            let x: Vec<f32> = frames.iter().map(|&k| db(take.bands[k][b])).collect();
            let y: Vec<f32> = frames.iter().map(|&k| db(energy(k, b))).collect();
            share[b] / total * lagged(&x, &y)
        })
        .sum()
}

/// The stem `take` plays over its `frames`, if one stands out.
fn matching_stem(stems: &[Profile], take: &Profile, frames: &[usize]) -> Option<usize> {
    let song = follows(take, frames, |k, b| stems.iter().map(|s| s.bands[k][b]).sum());
    let notes = onsets(take, frames);
    let mut scored: Vec<(usize, f32, f32)> = stems
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let energy = follows(take, frames, |k, b| s.bands[k][b]);
            (i, 0.7 * energy + 0.3 * lagged(&notes, &onsets(s, frames)), energy)
        })
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    let (best, score, energy) = *scored.first()?;
    let next = scored.get(1).map_or(-1.0, |n| n.1);
    (score >= MIN_SCORE && energy >= song + OVER_SONG && score >= next + OVER_NEXT).then_some(best)
}

/// Balances `takes` against the song's `stems` (all profiled on the same
/// timeline).
#[must_use]
pub fn plan(stems: &[Profile], takes: &[Profile]) -> Plan {
    let song = |k: usize| stems.iter().map(|s| s.total(k)).sum::<f32>();
    struct Found {
        frames: Vec<usize>,
        stem: Option<usize>,
    }
    let found: Vec<Found> = takes
        .iter()
        .map(|t| {
            let frames = active_frames(t);
            let stem = if frames.is_empty() { None } else { matching_stem(stems, t, &frames) };
            Found { frames, stem }
        })
        .collect();
    let sharing = |stem: usize| found.iter().filter(|f| f.stem == Some(stem)).count().max(1);
    let takes_out = takes
        .iter()
        .zip(&found)
        .map(|(t, f)| {
            if f.frames.is_empty() {
                return TakePlan { stem: None, gain_db: 0.0, pan: 0.0 };
            }
            let own = loudness(&f.frames, |k| t.total(k));
            let (target, pan) = match f.stem {
                Some(i) => {
                    let s = &stems[i];
                    // Takes playing the same stem share its level.
                    let target = loudness(&f.frames, |k| s.total(k)) - 10.0 * (sharing(i) as f32).log10();
                    let (l, r) = f.frames.iter().fold((0.0, 0.0), |(l, r), &k| (l + s.left[k], r + s.right[k]));
                    let pan = if l + r > EPS {
                        ((r.sqrt() - l.sqrt()) / (r.sqrt() + l.sqrt())).clamp(-0.8, 0.8)
                    } else {
                        0.0
                    };
                    (target, pan)
                }
                None => (loudness(&f.frames, song) - UNDER_SONG_DB, 0.0),
            };
            TakePlan {
                stem: f.stem,
                gain_db: round_half((target - own).clamp(-30.0, MAX_TAKE_GAIN_DB)),
                pan: round_hundredth(pan),
            }
        })
        .collect();
    let stem_muted = (0..stems.len()).map(|i| found.iter().any(|f| f.stem == Some(i))).collect();
    Plan { stem_muted, takes: takes_out }
}

fn round_half(x: f32) -> f32 {
    (x * 2.0).round() / 2.0
}

fn round_hundredth(x: f32) -> f32 {
    (x * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 44_100;
    const SECS: f64 = 8.0;

    /// A tiny deterministic noise source.
    struct Lcg(u32);
    impl Lcg {
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (self.0 >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
        }
    }

    fn samples() -> usize {
        (SECS * f64::from(RATE)) as usize
    }

    /// Noise bursts on every beat (120 bpm), with a little hi-hat between.
    fn drums(seed: u32, gain: f32) -> Vec<f32> {
        let mut rng = Lcg(seed);
        (0..samples())
            .map(|i| {
                let t = i as f32 / RATE as f32;
                let beat = (t * 2.0).fract();
                let half = (t * 4.0).fract();
                gain * rng.next() * ((-beat * 30.0).exp() + 0.2 * (-half * 60.0).exp())
            })
            .collect()
    }

    /// Low notes changing every 1.5 s on the off-beat.
    fn bass(gain: f32) -> Vec<f32> {
        let notes = [55.0f32, 73.4, 65.4, 49.0, 82.4, 61.7];
        (0..samples())
            .map(|i| {
                let t = i as f32 / RATE as f32 + 0.25;
                let n = (t / 1.5) as usize;
                let local = (t / 1.5).fract() * 1.5;
                gain * (std::f32::consts::TAU * notes[n % notes.len()] * t).sin() * (-local * 1.5).exp()
            })
            .collect()
    }

    /// A sustained mid-range melody with slow vibrato, notes every 0.7 s.
    fn vocals(gain: f32) -> Vec<f32> {
        let notes = [440.0f32, 494.0, 523.0, 587.0, 523.0, 659.0];
        (0..samples())
            .map(|i| {
                let t = i as f32 / RATE as f32 + 0.1;
                let n = (t / 0.7) as usize;
                let f = notes[n % notes.len()] * (1.0 + 0.01 * (t * 30.0).sin());
                let x = std::f32::consts::TAU * f * t;
                gain * (x.sin() + 0.4 * (2.0 * x).sin() + 0.2 * (3.0 * x).sin())
            })
            .collect()
    }

    fn stereo(l: Vec<f32>, r: Vec<f32>) -> StereoBuffer {
        StereoBuffer::new(RATE, l, r).unwrap()
    }

    fn centred(x: Vec<f32>) -> StereoBuffer {
        stereo(x.clone(), x)
    }

    fn song() -> Vec<Profile> {
        let n = timeline_frames(SECS);
        let left_heavy = drums(1, 0.5);
        let right = left_heavy.iter().map(|v| v * 0.5).collect();
        [stereo(left_heavy, right), centred(bass(0.3)), centred(vocals(0.2))]
            .iter()
            .map(|b| Profile::analyse(b, 0.0, n))
            .collect()
    }

    #[test]
    fn a_take_is_matched_to_the_instrument_it_plays_and_set_to_its_level() {
        let stems = song();
        let n = timeline_frames(SECS);
        // A drummer playing the same groove, quieter, with other noise.
        let take = Profile::analyse(&centred(drums(99, 0.15)), 0.0, n);
        let p = plan(&stems, &[take]);
        assert_eq!(p.takes[0].stem, Some(0));
        assert_eq!(p.stem_muted, vec![true, false, false]);
        // The original drums average (0.5 + 0.25) / 2 = 0.375 in mono, the
        // take 0.15: it must come up by 20·log10(2.5) ≈ 8 dB.
        assert!((p.takes[0].gain_db - 8.0).abs() <= 1.5, "{:?}", p.takes[0]);
        assert!(p.takes[0].pan < -0.15, "the original drums lean left: {:?}", p.takes[0]);

        let bass_take = Profile::analyse(&centred(bass(0.6)), 0.0, n);
        let p = plan(&stems, &[bass_take]);
        assert_eq!(p.takes[0].stem, Some(1));
        assert!((p.takes[0].gain_db + 6.0).abs() <= 1.0, "{:?}", p.takes[0]);
        assert_eq!(p.takes[0].pan, 0.0);

        let sung = Profile::analyse(&centred(vocals(0.2)), 0.0, n);
        assert_eq!(plan(&stems, &[sung]).takes[0].stem, Some(2));
    }

    #[test]
    fn takes_recorded_later_in_the_song_are_placed_on_its_timeline() {
        let stems = song();
        let n = timeline_frames(SECS);
        // Starts 3 s in: the file holds the groove from that point on.
        let full = drums(7, 0.05);
        let from = 3 * RATE as usize;
        let take = Profile::analyse(&centred(full[from..].to_vec()), 3.0, n);
        let p = plan(&stems, &[take]);
        assert_eq!(p.takes[0].stem, Some(0));
    }

    #[test]
    fn two_takes_of_one_instrument_share_its_level() {
        let stems = song();
        let n = timeline_frames(SECS);
        let a = Profile::analyse(&centred(drums(5, 0.15)), 0.0, n);
        let one = plan(&stems, std::slice::from_ref(&a)).takes[0].gain_db;
        let b = Profile::analyse(&centred(drums(6, 0.15)), 0.0, n);
        let two = plan(&stems, &[a, b]);
        assert!(two.takes.iter().all(|t| t.stem == Some(0)));
        assert!((one - two.takes[0].gain_db - 3.0).abs() <= 0.5, "{one} vs {:?}", two.takes);
    }

    #[test]
    fn unrelated_or_silent_takes_change_nothing_they_cannot_hear() {
        let stems = song();
        let n = timeline_frames(SECS);
        // A steady hum matches no part: it goes under the song, nothing muted.
        let hum: Vec<f32> = (0..samples()).map(|i| 0.1 * (i as f32 * 0.137).sin()).collect();
        let p = plan(&stems, &[Profile::analyse(&centred(hum), 0.0, n)]);
        assert_eq!(p.takes[0].stem, None);
        assert_eq!(p.stem_muted, vec![false, false, false]);
        let silent = Profile::analyse(&StereoBuffer::silent(RATE, samples()).unwrap(), 0.0, n);
        let p = plan(&stems, &[silent]);
        assert_eq!(p.takes[0], TakePlan { stem: None, gain_db: 0.0, pan: 0.0 });
    }

    #[test]
    fn a_microphone_hearing_the_speakers_is_not_taken_for_one_instrument() {
        let stems = song();
        let n = timeline_frames(SECS);
        let (d, b, v) = (drums(1, 0.5), bass(0.3), vocals(0.2));
        let mut rng = Lcg(3);
        let room: Vec<f32> = (0..samples()).map(|i| 0.1 * (d[i] + b[i] + v[i]) + 0.002 * rng.next()).collect();
        let p = plan(&stems, &[Profile::analyse(&centred(room), 0.0, n)]);
        assert_eq!(p.takes[0].stem, None, "{p:?}");
        assert_eq!(p.stem_muted, vec![false, false, false]);
    }

    #[test]
    fn a_very_quiet_take_stays_within_the_channel_limit() {
        let stems = song();
        let n = timeline_frames(SECS);
        let faint = Profile::analyse(&centred(drums(9, 0.01)), 0.0, n);
        let p = plan(&stems, &[faint]);
        assert_eq!(p.takes[0].stem, Some(0));
        assert_eq!(p.takes[0].gain_db, 12.0);
    }

    #[test]
    fn a_take_of_a_few_frames_is_handled() {
        let stems = song();
        let n = timeline_frames(SECS);
        let blip = drums(4, 0.3)[..RATE as usize / 20].to_vec();
        let p = plan(&stems, &[Profile::analyse(&centred(blip), 2.0, n)]);
        assert_eq!(p.takes.len(), 1);
    }
}
