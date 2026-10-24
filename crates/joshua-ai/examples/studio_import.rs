//! End-to-end import with the studio engine (what the app does per song).
//! `cargo run --release -p joshua-ai --example studio_import -- <model-dir> <workspace> <audio>`
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use joshua_ai::separator::Accelerator;
use joshua_ai::{HTDEMUCS_6S, ModelStore, NeuralSeparator};
use joshua_core::import::{ImportOptions, import_song};
use joshua_core::security::Limits;
use joshua_core::workspace::Workspace;

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let no = AtomicBool::new(false);
    let model = ModelStore::new(args[0].clone()).install(&HTDEMUCS_6S, &|_| {}, &no).unwrap();
    let ws = Workspace::open(&args[1], Limits::default()).unwrap();
    let sep = NeuralSeparator::new(model, Accelerator::Auto);
    let t = Instant::now();
    let opts = ImportOptions { separator: &sep, folder: None, progress: &|_, _| {}, cancel: &no };
    let p = import_song(&ws, &args[2], &opts).unwrap();
    let a = &p.analysis;
    let d: Vec<f64> = a.beats.windows(2).map(|w| w[1] - w[0]).collect();
    let spread = d.iter().copied().fold(f64::MIN, f64::max) - d.iter().copied().fold(f64::MAX, f64::min);
    println!("{:.1}s song imported in {:.1}s", p.duration_secs, t.elapsed().as_secs_f64());
    println!(
        "bpm {:.2}  variable_tempo {}  beats {}  interval spread {:.6}s  first beat {:.3}s",
        a.bpm,
        a.variable_tempo,
        a.beats.len(),
        spread,
        a.beats.first().copied().unwrap_or(0.0)
    );
    println!("stems: {:?}", p.tracks.iter().map(|t| t.stem).collect::<Vec<_>>());
}
