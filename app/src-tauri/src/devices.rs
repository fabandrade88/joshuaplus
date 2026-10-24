//! Live inputs: audio interfaces (several at once) and MIDI e-drums.

use std::sync::{Arc, Mutex, PoisonError, RwLock};

use joshua_engine::input::{InputStream, MAX_INPUTS, list_input_devices, same_interface};
use joshua_midi::brands::{BRANDS, KitBrand, brand};
use joshua_midi::{DrumHit, DrumPad, MidiConnection, MidiMessage, MidiPortInfo, NoteMap, list_inputs};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::{ApiError, ApiResult};
use crate::settings::InputConfig;
use crate::state::{AppState, lock};

/// A configured input and its running stream.
#[derive(Debug)]
pub struct LiveInput {
    pub config: InputConfig,
    pub stream: Option<InputStream>,
    pub error: Option<String>,
    /// Monitoring possible (input and output share the sample rate).
    pub monitor_ok: bool,
    /// The input belongs to the output interface, which monitors it directly;
    /// the app does not play it again (echo / feedback protection).
    pub direct_monitor: bool,
}

#[derive(Debug, Default)]
pub struct Devices {
    pub inputs: Mutex<Vec<LiveInput>>,
    pub midi: Mutex<Option<MidiConnection>>,
    pub midi_error: Mutex<Option<String>>,
    pub drum_map: Arc<RwLock<NoteMap>>,
    /// Pad waiting for MIDI learn (next struck note is assigned to it).
    pub learn: Arc<Mutex<Option<DrumPad>>>,
}

