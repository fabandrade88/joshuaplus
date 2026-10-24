//! A MIDI device is an untrusted input: arbitrary bytes and message floods
//! (a stuck controller, a buggy module, a malicious device) must neither
//! crash nor stall Joshua+.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Instant;

use joshua_midi::drums::MapPreset;
use joshua_midi::message::parse;
use joshua_midi::{DrumHit, NoteMap};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5_000))]

    #[test]
    fn any_bytes_parse_safely(bytes in proptest::collection::vec(any::<u8>(), 0..16)) {
        if let Some(msg) = parse(&bytes) {
            // Whatever parses must map without panicking and keep values in MIDI range.
            match NoteMap::preset(MapPreset::RolandVDrums).hit(msg) {
                Some(DrumHit::Strike { note, velocity, .. }) => prop_assert!(note <= 127 && velocity <= 127),
                Some(DrumHit::HiHatPedal { openness }) => prop_assert!(openness <= 127),
                _ => {}
            }
        }
    }

    #[test]
    fn learning_any_note_keeps_the_map_valid(notes in proptest::collection::vec(any::<u8>(), 0..300)) {
        let mut map = NoteMap::default();
        for n in notes {
            map.learn(n, joshua_midi::DrumPad::Aux);
        }
        prop_assert!(map.notes.keys().all(|n| *n <= 127));
        prop_assert!(map.notes.len() <= 128);
    }
}

#[test]
fn message_flood_is_handled_fast() {
    let map = NoteMap::preset(MapPreset::GeneralMidi);
    let mut state = 0x1234_5678u32;
    let start = Instant::now();
    let mut hits = 0u32;
    for _ in 0..1_000_000 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let b = state.to_le_bytes();
        if let Some(msg) = parse(&[0x90 | (b[0] & 0x0F), b[1] & 0x7F, b[2] & 0x7F]) {
            if map.hit(msg).is_some() {
                hits += 1;
            }
        }
    }
    let elapsed = start.elapsed();
    assert!(hits > 0);
    // A fast drummer plays ~30 hits/s; even a flood of 1M messages must take well under a second.
    assert!(elapsed.as_millis() < 1_500, "1M MIDI messages took {elapsed:?}");
}
