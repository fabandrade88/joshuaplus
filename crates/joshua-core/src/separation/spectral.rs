//! Built-in DSP stem separator ("Quick" mode).
//!
//! Pipeline per STFT frame:
//! 1. **HPSS** (Fitzgerald 2010): median filtering across time keeps
//!    sustained (harmonic) energy, across frequency keeps transient
//!    (percussive) energy. Soft Wiener-like masks split the two.
//! 2. **Percussive → drums.**
//! 3. **Harmonic, low band → bass.**
//! 4. **Harmonic, vocal band, phantom-centre → vocals.** The centre weight
//!    uses the inter-channel coherence `2·Re(L·R*) / (|L|²+|R|²)`.
//! 5. **Everything else → other.**
//!
//! The four masks sum to exactly one, so the stems always add back up to the
//! original mix (verified by tests). The process is streaming: memory use is
//! independent of song length.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};

use rustfft::num_complex::Complex32;

use super::{SeparationJob, Separator, StemKind, StemOutput, StemSink};
use crate::audio::StereoBuffer;
use crate::error::{CoreError, Result};
use crate::stft::{OverlapAdd, Stft, hann, windowed_frame};
use crate::wav::StereoWavWriter;

/// Output order of [`separate_spectral`].
pub const SPECTRAL_STEMS: [StemKind; 4] = [StemKind::Vocals, StemKind::Drums, StemKind::Bass, StemKind::Other];

/// Tunables of the spectral separator.
#[derive(Debug, Clone, PartialEq)]
pub struct SpectralConfig {
    /// Target analysis window length in seconds (rounded to a power of two).
    pub window_secs: f32,
    /// Median length across time (frames, odd).
    pub harmonic_kernel: usize,
    /// Median length across frequency (bins, odd).
    pub percussive_kernel: usize,
    /// Mask exponent (2 = Wiener-like soft mask).
    pub mask_power: f32,
    /// Bass is fully below `bass_full_hz`, fades out until `bass_cut_hz`.
    pub bass_full_hz: f32,
    pub bass_cut_hz: f32,
    /// Vocal band edges.
    pub vocal_low_hz: f32,
    pub vocal_high_hz: f32,
    /// Exponent applied to the centre coherence (higher = stricter centre).
    pub centre_sharpness: i32,
}

impl Default for SpectralConfig {
    fn default() -> Self {
        Self {
            window_secs: 0.046,
            harmonic_kernel: 17,
            percussive_kernel: 31,
            mask_power: 2.0,
            bass_full_hz: 120.0,
            bass_cut_hz: 250.0,
            vocal_low_hz: 140.0,
            vocal_high_hz: 8_000.0,
            centre_sharpness: 4,
        }
    }
}

impl SpectralConfig {
    fn fft_size(&self, sample_rate: u32) -> usize {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let target = (self.window_secs * sample_rate as f32).max(256.0) as usize;
        target.next_power_of_two().clamp(256, 16_384)
    }
}

struct Frame {
    left: Vec<Complex32>,
    right: Vec<Complex32>,
    mag: Vec<f32>,
}

/// Per-bin static weights.
struct BandWeights {
    bass: Vec<f32>,
    vocal: Vec<f32>,
}

impl BandWeights {
    fn new(cfg: &SpectralConfig, bins: usize, n: usize, sample_rate: u32) -> Self {
        let hz = |k: usize| k as f32 * sample_rate as f32 / n as f32;
        let ramp = |x: f32, a: f32, b: f32| ((x - a) / (b - a)).clamp(0.0, 1.0);
        let bass = (0..bins).map(|k| 1.0 - ramp(hz(k), cfg.bass_full_hz, cfg.bass_cut_hz)).collect();
        let vocal = (0..bins)
            .map(|k| {
                let f = hz(k);
                ramp(f, cfg.vocal_low_hz * 0.7, cfg.vocal_low_hz)
                    * (1.0 - ramp(f, cfg.vocal_high_hz, cfg.vocal_high_hz * 1.6))
            })
            .collect();
        Self { bass, vocal }
    }
}

