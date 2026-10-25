//! Kick and tom "body" of each take against the original drums: low-band
//! level at the hits, how long each hit sustains (time to fall 20 dB) and
//! how peaky the low end is (crest factor), where the take plays.
//! `cargo run --release -p joshua-core --example body_compare -- <song dir>...`
#![allow(clippy::unwrap_used, clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::path::Path;

use joshua_core::decode::decode_file;
use joshua_core::project::SongProject;
use joshua_core::resample::resample;
use joshua_core::security::Limits;
use joshua_core::separation::StemKind;
use joshua_core::stft::{Stft, hann, windowed_frame};
use rustfft::num_complex::Complex32;

const N: usize = 2048;
const HOP: usize = 441; // 10 ms at 44.1 kHz

/// Energy envelope (10 ms frames) of a band, on the song timeline.
fn band_env(mono: &[f32], sr: f64, lo: f64, hi: f64, offset: usize, frames: usize) -> Vec<f64> {
    let window = hann(N);
    let mut stft = Stft::new(N);
    let mut f = vec![0.0; N];
    let mut spec = vec![Complex32::default(); stft.bins()];
    let (b0, b1) = ((lo * N as f64 / sr) as usize, (hi * N as f64 / sr) as usize);
    (0..frames)
        .map(|k| {
            windowed_frame(mono, (k * HOP) as isize - offset as isize - (N / 2) as isize, &window, &mut f);
            stft.forward(&f, &mut spec);
            spec[b0..b1].iter().map(|c| f64::from(c.norm_sqr())).sum::<f64>()
        })
        .collect()
}

fn db(x: f64) -> f64 {
    10.0 * x.max(1e-20).log10()
}

/// Hits (local maxima well above the floor) and stats over them.
fn stats(env: &[f64], active: &[bool]) -> (f64, f64, f64) {
    let mut levels: Vec<f64> = Vec::new();
    let mut decays: Vec<f64> = Vec::new();
    let mut sorted: Vec<f64> = env.iter().zip(active).filter(|(_, a)| **a).map(|(e, _)| *e).collect();
    sorted.sort_by(f64::total_cmp);
    if sorted.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let floor = sorted[sorted.len() * 6 / 10];
    for k in 3..env.len().saturating_sub(40) {
        if !active[k] || env[k] < floor * 8.0 || env[k] < env[k - 1] || env[k] < env[k + 1] || env[k - 3] * 4.0 > env[k]
        {
            continue;
        }
        levels.push(db(env[k]));
        // Frames until 20 dB down (or the next hit).
        let target = env[k] / 100.0;
        let d = (k + 1..k + 40).find(|j| env[*j] < target || env[*j] > env[k]).unwrap_or(k + 40) - k;
        decays.push(d as f64 * 10.0);
    }
    levels.sort_by(f64::total_cmp);
    decays.sort_by(f64::total_cmp);
    let mean: f64 = env.iter().zip(active).filter(|(_, a)| **a).map(|(e, _)| *e).sum::<f64>()
        / active.iter().filter(|a| **a).count().max(1) as f64;
    let peak = levels.get(levels.len() * 9 / 10).copied().unwrap_or(0.0);
    (
        levels.get(levels.len() / 2).copied().unwrap_or(0.0),
        decays.get(decays.len() / 2).copied().unwrap_or(0.0),
        peak - db(mean),
    )
}

fn main() {
    let limits = Limits::default();
    for dir in std::env::args().skip(1) {
        let dir = Path::new(&dir);
        let p: SongProject = serde_json::from_str(&std::fs::read_to_string(dir.join("project.json")).unwrap()).unwrap();
        let Some(drums) = p.tracks.iter().find(|t| t.stem == StemKind::Drums) else { continue };
        let stem = decode_file(&dir.join(&drums.file), &limits).unwrap();
        let sr = f64::from(stem.sample_rate());
        let frames = stem.frames() / HOP;
        for take in &p.takes {
            let t = resample(decode_file(&dir.join(&take.file), &limits).unwrap(), stem.sample_rate()).unwrap();
            let off = (take.offset_secs * sr) as usize;
            let full = band_env(&t.to_mono(), sr, 30.0, 16_000.0, off, frames);
            let peak = full.iter().copied().fold(0.0, f64::max);
            let active: Vec<bool> = full.iter().map(|e| *e > peak * 1e-3).collect();
            println!("== {} / {}", p.title, take.name);
            for (name, lo, hi) in
                [("kick 40-100 Hz", 40.0, 100.0), ("toms 80-250 Hz", 80.0, 250.0), ("snare 150-400", 150.0, 400.0)]
            {
                let te = band_env(&t.to_mono(), sr, lo, hi, off, frames);
                let se = band_env(&stem.to_mono(), sr, lo, hi, 0, frames);
                let tf = band_env(&t.to_mono(), sr, 30.0, 16_000.0, off, frames);
                let sf = band_env(&stem.to_mono(), sr, 30.0, 16_000.0, 0, frames);
                // Level of the band relative to the whole kit, so the take's overall gain does not matter.
                let rel = |b: &[f64], f: &[f64]| {
                    let (bs, fs): (f64, f64) = b
                        .iter()
                        .zip(f)
                        .zip(&active)
                        .filter(|(_, a)| **a)
                        .fold((0.0, 0.0), |(x, y), ((b, f), _)| (x + b, y + f));
                    db(bs) - db(fs)
                };
                let (_, td, tc) = stats(&te, &active);
                let (_, sd, sc) = stats(&se, &active);
                println!(
                    "  {name:15} share of kit: take {:+6.1} dB vs original {:+6.1} dB | sustain (−20 dB): {td:4.0} vs {sd:4.0} ms | crest: {tc:4.1} vs {sc:4.1} dB",
                    rel(&te, &tf),
                    rel(&se, &sf)
                );
            }
        }
    }
}
