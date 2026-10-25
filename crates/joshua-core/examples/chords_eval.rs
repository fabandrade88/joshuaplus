//! Chord chart of songs, bar by bar, as the app shows it:
//! `cargo run --release -p joshua-core --example chords_eval -- <song dir>...`
#![allow(clippy::unwrap_used, clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::path::Path;

use joshua_core::chords::{chord_at, detect};
use joshua_core::decode::decode_file;
use joshua_core::project::SongProject;
use joshua_core::security::Limits;
use joshua_core::separation::StemKind;

fn main() {
    let limits = Limits::default();
    for dir in std::env::args().skip(1) {
        let dir = Path::new(&dir);
        let p: SongProject = serde_json::from_str(&std::fs::read_to_string(dir.join("project.json")).unwrap()).unwrap();
        let started = std::time::Instant::now();
        let (mut harmony, mut bass, mut rate) = (Vec::<f32>::new(), Vec::<f32>::new(), 44_100);
        for t in &p.tracks {
            if matches!(t.stem, StemKind::Drums | StemKind::Vocals) {
                continue;
            }
            let b = decode_file(&dir.join(&t.file), &limits).unwrap();
            rate = b.sample_rate();
            let target = if t.stem == StemKind::Bass { &mut bass } else { &mut harmony };
            let m = b.to_mono();
            if target.len() < m.len() {
                target.resize(m.len(), 0.0);
            }
            for (o, x) in target.iter_mut().zip(m) {
                *o += x;
            }
        }
        let chart = detect(&harmony, &bass, rate, &p.analysis, p.duration_secs);
        println!(
            "== {} — key {} — {} chords in {:.1}s",
            p.title,
            chart.key,
            chart.chords.len(),
            started.elapsed().as_secs_f64()
        );
        let a = &p.analysis;
        let mut line = String::new();
        let mut bar = 0;
        for (i, b) in a.beats.iter().enumerate() {
            if a.is_downbeat(i) {
                if bar % 8 == 0 && !line.is_empty() {
                    println!("{line}");
                    line.clear();
                }
                if bar % 8 == 0 {
                    line.push_str(&format!("{:>4} ", bar + 1));
                }
                bar += 1;
                line.push('|');
            }
            let name = chord_at(&chart.chords, *b + 0.01).map_or("-", |c| chart.chords[c].name.as_str());
            line.push_str(&format!("{name:<4}"));
        }
        println!("{line}");
    }
}
