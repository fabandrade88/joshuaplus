//! Tone matching: an octave-band equaliser on a recorded take, set so the
//! take sounds like the instrument it replaces in the original record (an
//! e-drum module through a mixer is darker and boxier than a recorded kit).
//!
//! [`ToneProfile`] keeps the long-term spectrum of a track in ten octave
//! bands, frame by frame on the song timeline; [`match_curve`] compares a
//! take with a stem over the stretches the take plays and returns the gains
//! that bring the take's tone to the stem's (its level is left alone); and
//! [`ToneEq`] applies such gains in real time without allocating.

use rustfft::num_complex::Complex32;

use crate::audio::StereoBuffer;
use crate::stft::{Stft, hann, windowed_frame};

/// Number of equaliser bands.
pub const TONE_BANDS: usize = 10;
/// Centre frequencies of the bands (octaves from 31 Hz to 16 kHz).
pub const TONE_CENTRES: [f32; TONE_BANDS] = [31.25, 62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];
/// Largest correction per band, either way.
pub const MAX_TONE_DB: f32 = 9.0;

/// Profile frame period (seconds).
const HOP_SECS: f64 = 0.1;
const FFT: usize = 8192;
/// Frames this far below the take's loudest one are not "playing".
const ACTIVE_BELOW_PEAK_DB: f32 = 30.0;
/// One octave per filter.
const Q: f32 = std::f32::consts::SQRT_2;

/// Octave-band energies of a track, frame by frame on the song timeline.
#[derive(Debug, Clone)]
pub struct ToneProfile {
    frames: Vec<[f32; TONE_BANDS]>,
}

impl ToneProfile {
    /// Profiles `buffer` placed `offset_secs` into a song of `duration_secs`.
    #[must_use]
    pub fn analyse(buffer: &StereoBuffer, offset_secs: f64, duration_secs: f64) -> Self {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let count = (duration_secs.max(0.0) / HOP_SECS).ceil() as usize;
        let rate = f64::from(buffer.sample_rate());
        let mono = buffer.to_mono();
        let window = hann(FFT);
        let mut stft = Stft::new(FFT);
        let mut frame = vec![0.0f32; FFT];
        let mut spectrum = vec![Complex32::default(); stft.bins()];
        let bin_hz = rate / FFT as f64;
        let band_of: Vec<Option<usize>> = (0..stft.bins())
            .map(|b| {
                let hz = (b as f64 * bin_hz) as f32;
                TONE_CENTRES
                    .iter()
                    .position(|c| hz >= c / std::f32::consts::SQRT_2 && hz < c * std::f32::consts::SQRT_2)
            })
            .collect();
        let mut frames = vec![[0.0f32; TONE_BANDS]; count];
        for (k, out) in frames.iter_mut().enumerate() {
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
                    out[*b] += s.norm_sqr();
                }
            }
        }
        Self { frames }
    }
}

fn db(p: f64) -> f64 {
    10.0 * p.max(1e-20).log10()
}

/// Gains (dB per band) that give `take` the tone of `stem`, measured where
/// the take plays. Level-neutral (the gains average zero over the bands that
/// carry sound), smoothed across neighbouring bands and limited to
/// ±[`MAX_TONE_DB`]; bands where either track is silent get no gain.
#[must_use]
pub fn match_curve(take: &ToneProfile, stem: &ToneProfile) -> [f32; TONE_BANDS] {
    let total = |f: &[f32; TONE_BANDS]| f.iter().sum::<f32>();
    let peak = take.frames.iter().map(total).fold(0.0f32, f32::max);
    let mut curve = [0.0f32; TONE_BANDS];
    if peak <= 0.0 {
        return curve;
    }
    let floor = peak * 10f32.powf(-ACTIVE_BELOW_PEAK_DB / 10.0);
    let (mut t, mut s) = ([0.0f64; TONE_BANDS], [0.0f64; TONE_BANDS]);
    for (tf, sf) in take.frames.iter().zip(&stem.frames) {
        if total(tf) < floor {
            continue;
        }
        for b in 0..TONE_BANDS {
            t[b] += f64::from(tf[b]);
            s[b] += f64::from(sf[b]);
        }
    }
    // Bands with real content in both (within 60 dB of each track's loudest).
    let tmax = t.iter().copied().fold(0.0f64, f64::max);
    let smax = s.iter().copied().fold(0.0f64, f64::max);
    if tmax <= 0.0 || smax <= 0.0 {
        return curve;
    }
    let live: Vec<bool> = (0..TONE_BANDS).map(|b| t[b] > tmax * 1e-6 && s[b] > smax * 1e-6).collect();
    let raw: Vec<f64> = (0..TONE_BANDS).map(|b| if live[b] { db(s[b]) - db(t[b]) } else { 0.0 }).collect();
    let used = live.iter().filter(|l| **l).count();
    if used == 0 {
        return curve;
    }
    let mean = raw.iter().zip(&live).filter(|(_, l)| **l).map(|(r, _)| r).sum::<f64>() / used as f64;
    for b in 0..TONE_BANDS {
        if !live[b] {
            continue;
        }
        // 1-2-1 smoothing over live neighbours: a broad tone, not a notch.
        let (mut sum, mut w) = (2.0 * (raw[b] - mean), 2.0);
        for n in [b.wrapping_sub(1), b + 1] {
            if n < TONE_BANDS && live[n] {
                sum += raw[n] - mean;
                w += 1.0;
            }
        }
        #[allow(clippy::cast_possible_truncation)]
        let g = (sum / w) as f32;
        curve[b] = (g.clamp(-MAX_TONE_DB, MAX_TONE_DB) * 2.0).round() / 2.0;
    }
    curve
}

