//! Mastering for exports: loudness measured as streaming platforms do
//! (ITU-R BS.1770 integrated loudness, LUFS) and a look-ahead peak limiter.
//!
//! A mix of a finished record plus a take easily runs over full scale: a
//! sample-by-sample safety clipper then flattens every kick and tom hit. For
//! exports the mix is instead brought to the platforms' loudness and kept
//! under a ceiling by a limiter that sees peaks coming and lowers the gain
//! smoothly around them, so transients keep their shape.

use std::collections::VecDeque;

/// Loudness YouTube, Spotify and Instagram play at (LUFS).
pub const STREAMING_LUFS: f32 = -14.0;
/// Peak ceiling of the exports (dBFS), with room for codec overshoot.
pub const CEILING_DB: f32 = -1.0;
/// How far an export may be turned up to reach the target (quiet mixes).
pub const MAX_BOOST_DB: f32 = 6.0;

/// Second-order IIR section (direct form I).
#[derive(Debug, Clone, Copy, Default)]
struct Section {
    b: [f64; 3],
    a: [f64; 2],
    x: [f64; 2],
    y: [f64; 2],
}

impl Section {
    fn run(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.b[1] * self.x[0] + self.b[2] * self.x[1]
            - self.a[0] * self.y[0]
            - self.a[1] * self.y[1];
        self.x = [x, self.x[0]];
        self.y = [y, self.y[0]];
        y
    }
}

/// The two K-weighting filters of BS.1770 for `rate` Hz (high shelf, then
/// high-pass), designed from their analogue prototypes so any rate works.
fn k_weighting(rate: f64) -> [Section; 2] {
    let shelf = {
        let (f0, g, q) = (1_681.974_450_955_533, 3.999_843_853_973_347, 0.707_175_236_955_419_6);
        let k = (std::f64::consts::PI * f0 / rate).tan();
        let vh = 10f64.powf(g / 20.0);
        let vb = vh.powf(0.499_666_774_154_541_6);
        let a0 = 1.0 + k / q + k * k;
        Section {
            b: [(vh + vb * k / q + k * k) / a0, 2.0 * (k * k - vh) / a0, (vh - vb * k / q + k * k) / a0],
            a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
            ..Section::default()
        }
    };
    let highpass = {
        let (f0, q) = (38.135_470_876_024_44, 0.500_327_037_323_877_3);
        let k = (std::f64::consts::PI * f0 / rate).tan();
        let a0 = 1.0 + k / q + k * k;
        Section { b: [1.0, -2.0, 1.0], a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0], ..Section::default() }
    };
    [shelf, highpass]
}

/// Integrated loudness (BS.1770-4: 400 ms blocks overlapping by 75 %,
/// absolute gate at −70 LUFS, relative gate 10 LU below) of a stereo stream.
#[derive(Debug)]
pub struct LoudnessMeter {
    filters: [[Section; 2]; 2],
    /// Mean square of each 100 ms step.
    steps: Vec<f64>,
    acc: f64,
    count: usize,
    step_len: usize,
}

impl LoudnessMeter {
    #[must_use]
    pub fn new(rate: u32) -> Self {
        let k = k_weighting(f64::from(rate));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let step_len = (f64::from(rate) * 0.1).round() as usize;
        Self { filters: [k, k], steps: Vec::new(), acc: 0.0, count: 0, step_len: step_len.max(1) }
    }

    pub fn add(&mut self, left: &[f32], right: &[f32]) {
        for (l, r) in left.iter().zip(right) {
            let mut e = 0.0;
            for (ch, x) in [f64::from(*l), f64::from(*r)].into_iter().enumerate() {
                let y = self.filters[ch].iter_mut().fold(x, |v, s| s.run(v));
                e += y * y;
            }
            self.acc += e;
            self.count += 1;
            if self.count == self.step_len {
                self.steps.push(self.acc / self.step_len as f64);
                self.acc = 0.0;
                self.count = 0;
            }
        }
    }

