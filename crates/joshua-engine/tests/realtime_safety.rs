//! The audio callback must never allocate or free memory: an allocation can
//! take a lock inside the OS allocator and cause an audible dropout. This
//! test installs an allocator that aborts on any allocation inside
//! `assert_no_alloc` and drives the processor through every feature.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use joshua_core::StereoBuffer;
use joshua_core::metronome::{ClickTrack, plan_count_in};
use joshua_core::mixer::Session;
use joshua_core::tempo::BeatAnalysis;
use joshua_engine::channel;

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

fn analysis() -> BeatAnalysis {
    BeatAnalysis {
        bpm: 120.0,
        beats: (0..400).map(|k| 0.25 + f64::from(k) * 0.5).collect(),
        beats_per_bar: 4,
        downbeat_phase: 0,
        confidence: 1.0,
        variable_tempo: false,
    }
}

fn session() -> Box<Session> {
    let rate = 48_000;
    let tracks = (0..7)
        .map(|k| {
            let s: Vec<f32> = (0..rate * 20).map(|i| ((i * (k + 3)) % 200) as f32 / 400.0 - 0.25).collect();
            StereoBuffer::new(rate as u32, s.clone(), s).unwrap()
        })
        .collect();
    Box::new(Session::new(48_000, tracks, ClickTrack::from_analysis(&analysis(), 48_000)))
}

#[test]
fn audio_callback_never_allocates() {
    let (mut ctl, mut processor) = channel(2);
    let mut monitor = ctl.attach_monitor(0, 48_000).unwrap();
    ctl.load(Some(session())).unwrap();
    ctl.set_click(ClickTrack::from_analysis(&analysis(), 48_000)).unwrap();

    let shared = ctl.shared().clone();
    shared.play();
    shared.channels[0].set_solo(true);
    shared.channels[1].set_mute(true);
    shared.channels[2].set_pan(-0.5);
    shared.monitor_gain[0].store(0.7);
    shared.set_click_voice(5);
    shared.arm_count_in(&plan_count_in(&analysis(), 48_000, 0, 2).unwrap());
    shared.set_loop(Some((48_000, 96_000)));

    let mut out_f32 = vec![0.0f32; 512 * 2];
    let mut out_i16 = vec![0i16; 37 * 2];
    for block in 0..2_000 {
        for _ in 0..512 {
            let _ = monitor.push(0.3);
            let _ = monitor.push(-0.3);
        }
        if block == 500 {
            shared.seek(10 * 48_000);
        }
        if block == 900 {
            shared.set_loop(None);
        }
        assert_no_alloc(|| {
            processor.process_f32(&mut out_f32, 2);
            processor.process(&mut out_i16, 2); // odd block size, integer device
        });
    }
    // Commands (session swap, click swap, monitor detach) are processed
    // inside the callback too: they must hand memory back, not free it.
    ctl.load(Some(session())).unwrap();
    ctl.set_click(ClickTrack::from_analysis(&analysis(), 48_000)).unwrap();
    ctl.detach_monitor(0).unwrap();
    assert_no_alloc(|| processor.process_f32(&mut out_f32, 2));
    assert!(out_f32.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
}
