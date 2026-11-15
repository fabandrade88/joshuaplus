//! Inputs overdriven before they reach the computer.
//!
//! A module or interface with too much level, or with an effect left on,
//! flattens every loud hit against a fixed level: one per polarity, and
//! usually well below full scale because a level control follows the stage
//! that runs out of room. No peak meter shows it (the take "peaks at
//! -8 dB"), yet it can sound distorted and dull.
//!
//! [`detect`] recognises that signature in a recorded take, so the musician
//! can be told where the level is too high. It only reports: the take stays
//! exactly as it arrived. What a hit looked like before it was flattened
//! cannot be read from the recording, and a guess is worse than the fault.
//! Joshua+ 0.5.6 rebuilt the flattened tops from the slopes around them; on
//! a real kit that made some kicks up to 8 dB louder than they were played
//! and left the others alone.
//!
//! Sound that is distorted on purpose is not reported: a fuzz guitar is
//! squashed all the time, an overdriven input only on its loudest hits.

use crate::audio::StereoBuffer;

/// The stage starts to squash around this fraction of its ceiling.
const KNEE: f32 = 0.82;
/// Samples above this fraction of the ceiling sit on it.
const TOP: f32 = 0.93;
/// Quieter samples say nothing about a ceiling (about -18 dBFS).
const FLOOR: f32 = 0.12;
/// Histogram resolution of the ceiling search.
const BIN: f32 = 0.0025;
const MIN_LOUD_SAMPLES: usize = 2_000;
const MIN_BINS: usize = 30;
/// A ceiling piles up at least this many samples in one bin, this far above
/// the trend of the bins below it.
const MIN_PILE: u32 = 300;
const PILE_RATIO: f32 = 1.8;
/// Width (bins) of the pile-up that is averaged into the ceiling.
const CLUSTER_BINS: usize = 12;
/// A dip this short inside a top (a fast transient cutting in) does not end it.
const MERGE_GAP: usize = 5;
/// A top touches the ceiling for at least this many samples.
const MIN_TOP_SAMPLES: usize = 4;
/// Tops at least this long can be judged squashed or round.
const WIDE: usize = 8;
/// Share of a top spent on the ceiling: a round peak that merely reaches it
/// stays near 0.6, a squashed one above 0.85.
const SQUASHED_DWELL: f32 = 0.78;
/// A take is overdriven when at least this many tops are squashed, they are
/// a fair share of the tops reaching the ceiling...
const MIN_SQUASHED: usize = 12;
const MIN_SQUASHED_SHARE: f32 = 0.3;
/// ...and they are the exception: sound squashed for more of its length than
/// this is distorted on purpose.
const MAX_TIME_SHARE: f32 = 0.05;
/// What [`detect`] found in a take.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Saturation {
    /// Lowest level (linear, `0..=1`) the input was squashed at.
    pub ceiling: f32,
    /// Stretches that arrived squashed, over both channels and polarities.
    pub tops: usize,
}

impl Saturation {
    /// The ceiling in dBFS.
    #[must_use]
    pub fn ceiling_db(&self) -> f32 {
        20.0 * self.ceiling.max(1e-6).log10()
    }
}

/// One stretch above the knee that touches the ceiling.
#[derive(Debug, Clone, Copy)]
struct Top {
    start: usize,
    /// One past the last sample.
    end: usize,
    on_ceiling: usize,
}

impl Top {
    const fn len(&self) -> usize {
        self.end - self.start
    }

    fn is_squashed(&self) -> bool {
        self.len() >= WIDE && self.on_ceiling as f32 >= SQUASHED_DWELL * self.len() as f32
    }
}

/// Says whether a take arrived overdriven, and at what level. `None` for a
/// clean take, and for one that is distorted throughout (which is a sound,
/// not a fault). The take itself is never changed.
#[must_use]
pub fn detect(audio: &StereoBuffer) -> Option<Saturation> {
    [audio.left(), audio.right()]
        .into_iter()
        .flat_map(|x| [squashed(x, 1.0), squashed(x, -1.0)])
        .flatten()
        .reduce(|a, b| Saturation { ceiling: a.ceiling.min(b.ceiling), tops: a.tops + b.tops })
}

/// Decides whether one polarity of `x` was overdriven, and where.
fn squashed(x: &[f32], sign: f32) -> Option<Saturation> {
    let ceiling = find_ceiling(x, sign)?;
    let tops = find_tops(x, sign, ceiling);
    let wide = tops.iter().filter(|t| t.len() >= WIDE).count();
    let squashed = tops.iter().filter(|t| t.is_squashed()).count();
    let squashed_samples: usize = tops.iter().filter(|t| t.is_squashed()).map(Top::len).sum();
    let overdriven = squashed >= MIN_SQUASHED
        && squashed as f32 >= MIN_SQUASHED_SHARE * wide as f32
        && squashed_samples as f32 <= MAX_TIME_SHARE * x.len() as f32;
    overdriven.then_some(Saturation { ceiling, tops: squashed })
}

