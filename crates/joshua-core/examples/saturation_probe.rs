//! Says whether recordings came from an overdriven input, and at what level.
//!
//! `cargo run --release -p joshua-core --example saturation_probe -- take.wav ...`
#![allow(clippy::unwrap_used)]
use std::path::Path;

use joshua_core::decode::decode_file;
use joshua_core::saturation::detect;
use joshua_core::security::Limits;

fn main() {
    for path in std::env::args().skip(1) {
        let audio = decode_file(Path::new(&path), &Limits::default()).unwrap();
        let peak = |x: &[f32]| 20.0 * x.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-6).log10();
        let peak = peak(audio.left()).max(peak(audio.right()));
        match detect(&audio) {
            Some(found) => println!(
                "{path}: overdriven, {} stretches squashed at {:.1} dBFS (peak {peak:.1} dBFS)",
                found.tops,
                found.ceiling_db(),
            ),
            None => println!("{path}: clean (peak {peak:.1} dBFS)"),
        }
    }
}
