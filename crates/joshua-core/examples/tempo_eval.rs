//! Measures how well beat grids land on the drums, for songs already in a
//! library: `cargo run --release -p joshua-core --example tempo_eval -- <song dir>...`
//!
//! For every beat it finds the nearest drum hit (onsets of the drums stem at
//! ~3 ms resolution, picked independently of the tempo code) within ±80 ms,
//! then reports the median distance, the mean offset (bias), the drift of
//! the offset over the song (a slightly wrong BPM shows up as a slope) and
//! the share of beats within 20 ms. Both the stored grid and a fresh
//! analysis with the current code are measured.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::cast_precision_loss)]

use std::path::Path;

use joshua_core::decode::decode_file;
use joshua_core::project::SongProject;
use joshua_core::security::Limits;
use joshua_core::separation::StemKind;
use joshua_core::stft::{Stft, hann, windowed_frame};
use joshua_core::{StereoBuffer, tempo};
use rustfft::num_complex::Complex32;

fn drum_hits(drums: &StereoBuffer) -> Vec<f64> {
    let sr = f64::from(drums.sample_rate());
    let mono = drums.to_mono();
    let (n, hop) = (1024usize, 128usize);
    let window = hann(n);
    let mut stft = Stft::new(n);
    let mut frame = vec![0.0; n];
    let mut spec = vec![Complex32::default(); stft.bins()];
    let mut prev = vec![0.0f32; stft.bins()];
    let mut flux = Vec::new();
    let mut t = 0usize;
    while t < mono.len() {
        windowed_frame(&mono, t as isize - (n / 2) as isize, &window, &mut frame);
        stft.forward(&frame, &mut spec);
        let mut f = 0.0f32;
        for (s, p) in spec.iter().zip(prev.iter_mut()) {
            let m = (1.0 + 100.0 * s.norm()).ln();
            f += (m - *p).max(0.0);
            *p = m;
        }
        flux.push(f);
        t += hop;
    }
    let fps = sr / hop as f64;
    let w = (0.03 * fps) as usize;
    let mut sorted = flux.clone();
    sorted.sort_by(f32::total_cmp);
    let floor = sorted[sorted.len() * 7 / 10];
    (0..flux.len())
        .filter(|&i| {
            let lo = i.saturating_sub(w);
            let hi = (i + w + 1).min(flux.len());
            flux[i] > floor * 2.0 && flux[lo..hi].iter().all(|x| *x <= flux[i])
        })
        // Centred windows see a hit early; same correction as the analysis.
        .map(|i| i as f64 / fps + 0.27 * n as f64 / sr)
        .collect()
}

struct Score {
    median_ms: f64,
    bias_ms: f64,
    drift_ms_per_min: f64,
    within_20: f64,
    matched: f64,
}

fn score(beats: &[f64], hits: &[f64]) -> Score {
    let mut offs: Vec<(f64, f64)> = Vec::new();
    for &b in beats {
        let i = hits.partition_point(|h| *h < b);
        let near = [i.checked_sub(1), Some(i)]
            .into_iter()
            .flatten()
            .filter_map(|j| hits.get(j))
            .map(|h| h - b)
            .min_by(|x, y| x.abs().total_cmp(&y.abs()));
        if let Some(d) = near.filter(|d| d.abs() <= 0.08) {
            offs.push((b, d));
        }
    }
    let mut abs: Vec<f64> = offs.iter().map(|o| o.1.abs()).collect();
    abs.sort_by(f64::total_cmp);
    let n = offs.len().max(1) as f64;
    let (mt, md) = (offs.iter().map(|o| o.0).sum::<f64>() / n, offs.iter().map(|o| o.1).sum::<f64>() / n);
    let (mut num, mut den) = (0.0, 0.0);
    for (t, d) in &offs {
        num += (t - mt) * (d - md);
        den += (t - mt) * (t - mt);
    }
    Score {
        median_ms: abs.get(abs.len() / 2).copied().unwrap_or(f64::NAN) * 1000.0,
        bias_ms: md * 1000.0,
        drift_ms_per_min: if den > 0.0 { num / den * 1000.0 * 60.0 } else { 0.0 },
        within_20: abs.iter().filter(|a| **a <= 0.02).count() as f64 / beats.len().max(1) as f64 * 100.0,
        matched: offs.len() as f64 / beats.len().max(1) as f64 * 100.0,
    }
}

fn show(label: &str, a: &tempo::BeatAnalysis, hits: &[f64]) {
    let s = score(&a.beats, hits);
    println!(
        "   {label:6} {:7.3} bpm {:4} beats  median {:5.1} ms  bias {:+6.1} ms  drift {:+6.1} ms/min  ≤20 ms {:5.1}%  matched {:5.1}%",
        a.bpm,
        a.beats.len(),
        s.median_ms,
        s.bias_ms,
        s.drift_ms_per_min,
        s.within_20,
        s.matched
    );
}

fn main() {
    let limits = Limits::default();
    for dir in std::env::args().skip(1) {
        let dir = Path::new(&dir);
        let Ok(json) = std::fs::read_to_string(dir.join("project.json")) else { continue };
        let p: SongProject = serde_json::from_str(&json).unwrap();
        let file = |k: StemKind| p.tracks.iter().find(|t| t.stem == k).map(|t| dir.join(&t.file));
        let Some(drums) = file(StemKind::Drums).and_then(|f| decode_file(&f, &limits).ok()) else { continue };
        let hits = drum_hits(&drums);
        let mut mono = drums.to_mono();
        if let Some(bass) = file(StemKind::Bass).and_then(|f| decode_file(&f, &limits).ok()) {
            for (m, b) in mono.iter_mut().zip(bass.to_mono()) {
                *m += 0.5 * b;
            }
        }
        let rhythm = StereoBuffer::from_mono(drums.sample_rate(), mono).unwrap();
        let fresh = tempo::analyze(&rhythm);
        println!("== {} ({} drum hits)", p.title, hits.len());
        show("stored", &p.analysis, &hits);
        show("now", &fresh, &hits);
    }
}
