//! Long-term spectrum of each take against the stem it replaces, in
//! third-octave bands, over the stretches where the take plays, before and
//! after the tone-matching equaliser:
//! `cargo run --release -p joshua-core --example tone_compare -- <song dir>...`
#![allow(clippy::unwrap_used, clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::path::Path;

use joshua_core::decode::decode_file;
use joshua_core::project::SongProject;
use joshua_core::security::Limits;
use joshua_core::separation::StemKind;
use joshua_core::stft::{Stft, hann, windowed_frame};
use rustfft::num_complex::Complex32;

const N: usize = 8192;

fn frames(mono: &[f32], offset: usize, total: usize) -> Vec<Vec<f32>> {
    let window = hann(N);
    let mut stft = Stft::new(N);
    let mut f = vec![0.0; N];
    let mut spec = vec![Complex32::default(); stft.bins()];
    (0..total)
        .map(|k| {
            let start = (k * N / 2) as isize - offset as isize;
            windowed_frame(mono, start, &window, &mut f);
            stft.forward(&f, &mut spec);
            spec.iter().map(|c| c.norm_sqr()).collect()
        })
        .collect()
}

fn main() {
    let limits = Limits::default();
    let centres: Vec<f64> = (0..28).map(|i| 31.5 * 2f64.powf(i as f64 / 3.0)).collect();
    for dir in std::env::args().skip(1) {
        let dir = Path::new(&dir);
        let p: SongProject = serde_json::from_str(&std::fs::read_to_string(dir.join("project.json")).unwrap()).unwrap();
        let Some(drums) = p.tracks.iter().find(|t| t.stem == StemKind::Drums) else { continue };
        let stem = decode_file(&dir.join(&drums.file), &limits).unwrap();
        let sr = f64::from(stem.sample_rate());
        let total = stem.frames() * 2 / N + 1;
        let s = frames(&stem.to_mono(), 0, total);
        for take in &p.takes {
            let t = joshua_core::resample::resample(
                decode_file(&dir.join(&take.file), &limits).unwrap(),
                stem.sample_rate(),
            )
            .unwrap();
            // Tone match, as the app does it, then compare both versions.
            let curve = joshua_core::tone::match_take(
                &t,
                take.offset_secs,
                p.duration_secs,
                &joshua_core::tone::ToneProfile::analyse(&stem, 0.0, p.duration_secs),
            );
            let mut eq = joshua_core::tone::ToneEq::default();
            eq.set(t.sample_rate(), &curve);
            let (l, r): (Vec<f32>, Vec<f32>) = t.left().iter().zip(t.right()).map(|(l, r)| eq.process(*l, *r)).unzip();
            let matched = joshua_core::StereoBuffer::new(t.sample_rate(), l, r).unwrap();
            println!("== {} / {}: curve {:?}", p.title, take.name, curve);
            let mut rows = Vec::new();
            for (label, buf) in [("as recorded", &t), ("tone-matched", &matched)] {
                let tf = frames(&buf.to_mono(), (take.offset_secs * sr) as usize, total);
                let energy: Vec<f32> = tf.iter().map(|f| f.iter().sum()).collect();
                let peak = energy.iter().copied().fold(0.0, f32::max);
                let active: Vec<usize> = (0..total).filter(|k| energy[*k] > peak * 1e-3).collect();
                let band = |spec: &[Vec<f32>], c: f64| -> f64 {
                    let (lo, hi) = (c / 2f64.powf(1.0 / 6.0), c * 2f64.powf(1.0 / 6.0));
                    let b0 = (lo * N as f64 / sr) as usize;
                    let b1 = ((hi * N as f64 / sr) as usize).max(b0 + 1).min(N / 2);
                    let e: f64 =
                        active.iter().map(|k| spec[*k][b0..b1].iter().map(|x| f64::from(*x)).sum::<f64>()).sum();
                    10.0 * (e / (b1 - b0) as f64 + 1e-20).log10()
                };
                let diffs: Vec<f64> =
                    centres.iter().filter(|c| **c < 14_000.0).map(|c| band(&tf, *c) - band(&s, *c)).collect();
                let mean = diffs.iter().sum::<f64>() / diffs.len() as f64;
                let dev: Vec<f64> = diffs.iter().map(|d| d - mean).collect();
                let rms = (dev.iter().map(|d| d * d).sum::<f64>() / dev.len() as f64).sqrt();
                rows.push((label, dev, rms));
            }
            println!("  {:>8}  {:>12} {:>12}", "band", rows[0].0, rows[1].0);
            for (i, c) in centres.iter().filter(|c| **c < 14_000.0).enumerate() {
                println!("  {:>6.0} Hz  {:+12.1} {:+12.1}", c, rows[0].1[i], rows[1].1[i]);
            }
            println!("  tone difference (RMS over bands): {:.1} dB -> {:.1} dB", rows[0].2, rows[1].2);
        }
    }
}
