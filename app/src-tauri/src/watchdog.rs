//! Follows audio interfaces being plugged and unplugged.
//!
//! Every couple of seconds it checks the output device and the configured
//! inputs. When the chosen output disappears (or reports an error) the
//! engine moves to the system default; when the chosen one comes back it
//! moves back. Inputs whose device appeared or vanished are restarted. The
//! UI gets one `audio-devices-changed` event per change (a notice, not a
//! permanent error).

use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::{AppState, lock};

const EVERY: Duration = Duration::from_secs(2);
/// After the preferred output failed to open, wait this long to retry it.
const RETRY_PREFERRED: Duration = Duration::from_secs(30);

/// What changed, for the notice in the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevicesChanged {
    /// Output device that went away (or was left), if the output changed.
    pub lost_output: Option<String>,
    /// Output device now playing, if the output changed (`None`: none left).
    pub output: Option<String>,
    /// The preferred output came back.
    pub restored: bool,
    /// Input devices were connected or disconnected.
    pub inputs_changed: bool,
}

pub fn start(app: AppHandle) {
    let spawned = std::thread::Builder::new().name("joshua-devices".into()).spawn(move || {
        let mut retry_after: Option<Instant> = None;
        loop {
            std::thread::sleep(EVERY);
            let state = app.state::<AppState>();
            // A device switch mid-take would cut the recording: wait.
            if crate::recorder::is_recording(&state) {
                continue;
            }
            if let Some(change) = check(&app, &state, &mut retry_after) {
                let _ = app.emit("audio-devices-changed", change);
            }
        }
    });
    if let Err(e) = spawned {
        log::error!("could not start the device watchdog: {e}");
    }
}

fn check(app: &AppHandle, state: &AppState, retry_after: &mut Option<Instant>) -> Option<DevicesChanged> {
    let preferred = lock(&state.settings).output_device.clone();
    let outputs = joshua_engine::list_output_devices();
    let (running_id, running_name, failed) = match lock(&state.engine).as_ref() {
        Some(e) => (e.info().device_id.clone(), Some(e.info().device_name.clone()), e.has_device_error()),
        None => (None, None, true),
    };
    let may_retry = retry_after.is_none_or(|t| Instant::now() >= t);
    let preferred_back = may_retry
        && preferred.as_ref().is_some_and(|p| running_id.as_ref() != Some(p) && outputs.iter().any(|d| &d.id == p));
    let switch_output = (failed && !outputs.is_empty()) || preferred_back;

    if switch_output {
        log::info!("audio output changed (failed: {failed}, preferred back: {preferred_back}); restarting");
        let position = position_secs(state);
        state.start_engine();
        crate::devices::restart_inputs(state);
        reload_song(app, state, position);
        let (output, now_id) = lock(&state.engine)
            .as_ref()
            .map_or((None, None), |e| (Some(e.info().device_name.clone()), e.info().device_id.clone()));
        let restored = preferred.is_some() && now_id == preferred;
        // The preferred device is listed but would not open: do not retry
        // (and restart the audio) on every check.
        *retry_after = if preferred_back && !restored { Some(Instant::now() + RETRY_PREFERRED) } else { None };
        if !failed && !restored {
            return None;
        }
        return Some(DevicesChanged {
            lost_output: if failed { running_name } else { None },
            output,
            restored,
            inputs_changed: true,
        });
    }

    // Inputs: a configured device that came back, or one that went away.
    let available = joshua_engine::input::list_input_devices();
    let stale = lock(&state.devices.inputs).iter().any(|i| {
        let present = available.iter().any(|d| d.id == i.config.device_id);
        present != i.stream.is_some()
    });
    if stale {
        crate::devices::restart_inputs(state);
        return Some(DevicesChanged { lost_output: None, output: None, restored: false, inputs_changed: true });
    }
    None
}

fn position_secs(state: &AppState) -> Option<f64> {
    let rate = lock(&state.current).as_ref().map(|s| s.session_rate)?;
    let shared = state.shared().ok()?;
    Some(shared.position() as f64 / f64::from(rate.max(1)))
}

/// Puts the open song on the new engine, at the same place.
fn reload_song(app: &AppHandle, state: &AppState, position: Option<f64>) {
    let Some(id) = lock(&state.current).as_ref().map(|s| s.project.id.clone()) else { return };
    match crate::commands::load_song_blocking(app, &id) {
        Ok(_) => {
            let rate = lock(&state.current).as_ref().map(|s| s.session_rate);
            if let (Some(secs), Some(rate), Ok(shared)) = (position, rate, state.shared()) {
                shared.seek((secs * f64::from(rate)) as i64);
            }
            let _ = app.emit("song-reloaded", id);
        }
        Err(e) => log::warn!("could not reload the song on the new device: {e}"),
    }
}
