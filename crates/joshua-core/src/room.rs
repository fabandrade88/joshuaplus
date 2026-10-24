//! A small shared "room" for recorded takes.
//!
//! A take recorded straight from a drum module or a DI box is completely
//! dry, while the song around it was mixed with its own space, so the take
//! sounds pasted on top. Sending some of it into one shared room puts every
//! take in the same place as the band.
//!
//! The room is a Freeverb-style network (Jezar's public-domain design):
//! eight damped feedback combs in parallel and four allpasses in series per
//! side, the right side offset for stereo width, tuned short (~0.7 s) like a
//! rehearsal room rather than a hall. A short pre-delay keeps the attack of
//! the dry signal clear. All buffers are allocated up front for the highest
//! supported sample rate, so processing never allocates.

/// Highest sample rate the buffers are sized for.
const MAX_RATE: u32 = 192_000;
/// Delay lengths below are for 44.1 kHz and scale with the sample rate.
const REF_RATE: f64 = 44_100.0;
const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
const STEREO_SPREAD: usize = 23;
const PREDELAY_SECS: f64 = 0.012;

/// Comb feedback: a ~0.7 s decay (small live room).
const FEEDBACK: f32 = 0.74;
/// High-frequency damping inside the combs (walls absorb treble).
const DAMP: f32 = 0.35;
const ALLPASS_FEEDBACK: f32 = 0.5;
/// Input scaling of the comb bank and output level of the wet signal.
const INPUT_GAIN: f32 = 0.015;
const WET_GAIN: f32 = 2.4;

#[derive(Debug)]
struct Line {
    buf: Vec<f32>,
    len: usize,
    at: usize,
}

impl Line {
    fn new(max: usize) -> Self {
        Self { buf: vec![0.0; max.max(1)], len: 1, at: 0 }
    }

    fn set_len(&mut self, len: usize) {
        self.len = len.clamp(1, self.buf.len());
        self.at = 0;
        self.buf.fill(0.0);
    }

    fn clear(&mut self) {
        self.buf[..self.len].fill(0.0);
        self.at = 0;
    }

    #[inline]
    fn read(&self) -> f32 {
        self.buf[self.at]
    }

    #[inline]
    fn write_advance(&mut self, v: f32) {
        self.buf[self.at] = v;
        self.at += 1;
        if self.at == self.len {
            self.at = 0;
        }
    }
}

#[derive(Debug)]
struct Comb {
    line: Line,
    store: f32,
}

impl Comb {
    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let out = self.line.read();
        self.store = out * (1.0 - DAMP) + self.store * DAMP;
        self.line.write_advance(input + self.store * FEEDBACK);
        out
    }
}

#[derive(Debug)]
struct Allpass {
    line: Line,
}

impl Allpass {
    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let delayed = self.line.read();
        self.line.write_advance(input + delayed * ALLPASS_FEEDBACK);
        delayed - input
    }
}

#[derive(Debug)]
struct Side {
    predelay: Line,
    combs: Vec<Comb>,
    allpasses: Vec<Allpass>,
}

impl Side {
    fn new(scale_max: f64, spread: usize) -> Self {
        let size = |n: usize| ((n + spread) as f64 * scale_max).ceil() as usize + 1;
        Self {
            predelay: Line::new((PREDELAY_SECS * f64::from(MAX_RATE)).ceil() as usize + 1),
            combs: COMBS.iter().map(|&n| Comb { line: Line::new(size(n)), store: 0.0 }).collect(),
            allpasses: ALLPASSES.iter().map(|&n| Allpass { line: Line::new(size(n)) }).collect(),
        }
    }

    fn tune(&mut self, rate: u32, spread: usize) {
        let scale = f64::from(rate) / REF_RATE;
        let len = |n: usize| ((n + spread) as f64 * scale).round() as usize;
        self.predelay.set_len((PREDELAY_SECS * f64::from(rate)).round() as usize);
        for (c, &n) in self.combs.iter_mut().zip(&COMBS) {
            c.line.set_len(len(n));
            c.store = 0.0;
        }
        for (a, &n) in self.allpasses.iter_mut().zip(&ALLPASSES) {
            a.line.set_len(len(n));
        }
    }