/// Passes of [`match_take`]: each one corrects what the filters' overlap
/// left of the previous curve.
const MATCH_PASSES: usize = 3;

/// The equaliser curve that gives `take` (placed `offset_secs` into a song of
/// `duration_secs`) the tone of the stem profiled in `stem`. Neighbouring
/// octave filters overlap, so the curve is refined on the equalised take a
/// few times, each pass adding what is left.
#[must_use]
pub fn match_take(take: &StereoBuffer, offset_secs: f64, duration_secs: f64, stem: &ToneProfile) -> [f32; TONE_BANDS] {
    let mut curve = match_curve(&ToneProfile::analyse(take, offset_secs, duration_secs), stem);
    for _ in 1..MATCH_PASSES {
        let mut eq = ToneEq::default();
        eq.set(take.sample_rate(), &curve);
        if !eq.is_active() {
            break;
        }
        let (l, r): (Vec<f32>, Vec<f32>) =
            take.left().iter().zip(take.right()).map(|(l, r)| eq.process(*l, *r)).unzip();
        let Ok(heard) = StereoBuffer::new(take.sample_rate(), l, r) else { break };
        let rest = match_curve(&ToneProfile::analyse(&heard, offset_secs, duration_secs), stem);
        for (c, r) in curve.iter_mut().zip(rest) {
            *c = ((*c + r).clamp(-MAX_TONE_DB, MAX_TONE_DB) * 2.0).round() / 2.0;
        }
    }
    curve
}

/// One peaking filter (RBJ cookbook), transposed direct form II.
#[derive(Debug, Clone, Copy, Default)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: [f32; 2],
    z2: [f32; 2],
}

impl Biquad {
    fn peaking(rate: f32, freq: f32, gain_db: f32) -> Self {
        let a = 10f32.powf(gain_db / 40.0);
        let w0 = std::f32::consts::TAU * freq / rate;
        let alpha = w0.sin() / (2.0 * Q);
        let cos = w0.cos();
        let a0 = 1.0 + alpha / a;
        Self {
            b0: (1.0 + alpha * a) / a0,
            b1: -2.0 * cos / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha / a) / a0,
            z1: [0.0; 2],
            z2: [0.0; 2],
        }
    }

    #[inline]
    fn run(&mut self, ch: usize, x: f32) -> f32 {
        let y = self.b0 * x + self.z1[ch];
        self.z1[ch] = self.b1 * x - self.a1 * y + self.z2[ch];
        self.z2[ch] = self.b2 * x - self.a2 * y;
        y
    }
}

/// A ten-band equaliser on a stereo channel. Changing the gains keeps the
/// filter state (no click); with all gains at zero it is bypassed.
#[derive(Debug, Clone, Copy, Default)]
pub struct ToneEq {
    bands: [Biquad; TONE_BANDS],
    active: [bool; TONE_BANDS],
    any: bool,
}

impl ToneEq {
    /// Sets the gains (dB per band) for `rate` Hz. Never allocates.
    pub fn set(&mut self, rate: u32, gains_db: &[f32; TONE_BANDS]) {
        #[allow(clippy::cast_precision_loss)]
        let rate = rate as f32;
        self.any = false;
        for ((band, active), (centre, gain)) in
            self.bands.iter_mut().zip(self.active.iter_mut()).zip(TONE_CENTRES.iter().zip(gains_db))
        {
            let gain = if gain.is_finite() { gain.clamp(-MAX_TONE_DB, MAX_TONE_DB) } else { 0.0 };
            *active = gain.abs() >= 0.05 && *centre < 0.45 * rate;
            if *active {
                let state = (band.z1, band.z2);
                *band = Biquad::peaking(rate, *centre, gain);
                (band.z1, band.z2) = state;
                self.any = true;
            }
        }
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.any
    }

