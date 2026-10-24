//! MIDI port discovery and connections.

use midir::{Ignore, MidiInput, MidiInputConnection};
use serde::Serialize;

use crate::MidiError;
use crate::message::{MidiMessage, parse};

const CLIENT: &str = "Joshua+";

/// A MIDI input port as shown to the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MidiPortInfo {
    /// Stable id for reconnecting.
    pub id: String,
    pub name: String,
}

/// Lists MIDI input ports (USB e-drum modules, MIDI interfaces…).
///
/// # Errors
/// When the OS MIDI system cannot be opened.
pub fn list_inputs() -> Result<Vec<MidiPortInfo>, MidiError> {
    let input = MidiInput::new(CLIENT).map_err(|e| MidiError::Init(e.to_string()))?;
    Ok(input.ports().iter().filter_map(|p| Some(MidiPortInfo { id: p.id(), name: input.port_name(p).ok()? })).collect())
}

/// An open input; messages stop when this is dropped.
pub struct MidiConnection {
    _conn: MidiInputConnection<()>,
    pub port: MidiPortInfo,
}

impl std::fmt::Debug for MidiConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MidiConnection").field("port", &self.port).finish_non_exhaustive()
    }
}

impl MidiConnection {
    /// Connects to port `id`; `on_message(timestamp_us, message)` runs on the
    /// MIDI thread for every parsed message (keep it short).
    ///
    /// # Errors
    /// Unknown port or OS errors.
    pub fn open(id: &str, mut on_message: impl FnMut(u64, MidiMessage) + Send + 'static) -> Result<Self, MidiError> {
        let mut input = MidiInput::new(CLIENT).map_err(|e| MidiError::Init(e.to_string()))?;
        input.ignore(Ignore::All); // no SysEx, clock or active sensing
        let port = input.find_port_by_id(id).ok_or(MidiError::PortNotFound)?;
        let name = input.port_name(&port).unwrap_or_else(|_| "MIDI".to_owned());
        let conn = input
            .connect(
                &port,
                "joshua-in",
                move |ts, bytes, ()| {
                    if let Some(msg) = parse(bytes) {
                        on_message(ts, msg);
                    }
                },
                (),
            )
            .map_err(|e| MidiError::Connect(e.to_string()))?;
        Ok(Self { _conn: conn, port: MidiPortInfo { id: id.to_owned(), name } })
    }
}