    fn clear(&mut self) {
        self.predelay.clear();
        for c in &mut self.combs {
            c.line.clear();
            c.store = 0.0;
        }
        for a in &mut self.allpasses {
            a.line.clear();
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let x = self.predelay.read();
        self.predelay.write_advance(input);
        let x = x * INPUT_GAIN;
        let mut acc = 0.0;
        for c in &mut self.combs {
            acc += c.process(x);
        }
        for a in &mut self.allpasses {
            acc = a.process(acc);
        }
        acc * WET_GAIN
    }
}

/// The shared room. Create it off the audio thread; [`Room::tune`],
/// [`Room::clear`] and [`Room::process`] never allocate.
#[derive(Debug)]
pub struct Room {
    left: Side,
    right: Side,
    rate: u32,
    /// Nothing has been sent in since the last clear (skip the work).
    silent: bool,
}

impl Default for Room {
    fn default() -> Self {
        Self::new()
    }
}

impl Room {
    #[must_use]
    pub fn new() -> Self {
        let scale_max = f64::from(MAX_RATE) / REF_RATE;
        let mut room =
            Self { left: Side::new(scale_max, 0), right: Side::new(scale_max, STEREO_SPREAD), rate: 0, silent: true };
        room.tune(48_000);
        room
    }

    /// Sets the sample rate (clamped to the supported range) and clears.
    pub fn tune(&mut self, rate: u32) {
        let rate = rate.clamp(8_000, MAX_RATE);
        self.rate = rate;
        self.left.tune(rate, 0);
        self.right.tune(rate, STEREO_SPREAD);
        self.silent = true;
    }

    #[must_use]
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Drops the reverb tail (after a seek or a new song).
    pub fn clear(&mut self) {
        if !self.silent {
            self.left.clear();
            self.right.clear();
            self.silent = true;
        }
    }

    /// Replaces the send signal in `l`/`r` with the room's wet output.
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let n = l.len().min(r.len());
        if self.silent && l[..n].iter().chain(&r[..n]).all(|s| *s == 0.0) {
            return;
        }
        self.silent = false;
        for i in 0..n {
            // Mono into the room, decorrelated stereo out: the takes sit in
            // one space instead of each keeping its own image.
            let x = 0.5 * (l[i] + r[i]);
            l[i] = self.left.process(x);
            r[i] = self.right.process(x);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn energy(x: &[f32]) -> f32 {
        x.iter().map(|s| s * s).sum()
    }

    #[test]
    fn an_impulse_rings_out_like_a_small_room() {
        let rate = 48_000;
        let mut room = Room::new();
        room.tune(rate);
        let n = rate as usize * 2;
        let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
        l[0] = 1.0;
        r[0] = 1.0;
        room.process(&mut l, &mut r);
        // Nothing before the pre-delay.
        let pre = (PREDELAY_SECS * f64::from(rate)) as usize;
        assert!(l[..pre].iter().all(|s| *s == 0.0));
        // Decays: the second half-second holds far less energy than the first.
        let s = rate as usize / 2;
        let (first, later) = (energy(&l[..s]), energy(&l[s * 2..s * 3]));
        assert!(first > 0.0 && later < first * 0.01, "first {first} later {later}");
        // Stereo: the sides differ.
        assert!(l.iter().zip(&r).any(|(a, b)| (a - b).abs() > 1e-4));
        assert!(l.iter().chain(&r).all(|s| s.is_finite() && s.abs() < 1.0));
    }

    #[test]
    fn steady_input_stays_bounded_and_clear_silences() {
        let mut room = Room::new();
        room.tune(44_100);
        let mut l = vec![0.9f32; 44_100];
        let mut r = vec![-0.9f32; 44_100];
        room.process(&mut l, &mut r);
        let mut l = vec![0.8f32; 44_100];
        let mut r = l.clone();
        room.process(&mut l, &mut r);
        assert!(l.iter().all(|s| s.is_finite() && s.abs() < 2.0));
        room.clear();
        let (mut l, mut r) = (vec![0.0f32; 4_096], vec![0.0f32; 4_096]);
        room.process(&mut l, &mut r);
        assert!(l.iter().chain(&r).all(|s| *s == 0.0));
    }

    #[test]
    fn retuning_to_any_rate_stays_inside_the_buffers() {
        let mut room = Room::new();
        for rate in [8_000, 22_050, 44_100, 48_000, 96_000, 192_000, 384_000, 0] {
            room.tune(rate);
            let (mut l, mut r) = (vec![0.5f32; 1_000], vec![0.5f32; 1_000]);
            room.process(&mut l, &mut r);
            assert!(l.iter().all(|s| s.is_finite()));
        }
    }
}
