//! Says whether recordings came from an overdriven input, and what
//! [`joshua_core::saturation::repair`] would rebuild.
//!
//! `cargo run --release -p joshua-core --example saturation_probe -- take.wav ...`
#![allow(clippy::unwrap_used)]
use std::path::Path;

use joshua_core::decode::decode_file;
use joshua_core::saturation::repair;
use joshua_core::security::Limits;

fn main() {
    for path in std::env::args().skip(1) {
        let audio = decode_file(Path::new(&path), &Limits::default()).unwrap();
        let peak = |x: &[f32]| 20.0 * x.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-6).log10();
        match repair(&audio) {
            Some((fixed, found)) => println!(
                "{path}: overdriven, squashed at {:.1} dBFS; {} tops rebuilt, peak {:.1} -> {:.1} dBFS",
                found.ceiling_db(),
                found.tops,
                peak(audio.left()).max(peak(audio.right())),
                peak(fixed.left()).max(peak(fixed.right())),
            ),
            None => println!("{path}: clean (peak {:.1} dBFS)", peak(audio.left()).max(peak(audio.right()))),
        }
    }
}
