//! `tempo_fold <file> <period_secs> <t0>` — onset energy folded over a beat.
#![allow(clippy::unwrap_used)]
use joshua_core::{decode::decode_file, security::Limits, tempo};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let audio = decode_file(a[1].as_ref(), &Limits::default()).unwrap();
    let fold = tempo::debug_fold(&audio, a[2].parse().unwrap(), a[3].parse().unwrap(), 24);
    let mf = fold.iter().map(|x| x.0).fold(0.0, f32::max);
    let ml = fold.iter().map(|x| x.1).fold(0.0, f32::max);
    for (i, (f, l)) in fold.iter().enumerate() {
        println!(
            "{:>5.2} full {:<26} low {}",
            i as f32 / 24.0,
            "#".repeat((f / mf * 25.0) as usize),
            "#".repeat((l / ml * 25.0) as usize)
        );
    }
}