/// Level where one polarity of `x` piles up: a stage that saturates leaves
/// far more samples just under its ceiling than the trend below predicts.
fn find_ceiling(x: &[f32], sign: f32) -> Option<f32> {
    let (mut loud, mut max) = (0usize, 0.0f32);
    for v in x {
        let s = v * sign;
        if s > FLOOR {
            loud += 1;
            max = max.max(s);
        }
    }
    if loud < MIN_LOUD_SAMPLES {
        return None;
    }
    let bins = ((max.min(1.0) - FLOOR) / BIN).ceil() as usize;
    if bins < MIN_BINS {
        return None;
    }
    let mut hist = vec![0u32; bins];
    for v in x {
        let s = v * sign;
        if s > FLOOR {
            // Saturating cast: anything past the last bin is ignored.
            if let Some(h) = hist.get_mut(((s - FLOOR) / BIN) as usize) {
                *h += 1;
            }
        }
    }
    let mut window = Vec::with_capacity(18);
    let mut strong = Vec::new();
    for (i, &h) in hist.iter().enumerate() {
        // Trend: median of the bins 6..24 below this one.
        window.clear();
        window.extend_from_slice(&hist[i.saturating_sub(24)..i.saturating_sub(6).max(1)]);
        window.sort_unstable();
        let mid = window.len() / 2;
        let median = if window.len() % 2 == 0 {
            (window[mid - 1] as f32 + window[mid] as f32) * 0.5
        } else {
            window[mid] as f32
        };
        if h > MIN_PILE && h as f32 > PILE_RATIO * (median + 20.0) {
            strong.push(i);
        }
    }
    let top = *strong.last()?;
    let (mut sum, mut weight) = (0.0f64, 0.0f64);
    for &i in strong.iter().filter(|&&i| i + CLUSTER_BINS >= top) {
        let w = f64::from(hist[i]);
        sum += (f64::from(FLOOR) + i as f64 * f64::from(BIN)) * w;
        weight += w;
    }
    (weight > 0.0).then(|| (sum / weight) as f32 + BIN * 0.5)
}

