//! ~30 Hz transport/meter updates for the UI, plus mixer autosave.

use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use joshua_core::mixer::MAX_TRACKS;

use crate::state::{AppState, lock};

const TICK: Duration = Duration::from_millis(33);
const AUTOSAVE_EVERY: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportTick {
    pub position_secs: f64,
    pub playing: bool,
    pub counting_in: bool,
    pub meters: Vec<f32>,
    pub master: [f32; 2],
    pub song_meter: f32,
    /// The song just played to its end (drives playlist advance).
    pub ended: bool,
    pub device_error: bool,
    /// Takes are being recorded.
    pub recording: bool,
}

pub fn start(app: AppHandle) {
    let spawned = std::thread::Builder::new().name("joshua-ticker".into()).spawn(move || {
        let mut last_save = Instant::now();
        loop {
            std::thread::sleep(TICK);
            let state = app.state::<AppState>();
            tick(&app, &state);
            if last_save.elapsed() >= AUTOSAVE_EVERY {
                last_save = Instant::now();
                flush_dirty(&state);
            }
        }
    });
    if let Err(e) = spawned {
        log::error!("could not start ticker: {e}");
    }
}

fn tick(app: &AppHandle, state: &AppState) {
    let levels = crate::devices::take_input_levels(state);
    if !levels.is_empty() {
        let _ = app.emit("input-levels", levels);
    }
    let Ok(shared) = state.shared() else { return };
    let (rate, tracks) = match lock(&state.current).as_ref() {
        Some(song) => (song.session_rate, (song.project.tracks.len() + song.project.takes.len()).min(MAX_TRACKS)),
        None => return,
    };
    let device_error = lock(&state.engine).as_ref().is_some_and(joshua_engine::Engine::has_device_error);
    let payload = TransportTick {
        position_secs: shared.position() as f64 / f64::from(rate.max(1)),
        playing: shared.is_playing(),
        counting_in: shared.counting_in(),
        meters: shared.channels[..tracks].iter().map(|c| c.meter.take()).collect(),
        master: [shared.master_meter[0].take(), shared.master_meter[1].take()],
        song_meter: shared.song_bus.meter.take(),
        ended: shared.take_ended(),
        device_error,
        recording: crate::recorder::is_recording(state),
    };
    let _ = app.emit("transport", payload);
}

/// Writes pending mixer changes of the current song to disk.
pub fn flush_dirty(state: &AppState) {
    let Ok(ws) = state.workspace() else { return };
    let mut current = lock(&state.current);
    if let Some(song) = current.as_mut().filter(|s| s.dirty) {
        match ws.save_project(&song.project) {
            Ok(()) => song.dirty = false,
            Err(e) => log::warn!("autosave failed: {e}"),
        }
    }
}
