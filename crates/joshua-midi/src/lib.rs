//! # joshua-midi
//!
//! MIDI input for Joshua+, focused on electronic drum kits.
//!
//! * [`message`] – parsing of raw MIDI bytes (pure, fuzz-friendly).
//! * [`drums`] – pad model, note maps (General MIDI, Roland V-Drums) and
//!   "learn" so any brand can be mapped by simply hitting the pads.
//! * [`device`] – port discovery and connections (CoreMIDI, WinMM, ALSA).
//!
//! Almost every current e-drum module (Roland, Alesis, Yamaha, Simmons,
//! EFNOTE, Millenium, Gewa…) is a USB-MIDI *class-compliant* device: it
//! appears as a MIDI port with no driver on macOS, Windows and Linux.
//! Older modules with 5-pin DIN MIDI OUT work through any USB-MIDI cable.
#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod brands;
pub mod device;
pub mod drums;
pub mod message;

pub use device::{MidiConnection, MidiPortInfo, list_inputs};
pub use drums::{DrumHit, DrumPad, MapPreset, NoteMap};
pub use message::MidiMessage;

/// Errors from MIDI device access.
#[derive(Debug, thiserror::Error)]
pub enum MidiError {
    #[error("MIDI system unavailable: {0}")]
    Init(String),
    #[error("MIDI port not found")]
    PortNotFound,
    #[error("could not connect to MIDI port: {0}")]
    Connect(String),
}