    /// Integrated loudness in LUFS, `None` for silence or under 400 ms.
    #[must_use]
    pub fn integrated(&self) -> Option<f32> {
        let blocks: Vec<f64> = self.steps.windows(4).map(|w| w.iter().sum::<f64>() / 4.0).collect();
        let lufs = |ms: f64| -0.691 + 10.0 * ms.log10();
        let above =
            |gate: f64| -> Vec<f64> { blocks.iter().copied().filter(|b| *b > 0.0 && lufs(*b) > gate).collect() };
        let absolute = above(-70.0);
        if absolute.is_empty() {
            return None;
        }
        let relative = lufs(absolute.iter().sum::<f64>() / absolute.len() as f64) - 10.0;
        let gated = above(relative.max(-70.0));
        #[allow(clippy::cast_possible_truncation)]
        let v = lufs(gated.iter().sum::<f64>() / gated.len().max(1) as f64) as f32;
        v.is_finite().then_some(v)
    }
}

/// Stereo-linked look-ahead peak limiter. Output is the input delayed by
/// [`PeakLimiter::latency`] frames, its gain lowered just enough, early
/// enough and smoothly enough that no sample exceeds the ceiling.
#[derive(Debug)]
pub struct PeakLimiter {
    ceiling: f32,
    look: usize,
    release: f32,
    delay: VecDeque<(f32, f32)>,
    /// Indices into `window` of an increasing sequence: its front is the
    /// window's minimum (sliding minimum).
    minima: VecDeque<(usize, f32)>,
    held: f32,
    /// Box average of the held gain over `look` frames.
    boxed: VecDeque<f32>,
    box_sum: f32,
    index: usize,
}

impl PeakLimiter {
    /// A limiter for `rate` Hz with its ceiling in dBFS: 5 ms look-ahead,
    /// 150 ms release.
    #[must_use]
    pub fn new(rate: u32, ceiling_db: f32) -> Self {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let look = ((f64::from(rate) * 0.005).round() as usize).max(1);
        #[allow(clippy::cast_precision_loss)]
        let release = 1.0 - (-1.0 / (0.15 * rate as f32)).exp();
        Self {
            ceiling: 10f32.powf(ceiling_db / 20.0),
            look,
            release,
            delay: VecDeque::with_capacity(look + 1),
            minima: VecDeque::with_capacity(look + 1),
            held: 1.0,
            boxed: VecDeque::from(vec![1.0; look]),
            box_sum: look as f32,
            index: 0,
        }
    }

    /// Frames between a sample going in and coming out.
    #[must_use]
    pub const fn latency(&self) -> usize {
        self.look
    }

    /// Feeds one stereo frame; returns the frame `latency` frames back.
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let peak = l.abs().max(r.abs());
        let need = if peak > self.ceiling { self.ceiling / peak } else { 1.0 };
        // Sliding minimum of the needed gain over the next `look` frames.
        while self.minima.back().is_some_and(|(_, g)| *g >= need) {
            self.minima.pop_back();
        }
        self.minima.push_back((self.index, need));
        while self.minima.front().is_some_and(|(i, _)| *i + self.look <= self.index) {
            self.minima.pop_front();
        }
        let target = self.minima.front().map_or(1.0, |(_, g)| *g);
        self.index += 1;
        // Down at once (the box filter smooths it), up slowly.
        self.held = if target < self.held { target } else { self.held + (target - self.held) * self.release };
        // The box average over `look` frames of a value that is at most the
        // need of every frame in its window never exceeds a frame's need.
        self.box_sum += self.held - self.boxed.pop_front().unwrap_or(1.0);
        self.boxed.push_back(self.held);
        #[allow(clippy::cast_precision_loss)]
        let gain = (self.box_sum / self.look as f32).min(1.0);
        self.delay.push_back((l, r));
        if self.delay.len() <= self.look {
            return (0.0, 0.0);
        }
        let (dl, dr) = self.delay.pop_front().unwrap_or((0.0, 0.0));
        // Safety net for float rounding: never past the ceiling.
        let c = self.ceiling;
        ((dl * gain).clamp(-c, c), (dr * gain).clamp(-c, c))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn sine(freq: f32, amp: f32, secs: f32) -> Vec<f32> {
        (0..(secs * RATE as f32) as usize)
            .map(|i| amp * (std::f32::consts::TAU * freq * i as f32 / RATE as f32).sin())
            .collect()
    }

