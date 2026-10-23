//! Headless import, handy for benchmarking and debugging:
//!
//! ```sh
//! cargo run --release -p joshua-core --example import -- <workspace> <audio-file>
//! ```
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use joshua_core::import::{ImportOptions, import_song};
use joshua_core::security::Limits;
use joshua_core::separation::spectral::SpectralSeparator;
use joshua_core::workspace::Workspace;

fn main() {
    let mut args = std::env::args().skip(1).map(PathBuf::from);
    let (Some(ws), Some(file)) = (args.next(), args.next()) else {
        eprintln!("usage: import <workspace> <audio-file>");
        std::process::exit(2);
    };
    let ws = Workspace::open(&ws, Limits::default()).expect("workspace");
    let sep = SpectralSeparator::default();
    let started = Instant::now();
    let opts = ImportOptions {
        separator: &sep,
        folder: None,
        progress: &|stage, p| eprint!("\r{stage:?} {:>3.0}%   ", p * 100.0),
        cancel: &AtomicBool::new(false),
    };
    let project = import_song(&ws, &file, &opts).expect("import");
    eprintln!();
    println!("id:        {}", project.id);
    println!("duration:  {:.1}s", project.duration_secs);
    println!("bpm:       {:.2} (confidence {:.2})", project.analysis.bpm, project.analysis.confidence);
    println!(
        "beats:     {} (first at {:.3}s, downbeat phase {})",
        project.analysis.beats.len(),
        project.analysis.beats.first().copied().unwrap_or(0.0),
        project.analysis.downbeat_phase
    );
    println!("stems:     {:?}", project.tracks.iter().map(|t| t.stem).collect::<Vec<_>>());
    println!("elapsed:   {:.2}s", started.elapsed().as_secs_f64());
}
