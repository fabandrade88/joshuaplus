//! Property-based robustness tests: random (often hostile) operation
//! sequences against the mixer, metronome, count-in planner and tempo edits.
//! Invariants: no panic, output always finite and within [-1, 1], tempo
//! data always valid.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use joshua_core::StereoBuffer;
use joshua_core::metronome::{ClickTrack, plan_count_in};
use joshua_core::mixer::{MAX_TRACKS, Mixer, Session, SharedState};
use joshua_core::project::SongProject;
use joshua_core::tempo::{BeatAnalysis, TempoEdit};
use proptest::prelude::*;

fn grid(bpm: f64, n: usize, start: f64) -> BeatAnalysis {
    let p = 60.0 / bpm;
    BeatAnalysis {
        bpm,
        beats: (0..n).map(|k| start + k as f64 * p).collect(),
        beats_per_bar: 4,
        downbeat_phase: 0,
        confidence: 1.0,
        variable_tempo: false,
    }
}

#[derive(Debug, Clone)]
enum Op {
    Play,
    Pause,
    Seek(i64),
    Loop(i64, i64),
    NoLoop,
    Gain(usize, f32),
    Pan(usize, f32),
    Mute(usize, bool),
    Solo(usize, bool),
    Master(f32),
    Voice(usize),
    CountIn(u8, i64),
    Monitor(usize, f32),
    Render(usize, usize),
}

fn hostile_f32() -> impl Strategy<Value = f32> {
    prop_oneof![
        -200.0f32..200.0,
        Just(f32::NAN),
        Just(f32::INFINITY),
        Just(f32::NEG_INFINITY),
        Just(f32::MAX),
        Just(-0.0),
    ]
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        Just(Op::Play),
        Just(Op::Pause),
        any::<i64>().prop_map(Op::Seek),
        (any::<i64>(), any::<i64>()).prop_map(|(a, b)| Op::Loop(a, b)),
        Just(Op::NoLoop),
        (0..MAX_TRACKS + 4, hostile_f32()).prop_map(|(i, g)| Op::Gain(i, g)),
        (0..MAX_TRACKS + 4, hostile_f32()).prop_map(|(i, p)| Op::Pan(i, p)),
        (0..MAX_TRACKS + 4, any::<bool>()).prop_map(|(i, m)| Op::Mute(i, m)),
        (0..MAX_TRACKS + 4, any::<bool>()).prop_map(|(i, s)| Op::Solo(i, s)),
        hostile_f32().prop_map(Op::Master),
        any::<usize>().prop_map(Op::Voice),
        (0u8..6, any::<i64>()).prop_map(|(b, s)| Op::CountIn(b, s)),
        (0usize..10, hostile_f32()).prop_map(|(i, g)| Op::Monitor(i, g)),
        (0usize..3000, 0usize..9).prop_map(|(f, c)| Op::Render(f, c)),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn mixer_survives_any_operation_sequence(ops in proptest::collection::vec(op(), 1..60), tracks in 0usize..5) {
        let rate = 8_000u32;
        let analysis = grid(120.0, 40, 0.3);
        let bufs = (0..tracks)
            .map(|k| StereoBuffer::new(rate, vec![0.9 - k as f32 * 0.1; 16_000], vec![-0.8; 16_000]).unwrap())
            .collect();
        let shared = SharedState::default();
        let mut mixer = Mixer::new();
        mixer.swap_session(Some(Box::new(Session::new(rate, bufs, ClickTrack::from_analysis(&analysis, rate)))));
        for op in ops {
            match op {
                Op::Play => shared.play(),
                Op::Pause => shared.pause(),
                Op::Seek(f) => shared.seek(f),
                Op::Loop(a, b) => shared.set_loop(Some((a, b))),
                Op::NoLoop => shared.set_loop(None),
                Op::Gain(i, g) => { if let Some(c) = shared.channels.get(i) { c.set_gain_db(g) } }
                Op::Pan(i, p) => { if let Some(c) = shared.channels.get(i) { c.set_pan(p) } }
                Op::Mute(i, m) => { if let Some(c) = shared.channels.get(i) { c.set_mute(m) } }
                Op::Solo(i, s) => { if let Some(c) = shared.channels.get(i) { c.set_solo(s) } }
                Op::Master(g) => shared.master_gain.store(if g.is_finite() { g.clamp(0.0, 4.0) } else { 1.0 }),
                Op::Voice(v) => shared.set_click_voice(v),
                Op::CountIn(bars, start) => {
                    if let Some(plan) = plan_count_in(&analysis, rate, start.clamp(-1_000_000, 1_000_000), bars) {
                        shared.arm_count_in(&plan);
                    }
                }
                Op::Monitor(i, g) => { if let Some(m) = shared.monitor_gain.get(i) { m.store(g) } }
                Op::Render(frames, channels) => {
                    let mut out = vec![0.0f32; frames * channels.max(1)];
                    mixer.render(&shared, &mut out, channels);
                    prop_assert!(out.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
                    prop_assert!(shared.position() >= 0);
                }
            }
        }
    }

    #[test]
    fn tempo_edits_keep_the_grid_valid(
        bpm in 40.0f64..240.0,
        n in 2usize..300,
        start in 0.0f64..5.0,
        phase in 0u8..4,
        edits in proptest::collection::vec(
            prop_oneof![
                Just(TempoEdit::Double),
                Just(TempoEdit::Half),
                (-1.0f64..1.0).prop_map(TempoEdit::Nudge),
                (-10.0f64..400.0).prop_map(TempoEdit::DownbeatAt),
                (0u8..20).prop_map(TempoEdit::BeatsPerBar),
                (proptest::collection::vec(-5.0f64..400.0, 0..12)).prop_map(TempoEdit::Tap),
                (-50.0f64..900.0, -10.0f64..400.0).prop_map(|(bpm, anchor)| TempoEdit::SetBpm { bpm, anchor }),
                Just(TempoEdit::SetBpm { bpm: f64::NAN, anchor: f64::INFINITY }),
            ],
            1..12,
        ),
    ) {
        let mut a = BeatAnalysis { downbeat_phase: phase, ..grid(bpm, n, start) };
        for e in edits {
            if let Some(next) = a.edited(e) {
                prop_assert!(next.beats.len() >= 2);
                prop_assert!(next.beats.windows(2).all(|w| w[1] > w[0]));
                prop_assert!(next.beats.iter().all(|b| b.is_finite() && *b >= 0.0));
                prop_assert!((20.0..=400.0).contains(&next.bpm));
                prop_assert!(next.downbeat_phase < next.beats_per_bar);
                a = next;
            }
        }
    }

    #[test]
    fn count_in_plans_are_bounded(bpm in 30.0f64..300.0, n in 0usize..80, start in any::<i64>(), bars in any::<u8>(), bpb in 1u8..16) {
        let a = BeatAnalysis { beats_per_bar: bpb, ..grid(bpm, n, 0.5) };
        if let Some(plan) = plan_count_in(&a, 48_000, start.clamp(-(1 << 40), 1 << 40), bars) {
            prop_assert!(!plan.clicks.is_empty() && plan.clicks.len() <= 64);
            prop_assert!(plan.preroll >= 0);
            prop_assert!(plan.clicks.windows(2).all(|w| w[1].0 > w[0].0));
        }
    }

    #[test]
    fn project_json_parser_never_panics(text in ".{0,400}") {
        let _ = serde_json::from_str::<SongProject>(&text);
    }
}
