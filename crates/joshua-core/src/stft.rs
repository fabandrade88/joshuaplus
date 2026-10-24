//! Short-time Fourier transform primitives with streaming overlap-add.

use std::sync::Arc;

use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};

/// Periodic Hann window (the variant that satisfies COLA for hop = n/4).
#[must_use]
pub fn hann(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = std::f32::consts::TAU * i as f32 / n as f32;
            0.5 - 0.5 * x.cos()
        })
        .collect()
}

/// Copies `n` samples starting at `start` (may be negative or past the end;
/// missing samples are zero) into `out`, applying `window`.
pub fn windowed_frame(signal: &[f32], start: isize, window: &[f32], out: &mut [f32]) {
    for (i, (o, w)) in out.iter_mut().zip(window).enumerate() {
        let idx = start + i as isize;
        *o = if idx >= 0 { signal.get(idx as usize).copied().unwrap_or(0.0) * w } else { 0.0 };
    }
}

/// Forward/inverse FFT of a fixed size with pre-allocated scratch space.
pub struct Stft {
    n: usize,
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
    buf: Vec<Complex32>,
    scratch: Vec<Complex32>,
}

impl std::fmt::Debug for Stft {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stft").field("n", &self.n).finish_non_exhaustive()
    }
}

impl Stft {
    #[must_use]
    pub fn new(n: usize) -> Self {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(n);
        let ifft = planner.plan_fft_inverse(n);
        let scratch_len = fft.get_inplace_scratch_len().max(ifft.get_inplace_scratch_len());
        Self { n, fft, ifft, buf: vec![Complex32::default(); n], scratch: vec![Complex32::default(); scratch_len] }
    }

    #[must_use]
    pub const fn bins(&self) -> usize {
        self.n / 2 + 1
    }

    /// Real frame (`n` samples) → half spectrum (`n/2 + 1` bins).
    pub fn forward(&mut self, frame: &[f32], spectrum: &mut [Complex32]) {
        for (b, s) in self.buf.iter_mut().zip(frame) {
            *b = Complex32::new(*s, 0.0);
        }
        self.fft.process_with_scratch(&mut self.buf, &mut self.scratch);
        spectrum.copy_from_slice(&self.buf[..self.bins()]);
    }

    /// Half spectrum → real frame (`n` samples), scaled by `1/n`.
    pub fn inverse(&mut self, spectrum: &[Complex32], frame: &mut [f32]) {
        let n = self.n;
        self.buf[..spectrum.len()].copy_from_slice(spectrum);
        for (k, s) in spectrum.iter().enumerate().take(n / 2).skip(1) {
            self.buf[n - k] = s.conj();
        }
        self.ifft.process_with_scratch(&mut self.buf, &mut self.scratch);
        let scale = 1.0 / n as f32;
        for (f, b) in frame.iter_mut().zip(&self.buf) {
            *f = b.re * scale;
        }
    }
}

/// Streaming weighted overlap-add. Each added frame is multiplied by the
/// synthesis window; output is normalised by the accumulated `window²`,
/// which yields perfect reconstruction for an unmodified spectrum.
#[derive(Debug, Clone)]
pub struct OverlapAdd {
    hop: usize,
    acc: Vec<f32>,
    norm: Vec<f32>,
}

impl OverlapAdd {
    #[must_use]
    pub fn new(n: usize, hop: usize) -> Self {
        Self { hop, acc: vec![0.0; n], norm: vec![0.0; n] }
    }

    /// Adds a raw (un-windowed) inverse-FFT frame.
    pub fn add(&mut self, frame: &[f32], window: &[f32]) {
        for ((a, nrm), (s, w)) in self.acc.iter_mut().zip(self.norm.iter_mut()).zip(frame.iter().zip(window)) {
            *a += s * w;
            *nrm += w * w;
        }
    }

    /// Emits the next `hop` finished samples into `out` and advances.
    pub fn pop_hop(&mut self, out: &mut [f32]) {
        let hop = self.hop;
        for ((o, a), nrm) in out.iter_mut().zip(&self.acc[..hop]).zip(&self.norm[..hop]) {
            *o = if *nrm > 1e-6 { a / nrm } else { 0.0 };
        }
        self.acc.copy_within(hop.., 0);
        self.norm.copy_within(hop.., 0);
        let len = self.acc.len();
        self.acc[len - hop..].fill(0.0);
        self.norm[len - hop..].fill(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_inverse_overlap_add_is_identity() {
        let (n, hop) = (512, 128);
        let signal: Vec<f32> = (0..5000).map(|i| ((i * 7919) % 1000) as f32 / 1000.0 - 0.5).collect();
        let window = hann(n);
        let mut stft = Stft::new(n);
        let mut ola = OverlapAdd::new(n, hop);
        let mut frame = vec![0.0; n];
        let mut spec = vec![Complex32::default(); stft.bins()];
        let mut chunk = vec![0.0; hop];
        let mut out = Vec::new();
        let first = -((n - hop) as isize);
        let mut start = first;
        while start < signal.len() as isize {
            windowed_frame(&signal, start, &window, &mut frame);
            stft.forward(&frame, &mut spec);
            stft.inverse(&spec, &mut frame);
            ola.add(&frame, &window);
            ola.pop_hop(&mut chunk);
            out.extend_from_slice(&chunk);
            start += hop as isize;
        }
        let out = &out[(n - hop)..];
        let err = signal.iter().zip(out).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
        assert!(err < 1e-4, "reconstruction error {err}");
    }
}