// ── Views sent to the UI ────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputDeviceView {
    pub id: String,
    pub name: String,
    pub channels: u16,
    pub sample_rate: u32,
    pub is_default: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputView {
    pub index: usize,
    pub device_id: String,
    pub name: String,
    pub channels: u16,
    pub sample_rate: u32,
    pub first_channel: u16,
    pub stereo: bool,
    pub monitor: bool,
    pub gain_db: f32,
    pub monitor_ok: bool,
    pub direct_monitor: bool,
    pub armed: bool,
    /// The input can record (stream running with a song clock).
    pub can_record: bool,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrumStateView {
    pub ports: Vec<MidiPortInfo>,
    pub port_id: Option<String>,
    pub connected: Option<String>,
    pub error: Option<String>,
    pub brand: String,
    pub brands: &'static [KitBrand],
    pub map: NoteMap,
    pub learning: Option<DrumPad>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputPatch {
    pub first_channel: Option<u16>,
    pub stereo: Option<bool>,
    pub monitor: Option<bool>,
    pub gain_db: Option<f32>,
    pub armed: Option<bool>,
}

fn rlock<T>(l: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    l.read().unwrap_or_else(PoisonError::into_inner)
}

// ── Audio inputs ────────────────────────────────────────────────────────────

/// Applies routing and monitor level of input `slot` to the running engine.
fn apply_live_params(state: &AppState, slot: usize, input: &LiveInput) {
    if let Some(stream) = &input.stream {
        stream.controls.set_routing(input.config.first_channel, input.config.stereo);
    }
    if let Ok(shared) = state.shared() {
        let gain = if input.config.monitor && input.monitor_ok {
            joshua_core::audio::db_to_gain(input.config.gain_db)
        } else {
            0.0
        };
        shared.monitor_gain[slot].store(gain);
    }
}

/// (Re)opens every configured input. Called at start-up, after changes and
/// when the output device changes (monitor rings belong to the output).
pub fn restart_inputs(state: &AppState) {
    let configs = lock(&state.settings).inputs.clone();
    let available = list_input_devices();
    let mut inputs = lock(&state.devices.inputs);
    let previously_attached: Vec<usize> = (0..inputs.len()).filter(|i| inputs[*i].monitor_ok).collect();
    inputs.clear(); // drops (stops) the old streams first
    let mut engine = lock(&state.engine);
    // A stalled output device would make every command wait for its timeout.
    let engine_ok = engine.as_ref().is_some_and(|e| !e.has_device_error());
    if engine_ok && let Some(e) = engine.as_mut() {
        for slot in previously_attached {
            let _ = e.detach_monitor(slot);
        }
    }
    let out_rate = engine.as_ref().map(|e| e.info().sample_rate);
    let out_dev = engine.as_ref().map(|e| (e.info().device_id.clone(), e.info().device_name.clone()));
    let clock = engine.as_ref().map(|e| Arc::clone(e.shared()));
    for (slot, config) in configs.into_iter().enumerate().take(MAX_INPUTS) {
        let dev = available.iter().find(|d| d.id == config.device_id);
        let rates_match = dev.is_some_and(|d| Some(d.sample_rate) == out_rate);
        let direct_monitor = out_dev
            .as_ref()
            .is_some_and(|(id, name)| same_interface(&config.device_id, &config.name, id.as_deref(), name));
        let ring = if rates_match && engine_ok && !direct_monitor {
            engine.as_mut().and_then(|e| e.attach_monitor(slot).ok())
        } else {
            None
        };
        let attached = ring.is_some();
        let (stream, error) = match dev {
            None => (None, Some("device not connected".to_owned())),
            Some(_) => match InputStream::start(&config.device_id, ring, clock.clone()) {
                Ok(s) => (Some(s), None),
                Err(e) => (None, Some(e.to_string())),
            },
        };
        let live = LiveInput { config, monitor_ok: attached && stream.is_some(), direct_monitor, stream, error };
        drop(engine);
        apply_live_params(state, slot, &live);
        engine = lock(&state.engine);
        inputs.push(live);
    }
}

fn input_views(state: &AppState) -> Vec<InputView> {
    lock(&state.devices.inputs)
        .iter()
        .enumerate()
        .map(|(index, i)| InputView {
            index,
            device_id: i.config.device_id.clone(),
            name: i.config.name.clone(),
            channels: i.stream.as_ref().map_or(0, |s| s.info.channels),
            sample_rate: i.stream.as_ref().map_or(0, |s| s.info.sample_rate),
            first_channel: i.config.first_channel,
            stereo: i.config.stereo,
            monitor: i.config.monitor,
            gain_db: i.config.gain_db,
            monitor_ok: i.monitor_ok,
            direct_monitor: i.direct_monitor,
            armed: i.config.armed,
            can_record: i.stream.as_ref().is_some_and(|s| s.recorder.is_some()),
            error: i.error.clone(),
        })
        .collect()
}

/// Current input levels (routed channels), for the meters.
pub fn take_input_levels(state: &AppState) -> Vec<f32> {
    lock(&state.devices.inputs)
        .iter()
        .map(|i| i.stream.as_ref().map_or(0.0, |s| s.controls.take_routed_level()))
        .collect()
}

#[tauri::command]
pub fn list_audio_inputs() -> Vec<InputDeviceView> {
    list_input_devices()
        .into_iter()
        .map(|d| InputDeviceView {
            id: d.id,
            name: d.name,
            channels: d.channels,
            sample_rate: d.sample_rate,
            is_default: d.is_default,
        })
        .collect()
}

#[tauri::command]
pub fn get_inputs(state: State<'_, AppState>) -> Vec<InputView> {
    input_views(&state)
}

/// Adds an audio input (several devices can be open at the same time).
#[tauri::command]
pub async fn add_audio_input(app: AppHandle, device_id: String) -> ApiResult<Vec<InputView>> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let dev = list_input_devices()
            .into_iter()
            .find(|d| d.id == device_id)
            .ok_or_else(|| ApiError::new("not_found", "input device not found"))?;
        {
            let mut s = lock(&state.settings);
            if s.inputs.len() >= MAX_INPUTS {
                return Err(ApiError::invalid("too many inputs"));
            }
            s.inputs.push(InputConfig {
                device_id: dev.id,
                name: dev.name,
                first_channel: 0,
                stereo: dev.channels >= 2,
                monitor: false,
                gain_db: 0.0,
                armed: false,
            });
        }
        state.save_settings()?;
        restart_inputs(&state);
        Ok(input_views(&state))
    })
    .await
    .map_err(|_| ApiError::new("internal", "input setup crashed"))?
}

#[tauri::command]
pub async fn remove_audio_input(app: AppHandle, index: usize) -> ApiResult<Vec<InputView>> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        {
            let mut s = lock(&state.settings);
            if index >= s.inputs.len() {
                return Err(ApiError::invalid("no such input"));
            }
            s.inputs.remove(index);
        }
        state.save_settings()?;
        restart_inputs(&state);
        Ok(input_views(&state))
    })
    .await
    .map_err(|_| ApiError::new("internal", "input setup crashed"))?
}

