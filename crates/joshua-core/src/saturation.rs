//! Inputs overdriven before they reach the computer.
//!
//! An interface or module with too much gain, or with an effect left on,
//! squashes every loud hit against a fixed level: one per polarity, and
//! usually well below full scale because a level control follows the stage
//! that saturates. No peak meter shows it (the take "peaks at -8 dB"), yet
//! it sounds distorted and dull, and nothing after the fact can know what
//! the hits looked like.
//!
//! [`repair`] recognises that signature in a recorded take and rebuilds the
//! squashed tops: each one is treated as missing and replaced by a cubic
//! arch that continues the slopes the wave had on its way up and down,
//! measured below the knee where the stage was still linear. It is an
//! estimate, far closer to the hit than a flat top, and the caller is told
//! so the musician can fix the gain at the source.
//!
//! Sound that is distorted on purpose is left alone: a fuzz guitar is
//! squashed all the time, an overdriven input only on its loudest hits.

use crate::audio::StereoBuffer;

/// The stage starts to squash around this fraction of its ceiling.
const KNEE: f32 = 0.82;
/// Samples above this fraction of the ceiling sit on it.
const TOP: f32 = 0.93;
/// Slopes are measured between this fraction of the ceiling and the knee.
const FIT_LOW: f32 = 0.40;
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
/// Nothing is rebuilt beyond this multiple of the ceiling (+8 dB), narrow
/// tops less (they cannot hide much), and never past full scale.
const MAX_REBUILD: f32 = 2.6;
const REBUILD_PER_SAMPLE: f32 = 0.03;
const HEADROOM: f32 = 0.98;
/// Samples needed on either side of a top to read its flanks.
const EDGE: usize = 32;
const FIT_POINTS: usize = 24;

/// What [`repair`] found in a take.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Saturation {
    /// Lowest level (linear, `0..=1`) the input was squashed at.
    pub ceiling: f32,
    /// Squashed tops that were rebuilt.
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

/// One polarity of one channel that was overdriven.
#[derive(Debug)]
struct Plan {
    sign: f32,
    ceiling: f32,
    tops: Vec<Top>,
}

/// Rebuilds the squashed hits of an overdriven take. Returns `None`, leaving
/// the take alone, when it was not overdriven (or is distorted throughout,
/// which is a sound, not a fault).
#[must_use]
pub fn repair(audio: &StereoBuffer) -> Option<(StereoBuffer, Saturation)> {
    let plans = [audio.left(), audio.right()].map(|x| [plan(x, 1.0), plan(x, -1.0)]);
    // A clean take costs two passes over it and no copy.
    if plans.iter().flatten().all(Option::is_none) {
        return None;
    }
    let mut found: Option<Saturation> = None;
    let mut channels = [audio.left(), audio.right()].map(<[f32]>::to_vec);
    for ((src, dst), plans) in [audio.left(), audio.right()].into_iter().zip(&mut channels).zip(&plans) {
        for p in plans.iter().flatten() {
            let tops = rebuild(src, dst, p);
            if tops == 0 {
                continue;
            }
            found = Some(match found {
                Some(f) => Saturation { ceiling: f.ceiling.min(p.ceiling), tops: f.tops + tops },
                None => Saturation { ceiling: p.ceiling, tops },
            });
        }
    }
    let found = found?;
    let [left, right] = channels;
    StereoBuffer::new(audio.sample_rate(), left, right).ok().map(|fixed| (fixed, found))
}

