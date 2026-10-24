//! Prints tempo diagnostics for audio files:
//! `cargo run --release -p joshua-core --example tempo_debug -- <file>...`
#![allow(clippy::unwrap_used, clippy::expect_used)]

use joshua_core::decode::decode_file;
use joshua_core::security::Limits;
use joshua_core::tempo;

fn main() {
    for path in std::env::args().skip(1) {
        let audio = decode_file(path.as_ref(), &Limits::default()).unwrap();
        let a = tempo::analyze(&audio);
        let iv: Vec<f64> = a.beats.windows(2).map(|w| w[1] - w[0]).collect();
        let (lo, hi) = iv.iter().fold((f64::MAX, f64::MIN), |(l, h), x| (l.min(*x), h.max(*x)));
        println!("== {path}");
        println!(
            "bpm {:.2} variable {} beats {} conf {:.2} interval {:.4}..{:.4} first {:.3}",
            a.bpm,
            a.variable_tempo,
            a.beats.len(),
            a.confidence,
            lo,
            hi,
            a.beats.first().copied().unwrap_or(0.0)
        );
        let w = tempo::debug_windows(&audio);
        let line: Vec<String> = w.iter().map(|(t, b, c)| format!("{t:.0}s:{b:.1}({c:.2})")).collect();
        println!("windows {}", line.join(" "));
    }
}