    #[test]
    fn loudness_matches_the_standard() {
        // BS.1770 reference: a 997 Hz sine at 0 dBFS in one channel reads
        // −3.01 LUFS; in both channels of a stereo programme, 0 LUFS.
        let s = sine(997.0, 1.0, 5.0);
        let mut one = LoudnessMeter::new(RATE);
        one.add(&s, &vec![0.0; s.len()]);
        assert!((one.integrated().unwrap() + 3.01).abs() < 0.1, "{:?}", one.integrated());
        for (amp, want) in [(1.0, 0.0), (0.1, -20.0)] {
            let s = sine(997.0, amp, 5.0);
            let mut m = LoudnessMeter::new(RATE);
            m.add(&s, &s);
            let got = m.integrated().unwrap();
            assert!((got - want).abs() < 0.1, "{got} vs {want}");
        }
        // Rate independent.
        let s44: Vec<f32> =
            (0..44_100 * 5).map(|i| (std::f32::consts::TAU * 997.0 * i as f32 / 44_100.0).sin()).collect();
        let mut m = LoudnessMeter::new(44_100);
        m.add(&s44, &vec![0.0; s44.len()]);
        assert!((m.integrated().unwrap() + 3.01).abs() < 0.1);
        // Silence has no loudness; the gate ignores quiet stretches.
        let mut quiet = LoudnessMeter::new(RATE);
        quiet.add(&vec![0.0; 96_000], &vec![0.0; 96_000]);
        assert_eq!(quiet.integrated(), None);
        let mut gated = LoudnessMeter::new(RATE);
        let s = sine(997.0, 0.1, 5.0);
        gated.add(&s, &s);
        gated.add(&vec![0.0; RATE as usize * 5], &vec![0.0; RATE as usize * 5]);
        // (The few blocks across the cut, partly sound, count as the standard says.)
        let g = gated.integrated().unwrap();
        assert!((g + 20.0).abs() < 0.3, "silence does not drag it down: {g}");
    }

    fn limit(input: &[f32], ceiling_db: f32) -> Vec<f32> {
        let mut lim = PeakLimiter::new(RATE, ceiling_db);
        let lat = lim.latency();
        let mut out: Vec<f32> =
            input.iter().chain(std::iter::repeat_n(&0.0, lat)).map(|x| lim.process(*x, *x).0).collect();
        out.drain(..lat);
        out
    }

    #[test]
    fn the_limiter_keeps_peaks_under_the_ceiling_without_moving_them() {
        // A kick-like hit at 2.0 (6 dB over full scale) on a quiet bed.
        let mut x = sine(110.0, 0.2, 1.0);
        for i in 0..2_400 {
            x[24_000 + i] +=
                1.8 * (-(i as f32) / 600.0).exp() * (std::f32::consts::TAU * 60.0 * i as f32 / RATE as f32).sin();
        }
        let y = limit(&x, -1.0);
        assert_eq!(y.len(), x.len());
        let ceiling = 10f32.powf(-1.0 / 20.0);
        assert!(y.iter().all(|v| v.abs() <= ceiling + 1e-6));
        // Far from the hit the signal passes untouched (aligned, not delayed).
        assert!(y[..20_000].iter().zip(&x[..20_000]).all(|(a, b)| (a - b).abs() < 1e-6));
        // The hit keeps its shape: lowered, not clipped (no flat tops).
        let flat = y
            .windows(3)
            .filter(|w| {
                (w[0].abs() - ceiling).abs() < 1e-4
                    && (w[1].abs() - ceiling).abs() < 1e-4
                    && (w[2].abs() - ceiling).abs() < 1e-4
            })
            .count();
        assert!(flat < 3, "{flat} clipped runs");
    }

    #[test]
    fn a_quiet_signal_goes_through_bit_for_bit() {
        let x = sine(440.0, 0.5, 0.5);
        assert_eq!(limit(&x, -1.0), x);
    }
}
