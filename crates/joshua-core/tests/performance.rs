//! Performance regression tests. Run in release mode:
//!
//! ```sh
//! cargo test --release -p joshua-core --test performance -- --ignored --nocapture
//! ```
//!
//! Each test prints its speed as a multiple of real time ("×RT") and fails
//! if it drops below a conservative floor.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::atomic::AtomicBool;
use std::time::Instant;

use joshua_core::StereoBuffer;
use joshua_core::decode::decode_file;
use joshua_core::metronome::ClickTrack;
use joshua_core::mixer::{Mixer, Session, SharedState};
use joshua_core::peaks::compute_peaks;
use joshua_core::resample::resample;
use joshua_core::security::Limits;
use joshua_core::separation::spectral::{SpectralConfig, separate_spectral};
use joshua_core::tempo;
use joshua_core::wav::write_stereo_wav;

const RATE: u32 = 44_100;

/// A minute of drum-like, bassy, harmonic material.
fn song(secs: usize) -> StereoBuffer {
    let n = RATE as usize * secs;
    let mut l = vec![0.0f32; n];
    let mut r = vec![0.0f32; n];
    for i in 0..n {
        let t = i as f32 / RATE as f32;
        let beat = t % 0.5;
        let kick = (std::f32::consts::TAU * 55.0 * t).sin() * (-beat * 18.0).exp();
        let chord = 0.2 * (std::f32::consts::TAU * 220.0 * t).sin() + 0.15 * (std::f32::consts::TAU * 330.0 * t).sin();
        l[i] = 0.6 * kick + chord;
        r[i] = 0.6 * kick + 0.8 * chord;
    }
    StereoBuffer::new(RATE, l, r).unwrap()
}

fn report(name: &str, audio_secs: f64, elapsed: f64, floor: f64) {
    let rt = audio_secs / elapsed;
    println!("{name:<28} {audio_secs:>5.0}s audio in {elapsed:>7.3}s  →  {rt:>8.1}×RT (floor {floor}×)");
    assert!(rt >= floor, "{name} regressed: {rt:.1}× real time < {floor}×");
}

#[test]
#[ignore = "performance: run in release with --ignored"]
fn mixer_render_seven_stems() {
    let audio = song(60);
    let tracks: Vec<StereoBuffer> = (0..7).map(|_| audio.clone()).collect();
    let analysis = tempo::BeatAnalysis {
        bpm: 120.0,
        beats: (0..120).map(|k| f64::from(k) * 0.5).collect(),
        beats_per_bar: 4,
        downbeat_phase: 0,
        confidence: 1.0,
        variable_tempo: false,
    };
    let shared = SharedState::default();
    let mut mixer = Mixer::new();
    mixer.swap_session(Some(Box::new(Session::new(RATE, tracks, ClickTrack::from_analysis(&analysis, RATE)))));
    shared.play();
    let mut out = vec![0.0f32; 256 * 2];
    let blocks = 60 * RATE as usize / 256;
    let start = Instant::now();
    for _ in 0..blocks {
        mixer.render(&shared, &mut out, 2);
    }
    // The audio callback must use only a small fraction of each block's time budget.
    report("mixer (7 stems + click)", 60.0, start.elapsed().as_secs_f64(), 200.0);
}

#[test]
#[ignore = "performance: run in release with --ignored"]
fn tempo_analysis() {
    let audio = song(60);
    let start = Instant::now();
    let a = tempo::analyze(&audio);
    report("tempo + beat tracking", 60.0, start.elapsed().as_secs_f64(), 30.0);
    assert!((a.bpm - 120.0).abs() < 1.0, "bpm {}", a.bpm);
}

#[test]
#[ignore = "performance: run in release with --ignored"]
fn quick_separation() {
    let audio = song(30);
    let start = Instant::now();
    separate_spectral(&audio, &SpectralConfig::default(), &mut |_, _, _| Ok(()), &|_| {}, &AtomicBool::new(false))
        .unwrap();
    report("quick separation (DSP)", 30.0, start.elapsed().as_secs_f64(), 8.0);
}

#[test]
#[ignore = "performance: run in release with --ignored"]
fn decode_and_resample() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("song.wav");
    write_stereo_wav(&path, &song(60)).unwrap();
    let start = Instant::now();
    let audio = decode_file(&path, &Limits::default()).unwrap();
    report("decode WAV", 60.0, start.elapsed().as_secs_f64(), 50.0);
    let start = Instant::now();
    let _ = resample(audio, 48_000).unwrap();
    report("resample 44.1→48 kHz", 60.0, start.elapsed().as_secs_f64(), 30.0);
}

#[test]
#[ignore = "performance: run in release with --ignored"]
fn waveform_peaks() {
    let audio = song(300);
    let start = Instant::now();
    let p = compute_peaks(&audio, 2_000);
    report("waveform peaks", 300.0, start.elapsed().as_secs_f64(), 200.0);
    assert_eq!(p.len(), 2_000);
}