    /// Clears the filter memory (after a seek).
    pub fn reset(&mut self) {
        for b in &mut self.bands {
            b.z1 = [0.0; 2];
            b.z2 = [0.0; 2];
        }
    }

    /// Filters one stereo frame.
    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        if !self.any {
            return (l, r);
        }
        let (mut l, mut r) = (l, r);
        for (band, active) in self.bands.iter_mut().zip(&self.active) {
            if *active {
                l = band.run(0, l);
                r = band.run(1, r);
            }
        }
        (l, r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn sine_gain(eq: &mut ToneEq, freq: f32) -> f32 {
        let n = RATE as usize;
        let mut peak_in = 0.0f32;
        let mut peak_out = 0.0f32;
        for i in 0..n {
            let x = (std::f32::consts::TAU * freq * i as f32 / RATE as f32).sin();
            let (y, _) = eq.process(x, x);
            if i > n / 2 {
                peak_in = peak_in.max(x.abs());
                peak_out = peak_out.max(y.abs());
            }
        }
        20.0 * (peak_out / peak_in).log10()
    }

    #[test]
    fn each_band_boosts_or_cuts_its_octave() {
        let mut gains = [0.0; TONE_BANDS];
        gains[5] = 6.0; // 1 kHz
        gains[8] = -6.0; // 8 kHz
        let mut eq = ToneEq::default();
        eq.set(RATE, &gains);
        assert!((sine_gain(&mut eq, 1000.0) - 6.0).abs() < 0.6);
        assert!((sine_gain(&mut eq, 8000.0) + 6.0).abs() < 0.6);
        assert!(sine_gain(&mut eq, 125.0).abs() < 0.6, "far bands untouched");
    }

    #[test]
    fn flat_gains_pass_the_signal_bit_for_bit() {
        let mut eq = ToneEq::default();
        eq.set(RATE, &[0.0; TONE_BANDS]);
        assert!(!eq.is_active());
        assert_eq!(eq.process(0.25, -0.5), (0.25, -0.5));
        // Out-of-range or broken gains are clamped, never NaN.
        eq.set(RATE, &[f32::NAN, 99.0, -99.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        let (l, r) = eq.process(0.1, 0.1);
        assert!(l.is_finite() && r.is_finite());
    }

    /// Noise through a one-pole low-pass at `cutoff` Hz (a "darker" copy).
    fn noise(seconds: f64, cutoff: Option<f32>, seed: u32) -> StereoBuffer {
        let n = (seconds * f64::from(RATE)) as usize;
        let mut s = seed;
        let a = cutoff.map(|c| (-std::f32::consts::TAU * c / RATE as f32).exp());
        let mut y = 0.0f32;
        let x: Vec<f32> = (0..n)
            .map(|_| {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let v = (s >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
                match a {
                    Some(a) => {
                        y = (1.0 - a) * v + a * y;
                        y
                    }
                    None => 0.3 * v,
                }
            })
            .collect();
        StereoBuffer::from_mono(RATE, x).unwrap()
    }

    #[test]
    fn a_darker_take_gets_its_highs_back() {
        let stem = ToneProfile::analyse(&noise(10.0, None, 1), 0.0, 10.0);
        let take = ToneProfile::analyse(&noise(10.0, Some(1500.0), 2), 0.0, 10.0);
        let curve = match_curve(&take, &stem);
        // The low-pass costs ~15 dB at 8 kHz over 125 Hz: the curve lifts the
        // highs relative to the lows (clamped at the limit).
        assert!(curve[8] - curve[2] > 8.0, "{curve:?}");
        assert!(curve.iter().all(|g| g.abs() <= MAX_TONE_DB));
        // The same tone needs nothing.
        let twin = ToneProfile::analyse(&noise(10.0, None, 3), 0.0, 10.0);
        assert!(match_curve(&twin, &stem).iter().all(|g| g.abs() <= 1.0), "{:?}", match_curve(&twin, &stem));
    }

    #[test]
    fn silence_gets_no_curve() {
        let stem = ToneProfile::analyse(&noise(4.0, None, 1), 0.0, 4.0);
        let silent = ToneProfile::analyse(&StereoBuffer::silent(RATE, RATE as usize * 4).unwrap(), 0.0, 4.0);
        assert_eq!(match_curve(&silent, &stem), [0.0; TONE_BANDS]);
    }
}