/// Stretches of `x` (one polarity) above the knee that touch the ceiling.
fn find_tops(x: &[f32], sign: f32, ceiling: f32) -> Vec<Top> {
    let (knee, top) = (ceiling * KNEE, ceiling * TOP);
    let mut tops = Vec::new();
    let mut pending: Option<Top> = None;
    let mut i = 0;
    // False for a NaN, which is skipped like a quiet sample.
    let above_knee = |v: f32| v * sign >= knee;
    while i < x.len() {
        if !above_knee(x[i]) {
            i += 1;
            continue;
        }
        let start = i;
        let mut on_ceiling = 0;
        while i < x.len() && above_knee(x[i]) {
            on_ceiling += usize::from(x[i] * sign >= top);
            i += 1;
        }
        match pending.as_mut() {
            Some(p) if start - p.end <= MERGE_GAP => {
                p.end = i;
                p.on_ceiling += on_ceiling;
            }
            _ => {
                tops.extend(pending.take().filter(|p| p.on_ceiling >= MIN_TOP_SAMPLES));
                pending = Some(Top { start, end: i, on_ceiling });
            }
        }
    }
    tops.extend(pending.filter(|p| p.on_ceiling >= MIN_TOP_SAMPLES));
    tops
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// Small deterministic generator (tests must not depend on a seed crate).
    struct Rng(u64);

    impl Rng {
        fn unit(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 40) as f32 / (1u64 << 24) as f32
        }
    }

    /// Drum-like hits: decaying 62 Hz and 200 Hz bursts of varying strength
    /// over a little hiss, `secs` long. `level` scales the strongest hit.
    fn kit(secs: usize, level: f32) -> Vec<f32> {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let n = secs * RATE as usize;
        let mut x: Vec<f32> = (0..n).map(|_| (rng.unit() - 0.5) * 0.006).collect();
        let mut hit = 0;
        let mut start = RATE as usize / 5;
        while start + RATE as usize / 2 < n {
            let amp = level * (0.15 + 0.85 * rng.unit());
            let (freq, decay) = if hit % 2 == 0 { (62.0, 12.0) } else { (200.0, 18.0) };
            for k in 0..RATE as usize / 2 {
                let t = k as f32 / RATE as f32;
                x[start + k] += amp * (-t * decay).exp() * (std::f32::consts::TAU * freq * t).sin();
            }
            hit += 1;
            start += RATE as usize / 3;
        }
        x
    }

    /// An analog stage with a soft knee, squashing at `pos` and `-neg`.
    fn overdrive(x: &[f32], pos: f32, neg: f32) -> Vec<f32> {
        x.iter()
            .map(|&v| {
                let c = if v >= 0.0 { pos } else { neg };
                let a = v.abs();
                let knee = c * 0.8;
                if a <= knee { v } else { (knee + (c - knee) * ((a - knee) / (c - knee)).tanh()).copysign(v) }
            })
            .collect()
    }

    fn stereo(x: &[f32]) -> StereoBuffer {
        StereoBuffer::from_mono(RATE, x.to_vec()).unwrap()
    }

    #[test]
    fn an_overdriven_take_is_reported_at_its_ceiling() {
        let squashed = overdrive(&kit(30, 1.0), 0.315, 0.382);
        let found = detect(&stereo(&squashed)).expect("overdriven take must be recognised");
        // Both ceilings are found; the lower one is reported.
        assert!((found.ceiling - 0.315).abs() < 0.012, "ceiling {}", found.ceiling);
        assert!((found.ceiling_db() + 10.0).abs() < 0.5, "{} dB", found.ceiling_db());
        assert!(found.tops >= 2 * MIN_SQUASHED, "{} tops", found.tops);
    }

    #[test]
    fn a_kick_flattened_before_the_interface_is_reported() {
        // The take that showed rebuilding was a mistake: 25 s, the first
        // swing of every kick arriving flat at 0.33, everything else round.
        let mut x = vec![0.0f32; 25 * RATE as usize];
        let kicks = 15;
        for hit in 0..2 * kicks {
            let start = RATE as usize / 2 + hit * (RATE as usize * 3 / 4);
            let kick = hit % 2 == 0;
            let (freq, level, decay) = if kick { (200.0, 0.9, 350.0) } else { (190.0, 0.25, 30.0) };
            for k in 0..RATE as usize / 4 {
                let t = k as f32 / RATE as f32;
                let v: f32 = level * (-t * decay).exp() * (std::f32::consts::TAU * freq * t).sin();
                x[start + k] += v.clamp(-0.33, 0.33);
            }
        }
        let found = detect(&stereo(&x)).expect("flattened kicks must be recognised");
        assert!((found.ceiling - 0.33).abs() < 0.012, "ceiling {}", found.ceiling);
        // Every kick, on both channels of the mono source; nothing else.
        assert_eq!(found.tops, 2 * kicks);
    }

    #[test]
    fn clean_takes_are_not_reported() {
        // Dynamic playing, well inside the input's range.
        assert!(detect(&stereo(&kit(30, 0.5))).is_none());
        // Every hit at the same strength (an e-drum at full velocity): the
        // peaks repeat at one level but stay round.
        let mut same = vec![0.0f32; 30 * RATE as usize];
        for hit in 0..85 {
            let start = RATE as usize / 5 + hit * RATE as usize / 3;
            let freq = [62.0, 200.0, 110.0][hit % 3];
            for k in 0..RATE as usize / 3 {
                let t = k as f32 / RATE as f32;
                same[start + k] += 0.5 * (-t * 14.0).exp() * (std::f32::consts::TAU * freq * t).sin();
            }
        }
        assert!(detect(&stereo(&same)).is_none());
        // A steady tone piles up at its peak level too, and is just as round.
        let tone: Vec<f32> = (0..20 * RATE as usize)
            .map(|k| 0.5 * (std::f32::consts::TAU * 100.0 * k as f32 / RATE as f32).sin())
            .collect();
        assert!(detect(&stereo(&tone)).is_none());
        // Silence and scraps.
        assert!(detect(&StereoBuffer::silent(RATE, RATE as usize).unwrap()).is_none());
        assert!(detect(&stereo(&[0.9; 7])).is_none());
    }

    #[test]
    fn distortion_on_purpose_is_not_a_fault() {
        // A fuzz guitar: squashed for as long as a note rings.
        let fuzz: Vec<f32> = (0..20 * RATE as usize)
            .map(|k| {
                let t = k as f32 / RATE as f32;
                let note = 0.9 * (1.0 - (t % 1.0) * 0.5) * (std::f32::consts::TAU * 196.0 * t).sin();
                note.clamp(-0.35, 0.35)
            })
            .collect();
        assert!(detect(&stereo(&fuzz)).is_none());
    }

    #[test]
    fn one_polarity_or_one_channel_is_enough() {
        // One polarity overdriven (a stage with an offset).
        let squashed = overdrive(&kit(30, 1.0), 0.3, 4.0);
        let found = detect(&stereo(&squashed)).expect("recognised");
        assert!((found.ceiling - 0.3).abs() < 0.012);
        // One channel overdriven, the other clean (two sources on a pair).
        let pair = StereoBuffer::new(RATE, squashed, kit(30, 0.4)).unwrap();
        let alone = detect(&pair).expect("recognised");
        assert!((alone.ceiling - 0.3).abs() < 0.012);
        assert!(alone.tops < found.tops);
    }

    #[test]
    fn odd_values_never_panic() {
        let mut x = overdrive(&kit(10, 1.0), 0.315, 0.382);
        x[1_000] = f32::NAN;
        x[2_000] = f32::INFINITY;
        x[3_000] = f32::NEG_INFINITY;
        let _ = detect(&stereo(&x));
    }
}