/// Live changes (routing, monitoring, level) — no stream restart needed.
#[tauri::command]
pub fn update_audio_input(state: State<'_, AppState>, index: usize, patch: InputPatch) -> ApiResult<Vec<InputView>> {
    if patch.gain_db.is_some_and(|g| !g.is_finite()) {
        return Err(ApiError::invalid("gain must be finite"));
    }
    {
        let mut inputs = lock(&state.devices.inputs);
        let live = inputs.get_mut(index).ok_or_else(|| ApiError::invalid("no such input"))?;
        let channels = live.stream.as_ref().map_or(64, |s| s.info.channels);
        let c = &mut live.config;
        if let Some(ch) = patch.first_channel {
            c.first_channel = ch.min(channels.saturating_sub(1));
        }
        if let Some(st) = patch.stereo {
            c.stereo = st;
        }
        if let Some(m) = patch.monitor {
            c.monitor = m;
        }
        if let Some(g) = patch.gain_db {
            c.gain_db = g.clamp(-60.0, 12.0);
        }
        if let Some(a) = patch.armed {
            c.armed = a;
        }
        let config = c.clone();
        apply_live_params(&state, index, live);
        if let Some(saved) = lock(&state.settings).inputs.get_mut(index) {
            *saved = config;
        }
    }
    state.save_settings()?;
    Ok(input_views(&state))
}

// ── MIDI e-drums ────────────────────────────────────────────────────────────

/// Payload of the `drum-hit` event.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrumEvent {
    #[serde(flatten)]
    pub hit: DrumHit,
}

fn on_midi(app: &AppHandle, msg: MidiMessage) {
    let state = app.state::<AppState>();
    // MIDI learn: the next struck note becomes the waiting pad.
    if let MidiMessage::NoteOn { note, .. } = msg {
        let pending = lock(&state.devices.learn).take();
        if let Some(pad) = pending {
            state.devices.drum_map.write().unwrap_or_else(PoisonError::into_inner).learn(note, pad);
            let map = rlock(&state.devices.drum_map).clone();
            lock(&state.settings).drums.map = map;
            // Disk I/O off the MIDI thread, so hits keep low latency.
            let handle = app.clone();
            std::thread::spawn(move || {
                let _ = handle.state::<AppState>().save_settings();
            });
            let _ = app.emit("drum-learned", (pad, note));
        }
    }
    if let Some(hit) = rlock(&state.devices.drum_map).hit(msg) {
        let _ = app.emit("drum-hit", DrumEvent { hit });
    }
}

/// Opens the configured MIDI port (if any).
pub fn connect_midi(app: &AppHandle) {
    let state = app.state::<AppState>();
    let (port, map) = {
        let s = lock(&state.settings);
        (s.drums.port_id.clone(), s.drums.map.clone())
    };
    *state.devices.drum_map.write().unwrap_or_else(PoisonError::into_inner) = map;
    *lock(&state.devices.midi) = None;
    *lock(&state.devices.midi_error) = None;
    let Some(port) = port else { return };
    let handle = app.clone();
    match MidiConnection::open(&port, move |_, msg| on_midi(&handle, msg)) {
        Ok(conn) => *lock(&state.devices.midi) = Some(conn),
        Err(e) => *lock(&state.devices.midi_error) = Some(e.to_string()),
    }
}

fn drum_view(state: &AppState) -> DrumStateView {
    let s = lock(&state.settings).drums.clone();
    DrumStateView {
        ports: list_inputs().unwrap_or_default(),
        port_id: s.port_id,
        connected: lock(&state.devices.midi).as_ref().map(|c| c.port.name.clone()),
        error: lock(&state.devices.midi_error).clone(),
        brand: s.brand,
        brands: BRANDS,
        map: rlock(&state.devices.drum_map).clone(),
        learning: *lock(&state.devices.learn),
    }
}

#[tauri::command]
pub fn get_drums(state: State<'_, AppState>) -> DrumStateView {
    drum_view(&state)
}

#[tauri::command]
pub fn set_midi_port(app: AppHandle, port_id: Option<String>) -> ApiResult<DrumStateView> {
    if port_id.as_ref().is_some_and(|p| p.len() > 512) {
        return Err(ApiError::invalid("port id too long"));
    }
    let state = app.state::<AppState>();
    lock(&state.settings).drums.port_id = port_id;
    state.save_settings()?;
    connect_midi(&app);
    Ok(drum_view(&state))
}

/// Selects the kit brand and loads its default note map.
#[tauri::command]
pub fn set_drum_brand(state: State<'_, AppState>, brand_id: String) -> ApiResult<DrumStateView> {
    let b = brand(&brand_id);
    let map = NoteMap::preset(b.preset);
    {
        let mut s = lock(&state.settings);
        s.drums.brand = b.id.to_owned();
        s.drums.map = map.clone();
    }
    *state.devices.drum_map.write().unwrap_or_else(PoisonError::into_inner) = map;
    state.save_settings()?;
    Ok(drum_view(&state))
}

/// Arms MIDI learn for `pad` (`None` cancels).
#[tauri::command]
pub fn drum_learn(state: State<'_, AppState>, pad: Option<DrumPad>) -> DrumStateView {
    *lock(&state.devices.learn) = pad;
    drum_view(&state)
}