fn median(values: &mut [f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mid = values.len() / 2;
    *values.select_nth_unstable_by(mid, f32::total_cmp).1
}

/// Streams separated stems to `sink(stem_index, left, right)` in order.
/// `stem_index` indexes [`SPECTRAL_STEMS`].
///
/// # Errors
/// [`CoreError::Cancelled`] or any error returned by `sink`.
pub fn separate_spectral(
    audio: &StereoBuffer,
    cfg: &SpectralConfig,
    sink: &mut StemSink<'_>,
    progress: &dyn Fn(f32),
    cancel: &AtomicBool,
) -> Result<()> {
    let sr = audio.sample_rate();
    let n = cfg.fft_size(sr);
    let hop = n / 4;
    let len = audio.frames();
    let half_h = cfg.harmonic_kernel.max(1) / 2;
    let half_p = cfg.percussive_kernel.max(1) / 2;

    let window = hann(n);
    let mut stft = Stft::new(n);
    let bins = stft.bins();
    let weights = BandWeights::new(cfg, bins, n, sr);

    let lead = (n - hop) as isize;
    let total_frames = (len + n - hop).div_ceil(hop);

    let mut frames: VecDeque<Frame> = VecDeque::with_capacity(2 * half_h + 1);
    let mut first_index = 0usize;
    let mut time_buf = vec![0.0f32; n];
    let mut median_buf = Vec::with_capacity(cfg.harmonic_kernel.max(cfg.percussive_kernel));
    let mut harmonic = vec![0.0f32; bins];
    let mut masks = [vec![0.0f32; bins], vec![0.0f32; bins], vec![0.0f32; bins], vec![0.0f32; bins]];
    let mut stem_spec = vec![Complex32::default(); bins];
    let mut olas: Vec<[OverlapAdd; 2]> =
        (0..SPECTRAL_STEMS.len()).map(|_| [OverlapAdd::new(n, hop), OverlapAdd::new(n, hop)]).collect();
    let mut hop_l = vec![0.0f32; hop];
    let mut hop_r = vec![0.0f32; hop];

    for t in 0..total_frames + half_h {
        if t % 64 == 0 {
            if cancel.load(Ordering::Relaxed) {
                return Err(CoreError::Cancelled);
            }
            progress(t as f32 / (total_frames + half_h) as f32);
        }

        if t < total_frames {
            let start = t as isize * hop as isize - lead;
            let mut left = vec![Complex32::default(); bins];
            let mut right = vec![Complex32::default(); bins];
            windowed_frame(audio.left(), start, &window, &mut time_buf);
            stft.forward(&time_buf, &mut left);
            windowed_frame(audio.right(), start, &window, &mut time_buf);
            stft.forward(&time_buf, &mut right);
            let mag = left.iter().zip(&right).map(|(l, r)| 0.5 * (l.norm() + r.norm())).collect();
            frames.push_back(Frame { left, right, mag });
        }

        let Some(c) = t.checked_sub(half_h) else { continue };
        if c >= total_frames {
            break;
        }
        let centre = &frames[c - first_index];

        // 1. Harmonic estimate: median across time for each bin.
        for (k, h) in harmonic.iter_mut().enumerate() {
            median_buf.clear();
            median_buf.extend(frames.iter().map(|f| f.mag[k]));
            *h = median(&mut median_buf);
        }
        // 2. Percussive estimate (median across frequency) and masks.
        let p = cfg.mask_power;
        for k in 0..bins {
            let lo = k.saturating_sub(half_p);
            let hi = (k + half_p + 1).min(bins);
            median_buf.clear();
            median_buf.extend_from_slice(&centre.mag[lo..hi]);
            let perc = median(&mut median_buf);

            let (hp, pp) = (harmonic[k].powf(p), perc.powf(p));
            let h_mask = if hp + pp > 1e-20 { hp / (hp + pp) } else { 0.5 };
            let p_mask = 1.0 - h_mask;

            let (l, r) = (centre.left[k], centre.right[k]);
            let energy = l.norm_sqr() + r.norm_sqr();
            let coherence = if energy > 1e-20 { (2.0 * (l * r.conj()).re / energy).max(0.0) } else { 0.0 };
            let vocal = weights.vocal[k] * coherence.powi(cfg.centre_sharpness);
            let bass = weights.bass[k];

            masks[0][k] = h_mask * (1.0 - bass) * vocal; // vocals
            masks[1][k] = p_mask; // drums
            masks[2][k] = h_mask * bass; // bass
            masks[3][k] = h_mask * (1.0 - bass) * (1.0 - vocal); // other
        }

        // 3. Resynthesise every stem and emit the finished hop.
        let start = c as isize * hop as isize - lead;
        let emit_from = (-start).clamp(0, hop as isize) as usize;
        let emit_to = (len as isize - start).clamp(0, hop as isize) as usize;
        for (s, mask) in masks.iter().enumerate() {
            for (ch, src) in [&centre.left, &centre.right].into_iter().enumerate() {
                for ((dst, x), m) in stem_spec.iter_mut().zip(src.iter()).zip(mask) {
                    *dst = x * *m;
                }
                stft.inverse(&stem_spec, &mut time_buf);
                olas[s][ch].add(&time_buf, &window);
                olas[s][ch].pop_hop(if ch == 0 { &mut hop_l } else { &mut hop_r });
            }
            if emit_from < emit_to {
                sink(s, &hop_l[emit_from..emit_to], &hop_r[emit_from..emit_to])?;
            }
        }

        // 4. Drop frames that no future centre needs.
        while first_index + half_h < c + 1 && !frames.is_empty() {
            frames.pop_front();
            first_index += 1;
        }
    }
    progress(1.0);
    Ok(())
}

/// [`Separator`] adapter writing the spectral stems as WAV files.
#[derive(Debug, Default, Clone)]
pub struct SpectralSeparator {
    pub config: SpectralConfig,
}

impl Separator for SpectralSeparator {
    fn id(&self) -> String {
        "spectral".to_owned()
    }

    fn stems(&self) -> Vec<StemKind> {
        SPECTRAL_STEMS.to_vec()
    }

    fn separate(&self, job: &SeparationJob<'_>) -> Result<Vec<StemOutput>> {
        std::fs::create_dir_all(job.stems_dir)?;
        let rate = job.audio.sample_rate();
        let mut writers = SPECTRAL_STEMS
            .iter()
            .map(|k| StereoWavWriter::create(&job.stems_dir.join(format!("{}.wav", k.as_str())), rate))
            .collect::<Result<Vec<_>>>()?;
        separate_spectral(
            job.audio,
            &self.config,
            &mut |s, l, r| writers[s].write(l, r),
            &|p| (job.progress)(p),
            job.cancel,
        )?;
        for w in writers {
            w.finish()?;
        }
        Ok(SPECTRAL_STEMS.iter().map(|k| StemOutput { kind: *k, relative_path: k.file_name() }).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 22_050;

    fn run(audio: &StereoBuffer) -> Vec<(Vec<f32>, Vec<f32>)> {
        let mut stems = vec![(Vec::new(), Vec::new()); SPECTRAL_STEMS.len()];
        separate_spectral(
            audio,
            &SpectralConfig::default(),
            &mut |s, l, r| {
                stems[s].0.extend_from_slice(l);
                stems[s].1.extend_from_slice(r);
                Ok(())
            },
            &|_| {},
            &AtomicBool::new(false),
        )
        .unwrap();
        stems
    }

    fn energy(x: &[f32]) -> f32 {
        x.iter().map(|s| s * s).sum()
    }

    fn sine(freq: f32, secs: f32, amp: f32) -> Vec<f32> {
        (0..(secs * SR as f32) as usize)
            .map(|i| amp * (std::f32::consts::TAU * freq * i as f32 / SR as f32).sin())
            .collect()
    }

    /// Index of the stem holding most of the energy.
    fn dominant(stems: &[(Vec<f32>, Vec<f32>)]) -> StemKind {
        let (idx, _) =
            stems.iter().map(|(l, r)| energy(l) + energy(r)).enumerate().max_by(|a, b| a.1.total_cmp(&b.1)).unwrap();
        SPECTRAL_STEMS[idx]
    }

    #[test]
    fn stems_sum_back_to_the_original_mix() {
        let left: Vec<f32> = (0..SR as usize * 2).map(|i| ((i * 2654435761) % 2000) as f32 / 4000.0 - 0.25).collect();
        let right: Vec<f32> = sine(330.0, 2.0, 0.4);
        let audio = StereoBuffer::new(SR, left, right).unwrap();
        let stems = run(&audio);
        for (s, _) in &stems {
            assert_eq!(s.len(), audio.frames());
        }
        let max_err = (0..audio.frames())
            .map(|i| {
                let l: f32 = stems.iter().map(|s| s.0[i]).sum();
                let r: f32 = stems.iter().map(|s| s.1[i]).sum();
                (l - audio.left()[i]).abs().max((r - audio.right()[i]).abs())
            })
            .fold(0.0, f32::max);
        assert!(max_err < 1e-3, "reconstruction error {max_err}");
    }

    #[test]
    fn routes_signals_to_the_expected_stem() {
        let centre_tone = sine(440.0, 2.0, 0.5);
        let audio = StereoBuffer::from_mono(SR, centre_tone).unwrap();
        assert_eq!(dominant(&run(&audio)), StemKind::Vocals);

        let sub = sine(55.0, 2.0, 0.5);
        let audio = StereoBuffer::from_mono(SR, sub).unwrap();
        assert_eq!(dominant(&run(&audio)), StemKind::Bass);

        let panned = sine(1_000.0, 2.0, 0.5);
        let silence = vec![0.0; panned.len()];
        let audio = StereoBuffer::new(SR, panned, silence).unwrap();
        assert_eq!(dominant(&run(&audio)), StemKind::Other);

        let mut clicks = vec![0.0f32; SR as usize * 2];
        for beat in (0..clicks.len()).step_by(SR as usize / 4) {
            for (j, s) in clicks.iter_mut().skip(beat).take(40).enumerate() {
                *s = if j % 2 == 0 { 0.9 } else { -0.9 };
            }
        }
        let audio = StereoBuffer::from_mono(SR, clicks).unwrap();
        assert_eq!(dominant(&run(&audio)), StemKind::Drums);
    }

    #[test]
    fn handles_tiny_and_silent_inputs() {
        for frames in [0, 1, 100, 5_000] {
            let audio = StereoBuffer::silent(SR, frames).unwrap();
            let stems = run(&audio);
            assert!(stems.iter().all(|(l, r)| l.len() == frames && r.len() == frames));
            assert!(stems.iter().all(|(l, _)| l.iter().all(|s| *s == 0.0)));
        }
    }

    #[test]
    fn honours_cancellation() {
        let audio = StereoBuffer::silent(SR, SR as usize).unwrap();
        let result = separate_spectral(
            &audio,
            &SpectralConfig::default(),
            &mut |_, _, _| Ok(()),
            &|_| {},
            &AtomicBool::new(true),
        );
        assert!(matches!(result, Err(CoreError::Cancelled)));
    }

    #[test]
    fn writes_stem_files() {
        let dir = tempfile::tempdir().unwrap();
        let audio = StereoBuffer::from_mono(SR, sine(220.0, 0.5, 0.3)).unwrap();
        let stems_dir = dir.path().join("stems");
        let job = SeparationJob {
            source_path: dir.path(),
            audio: &audio,
            stems_dir: &stems_dir,
            scratch_dir: dir.path(),
            progress: &|_| {},
            cancel: &AtomicBool::new(false),
        };
        let out = SpectralSeparator::default().separate(&job).unwrap();
        assert_eq!(out.len(), 4);
        for o in out {
            assert!(dir.path().join(&o.relative_path).is_file());
        }
    }
}
