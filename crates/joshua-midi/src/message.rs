//! Raw MIDI bytes → typed messages. Never panics on any input.

use serde::Serialize;

/// The channel-voice messages Joshua+ cares about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum MidiMessage {
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        note: u8,
    },
    /// Polyphonic aftertouch – e-drums use it for cymbal chokes.
    PolyPressure {
        channel: u8,
        note: u8,
        pressure: u8,
    },
    /// Control change – CC4 carries the hi-hat pedal position.
    Control {
        channel: u8,
        controller: u8,
        value: u8,
    },
}

/// Parses one complete MIDI message. Unsupported or malformed input → `None`.
#[must_use]
pub fn parse(bytes: &[u8]) -> Option<MidiMessage> {
    let (&status, data) = bytes.split_first()?;
    if status & 0x80 == 0 || status >= 0xF0 {
        return None; // running status / system messages are not used here
    }
    let channel = status & 0x0F;
    let d0 = *data.first()?;
    let d1 = *data.get(1)?;
    if d0 > 0x7F || d1 > 0x7F {
        return None;
    }
    Some(match status & 0xF0 {
        // Note-on with velocity 0 is a note-off by specification.
        0x90 if d1 == 0 => MidiMessage::NoteOff { channel, note: d0 },
        0x90 => MidiMessage::NoteOn { channel, note: d0, velocity: d1 },
        0x80 => MidiMessage::NoteOff { channel, note: d0 },
        0xA0 => MidiMessage::PolyPressure { channel, note: d0, pressure: d1 },
        0xB0 => MidiMessage::Control { channel, controller: d0, value: d1 },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_drum_messages() {
        assert_eq!(parse(&[0x99, 36, 110]), Some(MidiMessage::NoteOn { channel: 9, note: 36, velocity: 110 }));
        assert_eq!(parse(&[0x99, 36, 0]), Some(MidiMessage::NoteOff { channel: 9, note: 36 }));
        assert_eq!(parse(&[0x89, 38, 64]), Some(MidiMessage::NoteOff { channel: 9, note: 38 }));
        assert_eq!(parse(&[0xB9, 4, 90]), Some(MidiMessage::Control { channel: 9, controller: 4, value: 90 }));
        assert_eq!(parse(&[0xA9, 49, 127]), Some(MidiMessage::PolyPressure { channel: 9, note: 49, pressure: 127 }));
    }

    #[test]
    fn rejects_garbage_without_panicking() {
        for bad in [&[][..], &[0x90], &[0x90, 36], &[36, 100], &[0xF0, 1, 2, 3], &[0x90, 0x80, 10], &[0xC0, 5, 0]] {
            assert_eq!(parse(bad), None, "{bad:?}");
        }
        // Exhaustive 3-byte sweep of status bytes: never panics.
        for status in 0..=255u8 {
            let _ = parse(&[status, 0x7F, 0xFF]);
            let _ = parse(&[status, 1, 2, 3, 4]);
        }
    }
}
