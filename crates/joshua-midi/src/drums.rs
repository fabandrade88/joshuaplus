//! E-drum pads and note maps.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::message::MidiMessage;

/// The playing surfaces of a drum kit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DrumPad {
    Kick,
    Snare,
    SnareRim,
    SnareCrossStick,
    HiHatClosed,
    HiHatOpen,
    HiHatPedal,
    Tom1,
    Tom2,
    Tom3,
    Tom4,
    Crash1,
    Crash2,
    Ride,
    RideBell,
    Splash,
    China,
    Aux,
}

impl DrumPad {
    pub const ALL: [Self; 18] = [
        Self::Kick,
        Self::Snare,
        Self::SnareRim,
        Self::SnareCrossStick,
        Self::HiHatClosed,
        Self::HiHatOpen,
        Self::HiHatPedal,
        Self::Tom1,
        Self::Tom2,
        Self::Tom3,
        Self::Tom4,
        Self::Crash1,
        Self::Crash2,
        Self::Ride,
        Self::RideBell,
        Self::Splash,
        Self::China,
        Self::Aux,
    ];
}

/// Built-in note map presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum MapPreset {
    /// General MIDI percussion map: Alesis, Yamaha DTX, Simmons, Millenium,
    /// Gewa and most kits in their default setup.
    #[default]
    GeneralMidi,
    /// Roland V-Drums defaults (TD-02/07/17/27/50 family): adds rim, edge and
    /// bell zones on extra notes.
    RolandVDrums,
}

/// MIDI note → pad. A pad may be reached by several notes (zones).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteMap {
    pub notes: BTreeMap<u8, DrumPad>,
}

impl NoteMap {
    #[must_use]
    pub fn preset(preset: MapPreset) -> Self {
        use DrumPad::{
            China, Crash1, Crash2, HiHatClosed, HiHatOpen, HiHatPedal, Kick, Ride, RideBell, Snare, SnareCrossStick,
            SnareRim, Splash, Tom1, Tom2, Tom3, Tom4,
        };
        let pairs: &[(u8, DrumPad)] = match preset {
            MapPreset::GeneralMidi => &[
                (35, Kick),
                (36, Kick),
                (37, SnareCrossStick),
                (38, Snare),
                (40, SnareRim),
                (42, HiHatClosed),
                (44, HiHatPedal),
                (46, HiHatOpen),
                (48, Tom1),
                (50, Tom1),
                (45, Tom2),
                (47, Tom2),
                (43, Tom3),
                (41, Tom4),
                (49, Crash1),
                (57, Crash2),
                (51, Ride),
                (59, Ride),
                (53, RideBell),
                (55, Splash),
                (52, China),
            ],
            MapPreset::RolandVDrums => &[
                (36, Kick),
                (38, Snare),
                (40, SnareRim),
                (37, SnareCrossStick),
                (46, HiHatOpen),
                (26, HiHatOpen),
                (42, HiHatClosed),
                (22, HiHatClosed),
                (44, HiHatPedal),
                (48, Tom1),
                (50, Tom1),
                (45, Tom2),
                (47, Tom2),
                (43, Tom3),
                (58, Tom3),
                (41, Tom4),
                (39, Tom4),
                (49, Crash1),
                (55, Crash1),
                (57, Crash2),
                (52, Crash2),
                (51, Ride),
                (59, Ride),
                (53, RideBell),
            ],
        };
        Self { notes: pairs.iter().copied().collect() }
    }

    /// Assigns `note` to `pad` (MIDI learn). Notes above 127 are ignored.
    pub fn learn(&mut self, note: u8, pad: DrumPad) {
        if note <= 127 {
            self.notes.insert(note, pad);
        }
    }

    #[must_use]
    pub fn pad(&self, note: u8) -> Option<DrumPad> {
        self.notes.get(&note).copied()
    }

    /// Maps a message to a drum event.
    #[must_use]
    pub fn hit(&self, msg: MidiMessage) -> Option<DrumHit> {
        match msg {
            MidiMessage::NoteOn { note, velocity, .. } => Some(DrumHit::Strike { pad: self.pad(note), note, velocity }),
            MidiMessage::Control { controller: 4, value, .. } => Some(DrumHit::HiHatPedal { openness: 127 - value }),
            MidiMessage::PolyPressure { note, pressure, .. } if pressure > 0 => {
                Some(DrumHit::Choke { pad: self.pad(note), note })
            }
            _ => None,
        }
    }
}

impl Default for NoteMap {
    fn default() -> Self {
        Self::preset(MapPreset::GeneralMidi)
    }
}

/// What the drummer just did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum DrumHit {
    /// A pad was struck (`pad` is `None` for an unmapped note).
    Strike { pad: Option<DrumPad>, note: u8, velocity: u8 },
    /// Hi-hat pedal moved; 0 = fully closed, 127 = fully open.
    HiHatPedal { openness: u8 },
    /// A cymbal was grabbed (choked).
    Choke { pad: Option<DrumPad>, note: u8 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_cover_the_core_kit() {
        for preset in [MapPreset::GeneralMidi, MapPreset::RolandVDrums] {
            let map = NoteMap::preset(preset);
            assert_eq!(map.pad(36), Some(DrumPad::Kick));
            assert_eq!(map.pad(38), Some(DrumPad::Snare));
            assert_eq!(map.pad(42), Some(DrumPad::HiHatClosed));
            assert_eq!(map.pad(51), Some(DrumPad::Ride));
        }
        assert_eq!(NoteMap::preset(MapPreset::RolandVDrums).pad(22), Some(DrumPad::HiHatClosed));
    }

    #[test]
    fn maps_hits_pedal_and_chokes() {
        let mut map = NoteMap::default();
        assert_eq!(
            map.hit(MidiMessage::NoteOn { channel: 9, note: 38, velocity: 100 }),
            Some(DrumHit::Strike { pad: Some(DrumPad::Snare), note: 38, velocity: 100 })
        );
        assert_eq!(
            map.hit(MidiMessage::Control { channel: 9, controller: 4, value: 127 }),
            Some(DrumHit::HiHatPedal { openness: 0 })
        );
        assert_eq!(map.hit(MidiMessage::NoteOff { channel: 9, note: 38 }), None);
        assert!(matches!(
            map.hit(MidiMessage::NoteOn { channel: 9, note: 99, velocity: 5 }),
            Some(DrumHit::Strike { pad: None, .. })
        ));
        map.learn(99, DrumPad::Aux);
        map.learn(200, DrumPad::Aux);
        assert_eq!(map.pad(99), Some(DrumPad::Aux));
        assert!(!map.notes.contains_key(&200));
    }

    #[test]
    fn map_round_trips_through_json() {
        let map = NoteMap::preset(MapPreset::RolandVDrums);
        let json = serde_json::to_string(&map).unwrap();
        assert_eq!(serde_json::from_str::<NoteMap>(&json).unwrap(), map);
    }
}