/// Decides whether one polarity of `x` was overdriven, and where.
fn plan(x: &[f32], sign: f32) -> Option<Plan> {
    let ceiling = find_ceiling(x, sign)?;
    let tops = find_tops(x, sign, ceiling);
    let wide = tops.iter().filter(|t| t.len() >= WIDE).count();
    let squashed = tops.iter().filter(|t| t.is_squashed()).count();
    let squashed_samples: usize = tops.iter().filter(|t| t.is_squashed()).map(Top::len).sum();
    let overdriven = squashed >= MIN_SQUASHED
        && squashed as f32 >= MIN_SQUASHED_SHARE * wide as f32
        && squashed_samples as f32 <= MAX_TIME_SHARE * x.len() as f32;
    overdriven.then_some(Plan { sign, ceiling, tops })
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

/// Slope (per sample) of the flank next to a top, walking away from it from
/// `at`: a least-squares line through the samples down to [`FIT_LOW`].
fn flank_slope(x: &[f32], sign: f32, at: usize, forward: bool, ceiling: f32) -> f32 {
    let low = ceiling * FIT_LOW;
    let mut points = [(0.0f32, 0.0f32); FIT_POINTS];
    let mut n = 0;
    let mut i = at;
    while n < FIT_POINTS {
        let v = x[i] * sign;
        if v < low && n >= 3 {
            break;
        }
        // Past the flank: the other polarity, or another drum rising on top.
        if v < -ceiling * 0.2 || (n >= 2 && v - points[n - 1].1 > ceiling * 0.08) {
            break;
        }
        points[n] = (i as f32 - at as f32, v);
        n += 1;
        let next = if forward { i.checked_add(1).filter(|j| *j < x.len()) } else { i.checked_sub(1) };
        match next {
            Some(j) => i = j,
            None => break,
        }
    }
    if n < 3 {
        return 0.0;
    }
    let points = &points[..n];
    let (mk, mv) = points.iter().fold((0.0, 0.0), |(k, v), p| (k + p.0, v + p.1));
    let (mk, mv) = (mk / n as f32, mv / n as f32);
    let (mut skv, mut skk) = (0.0f32, 0.0f32);
    for (k, v) in points {
        skv += (k - mk) * (v - mv);
        skk += (k - mk) * (k - mk);
    }
    if skk > 0.0 { skv / skk } else { 0.0 }
}

/// Rebuilds the tops of `plan` from `src` into `dst`. Returns how many.
fn rebuild(src: &[f32], dst: &mut [f32], plan: &Plan) -> usize {
    let (sign, ceiling) = (plan.sign, plan.ceiling);
    let mut rebuilt = 0;
    for top in &plan.tops {
        if top.start < EDGE + 1 || top.end + EDGE > src.len() {
            continue;
        }
        // Last sample below the knee before the top, first one after it.
        let (i0, i1) = (top.start - 1, top.end);
        let w = (i1 - i0) as f32;
        // A wave going into a flat top was rising, and falling when it left.
        let m0 = flank_slope(src, sign, i0, false, ceiling).max(0.0);
        let m1 = flank_slope(src, sign, i1, true, ceiling).min(0.0);
        if m0 == 0.0 && m1 == 0.0 {
            continue;
        }
        // One flank can be unreadable (another drum on top of it): an arch
        // needs both, so the weak side borrows from the other.
        let m0 = m0.max(-m1 * 0.5);
        let m1 = m1.min(-m0 * 0.5);
        let (y0, y1) = (src[i0] * sign, src[i1] * sign);
        let arch = |j: usize| {
            let t = j as f32 / w;
            let (t2, t3) = (t * t, t * t * t);
            let y = (2.0 * t3 - 3.0 * t2 + 1.0) * y0
                + (t3 - 2.0 * t2 + t) * w * m0
                + (-2.0 * t3 + 3.0 * t2) * y1
                + (t3 - t2) * w * m1;
            (y, y0 + (y1 - y0) * t)
        };
        let (mut peak, mut chord_peak) = (f32::MIN, f32::MIN);
        for j in 1..i1 - i0 {
            let (y, chord) = arch(j);
            peak = peak.max(y);
            chord_peak = chord_peak.max(chord);
        }
        let cap = (ceiling * MAX_REBUILD.min(1.0 + REBUILD_PER_SAMPLE * w)).min(HEADROOM);
        let over = peak - chord_peak;
        let scale = if peak > cap && over > 0.0 { (cap - chord_peak).max(0.0) / over } else { 1.0 };
        for j in 1..i1 - i0 {
            let (y, chord) = arch(j);
            // Never below what was recorded.
            let y = (chord + (y - chord) * scale).max(src[i0 + j] * sign);
            dst[i0 + j] = y * sign;
        }
        rebuilt += 1;
    }
    rebuilt
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

    fn error_db(a: &[f32], reference: &[f32]) -> f32 {
        let err: f64 = a.iter().zip(reference).map(|(x, y)| f64::from(x - y).powi(2)).sum();
        let power: f64 = reference.iter().map(|y| f64::from(*y).powi(2)).sum();
        (10.0 * (err / power).log10()) as f32
    }

    fn stereo(x: &[f32]) -> StereoBuffer {
        StereoBuffer::from_mono(RATE, x.to_vec()).unwrap()
    }

    #[test]
    fn an_overdriven_take_gets_its_hits_back() {
        let clean = kit(30, 1.0);
        let squashed = overdrive(&clean, 0.315, 0.382);
        let (fixed, found) = repair(&stereo(&squashed)).expect("overdriven take must be recognised");
        // Both ceilings are found; the lower one is reported.
        assert!((found.ceiling - 0.315).abs() < 0.012, "ceiling {}", found.ceiling);
        assert!((found.ceiling_db() + 10.0).abs() < 0.5, "{} dB", found.ceiling_db());
        assert!(found.tops >= 2 * MIN_SQUASHED, "{} tops", found.tops);
        // Much closer to what was played than the squashed recording.
        let (before, after) = (error_db(&squashed, &clean), error_db(fixed.left(), &clean));
        assert!(after < before - 10.0, "error {before:.1} dB -> {after:.1} dB");
        // Rebuilt, not invented: within full scale, never below the recording,
        // and untouched wherever the stage was linear.
        let peak = fixed.left().iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.6 && peak <= HEADROOM + 1e-6, "peak {peak}");
        for (f, s) in fixed.left().iter().zip(&squashed) {
            assert!(f.abs() >= s.abs() - 1e-6 && f.signum() * s.signum() >= 0.0);
            if s.abs() < found.ceiling * KNEE * 0.9 {
                assert_eq!(f, s);
            }
        }
        assert_eq!(fixed.left(), fixed.right());
        assert_eq!(fixed.frames(), squashed.len());
    }

    #[test]
    fn clean_takes_are_left_alone() {
        // Dynamic playing, well inside the input's range.
        assert!(repair(&stereo(&kit(30, 0.5))).is_none());
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
        assert!(repair(&stereo(&same)).is_none());
        // A steady tone piles up at its peak level too, and is just as round.
        let tone: Vec<f32> = (0..20 * RATE as usize)
            .map(|k| 0.5 * (std::f32::consts::TAU * 100.0 * k as f32 / RATE as f32).sin())
            .collect();
        assert!(repair(&stereo(&tone)).is_none());
        // Silence and scraps.
        assert!(repair(&StereoBuffer::silent(RATE, RATE as usize).unwrap()).is_none());
        assert!(repair(&stereo(&[0.9; 7])).is_none());
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
        assert!(repair(&stereo(&fuzz)).is_none());
    }

    #[test]
    fn only_what_was_squashed_changes() {
        // One polarity overdriven (a stage with an offset): the other half of
        // the wave is returned bit for bit.
        let clean = kit(30, 1.0);
        let squashed = overdrive(&clean, 0.3, 4.0);
        let (fixed, found) = repair(&stereo(&squashed)).expect("recognised");
        assert!((found.ceiling - 0.3).abs() < 0.012);
        for (f, s) in fixed.left().iter().zip(&squashed) {
            if *s <= 0.0 {
                assert_eq!(f, s);
            }
        }
        // One channel overdriven, the other clean (two sources on a pair).
        let pair = StereoBuffer::new(RATE, squashed.clone(), kit(30, 0.4)).unwrap();
        let (fixed, _) = repair(&pair).expect("recognised");
        assert_eq!(fixed.right(), pair.right());
        assert_ne!(fixed.left(), pair.left());
    }

    #[test]
    fn a_ceiling_at_full_scale_leaves_no_room_to_overshoot() {
        let squashed = overdrive(&kit(30, 3.0), 0.97, 0.97);
        if let Some((fixed, _)) = repair(&stereo(&squashed)) {
            assert!(fixed.left().iter().all(|v| v.abs() <= 1.0));
        }
    }

    #[test]
    fn odd_values_never_panic() {
        let mut x = overdrive(&kit(10, 1.0), 0.315, 0.382);
        x[1_000] = f32::NAN;
        x[2_000] = f32::INFINITY;
        x[3_000] = f32::NEG_INFINITY;
        let _ = repair(&stereo(&x));
    }
}
