//! IPC commands. Every argument coming from the webview is treated as
//! untrusted: IDs are re-validated by the core, numbers are checked for
//! finiteness and range, collections are size-capped.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use joshua_ai::HTDEMUCS_6S;
use joshua_ai::cloud::LalalSeparator;
use joshua_core::folders::Folder;
use joshua_core::metronome::{ClickSound, ClickTrack, plan_count_in};
use joshua_core::mixer::METRONOME;
use joshua_core::project::SongProject;
use joshua_core::tempo::{BeatAnalysis, TempoEdit, TempoMode};
use joshua_core::workspace::{SongSummary, Workspace};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::{ApiError, ApiResult};
use crate::jobs::MAX_FILES_PER_REQUEST;
use crate::player::{apply_mix, build_session};
use crate::settings::{LOCALES, SeparatorChoice};
use crate::state::{AppState, LoadedSong, lock};
use crate::ticker::flush_dirty;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioInfo {
    pub device: String,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: &'static str,
    pub locale: Option<String>,
    pub workspace: Option<String>,
    pub inbox: Option<String>,
    pub separator: SeparatorChoice,
    /// Studio engine model present on disk.
    pub model_installed: bool,
    pub model_size_mb: u64,
    /// A cloud API key is stored (the key itself never leaves the backend).
    pub cloud_configured: bool,
    pub audio: Option<AudioInfo>,
    pub audio_error: Option<String>,
    pub output_device: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceView {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SongView {
    pub project: SongProject,
    pub peaks: Vec<Vec<u8>>,
    pub audio_ready: bool,
    /// Album cover as a `data:` URL.
    pub cover: Option<String>,
}

/// Library entry with its cover thumbnail (`data:` URL).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SongListItem {
    #[serde(flatten)]
    pub summary: SongSummary,
    pub cover: Option<String>,
}

fn cover_url(ws: &Workspace, id: &str, thumbnail: bool) -> Option<String> {
    let path = ws.cover_path(id, thumbnail).ok().flatten()?;
    joshua_core::metadata::jpeg_data_url(&path).ok()
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrackPatch {
    pub gain_db: Option<f32>,
    pub pan: Option<f32>,
    pub muted: Option<bool>,
    pub solo: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MetronomePatch {
    pub gain_db: Option<f32>,
    pub muted: Option<bool>,
    pub count_in: Option<bool>,
    pub count_in_bars: Option<u8>,
    pub sound: Option<ClickSound>,
}

fn checked_db(v: Option<f32>) -> ApiResult<Option<f32>> {
    match v {
        Some(db) if !db.is_finite() => Err(ApiError::invalid("gain must be finite")),
        Some(db) => Ok(Some(db.clamp(-96.0, 12.0))),
        None => Ok(None),
    }
}

fn checked_secs(secs: f64) -> ApiResult<f64> {
    if secs.is_finite() && secs >= 0.0 { Ok(secs) } else { Err(ApiError::invalid("time must be ≥ 0")) }
}

#[tauri::command]
pub fn get_app_info(state: State<'_, AppState>) -> AppInfo {
    let settings = lock(&state.settings).clone();
    let ws = state.workspace().ok();
    let audio = lock(&state.engine).as_ref().map(|e| {
        let i = e.info();
        AudioInfo { device: i.device_name.clone(), sample_rate: i.sample_rate, channels: i.channels }
    });
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        locale: settings.locale,
        workspace: ws.as_ref().map(|w| w.root().display().to_string()),
        inbox: ws.as_ref().map(|w| w.inbox_dir().display().to_string()),
        separator: settings.separator,
        model_installed: state.model_installed(),
        model_size_mb: HTDEMUCS_6S.size / 1_000_000,
        cloud_configured: settings.cloud_key.is_some(),
        audio,
        audio_error: lock(&state.engine_error).clone(),
        output_device: settings.output_device,
    }
}

#[tauri::command]
pub fn set_locale(state: State<'_, AppState>, locale: String) -> ApiResult<()> {
    if !LOCALES.contains(&locale.as_str()) {
        return Err(ApiError::invalid("unsupported locale"));
    }
    lock(&state.settings).locale = Some(locale);
    state.save_settings()
}

#[tauri::command]
pub fn set_separator(state: State<'_, AppState>, separator: SeparatorChoice) -> ApiResult<()> {
    lock(&state.settings).separator = separator;
    state.save_settings()
}

#[tauri::command]
pub fn set_workspace(app: AppHandle, state: State<'_, AppState>, path: String) -> ApiResult<String> {
    let path = PathBuf::from(path);
    if !path.is_absolute() || path.parent().is_none() {
        return Err(ApiError::invalid("choose a folder other than the filesystem root"));
    }
    let ws = Workspace::open(&path, joshua_core::security::Limits::default())?;
    let root = ws.root().display().to_string();
    flush_dirty(&state);
    *lock(&state.current) = None;
    if let Ok(shared) = state.shared() {
        shared.pause();
    }
    *state.workspace.write().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(ws.clone());
    lock(&state.settings).workspace = Some(ws.root().to_path_buf());
    state.save_settings()?;
    crate::backfill::start(app);
    Ok(root)
}

#[tauri::command]
pub fn list_songs(state: State<'_, AppState>) -> ApiResult<Vec<SongListItem>> {
    let ws = state.workspace()?;
    Ok(ws
        .list_songs()?
        .into_iter()
        .map(|summary| {
            let cover = if summary.has_cover { cover_url(&ws, &summary.id, true) } else { None };
            SongListItem { summary, cover }
        })
        .collect())
}

#[tauri::command]
pub fn rename_song(state: State<'_, AppState>, id: String, title: String) -> ApiResult<String> {
    if title.chars().count() > 200 {
        return Err(ApiError::invalid("title too long"));
    }
    flush_dirty(&state);
    let project = state.workspace()?.rename_song(&id, &title)?;
    if let Some(song) = lock(&state.current).as_mut().filter(|s| s.project.id == id) {
        song.project.title.clone_from(&project.title);
        song.project.title_edited = true;
    }
    Ok(project.title)
}

#[tauri::command]
pub fn import_files(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
    folder: Option<String>,
) -> ApiResult<Vec<u64>> {
    let ws = state.workspace()?;
    if paths.len() > MAX_FILES_PER_REQUEST {
        return Err(ApiError::invalid("too many files at once"));
    }
    if let Some(f) = &folder
        && !ws.list_folders()?.iter().any(|x| &x.id == f)
    {
        return Err(ApiError::new("not_found", "folder not found"));
    }
    Ok(paths
        .into_iter()
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .filter_map(|p| state.jobs.enqueue(&app, p, folder.clone(), false))
        .collect())
}

// ── Folders ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_folders(state: State<'_, AppState>) -> ApiResult<Vec<Folder>> {
    Ok(state.workspace()?.list_folders()?)
}

#[tauri::command]
pub fn create_folder(state: State<'_, AppState>, name: String, color: String) -> ApiResult<Folder> {
    Ok(state.workspace()?.create_folder(&name, &color)?)
}

#[tauri::command]
pub fn update_folder(
    state: State<'_, AppState>,
    id: String,
    name: Option<String>,
    color: Option<String>,
) -> ApiResult<Folder> {
    Ok(state.workspace()?.update_folder(&id, name.as_deref(), color.as_deref())?)
}

#[tauri::command]
pub fn delete_folder(state: State<'_, AppState>, id: String) -> ApiResult<()> {
    state.workspace()?.delete_folder(&id)?;
    // Keep the open song consistent, or autosave would write the dead id back.
    if let Some(song) = lock(&state.current).as_mut().filter(|s| s.project.folder.as_deref() == Some(id.as_str())) {
        song.project.folder = None;
    }
    Ok(())
}

/// Saves the playlist order of a folder; returns the order actually stored.
#[tauri::command]
pub fn reorder_folder(state: State<'_, AppState>, id: String, order: Vec<String>) -> ApiResult<Vec<String>> {
    if order.len() > joshua_core::folders::MAX_PLAYLIST {
        return Err(ApiError::invalid("playlist too long"));
    }
    Ok(state.workspace()?.reorder_folder(&id, &order)?)
}

#[tauri::command]
pub fn move_song(state: State<'_, AppState>, id: String, folder: Option<String>) -> ApiResult<()> {
    let project = state.workspace()?.move_song(&id, folder.as_deref())?;
    if let Some(song) = lock(&state.current).as_mut().filter(|s| s.project.id == id) {
        song.project.folder = project.folder;
    }
    Ok(())
}

// ── Engines ─────────────────────────────────────────────────────────────────

/// Downloads the studio engine model now (instead of on the first import).
/// Progress is reported through `model-progress` events (0..1).
#[tauri::command]
pub async fn download_model(app: AppHandle) -> ApiResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let report = |p: f32| {
            let _ = app.emit("model-progress", p);
        };
        state.ensure_model(&report, &AtomicBool::new(false))
    })
    .await
    .map_err(|_| ApiError::new("internal", "download crashed"))?
}

/// Validates a LALAL.AI key against the service and stores it.
/// Returns the processing minutes left on the account.
#[tauri::command]
pub async fn set_cloud_key(app: AppHandle, key: String) -> ApiResult<f64> {
    tauri::async_runtime::spawn_blocking(move || {
        let client = LalalSeparator::new(&key).map_err(|_| ApiError::new("cloud_key_invalid", "invalid key"))?;
        let minutes = client.minutes_left().map_err(|e| ApiError::new("cloud_key_invalid", e.to_string()))?;
        let state = app.state::<AppState>();
        lock(&state.settings).cloud_key = Some(key.trim().to_owned());
        state.save_settings()?;
        Ok(minutes)
    })
    .await
    .map_err(|_| ApiError::new("internal", "validation crashed"))?
}

#[tauri::command]
pub fn clear_cloud_key(state: State<'_, AppState>) -> ApiResult<()> {
    {
        let mut s = lock(&state.settings);
        s.cloud_key = None;
        if s.separator == SeparatorChoice::Cloud {
            s.separator = SeparatorChoice::Studio;
        }
    }
    state.save_settings()
}

#[tauri::command]
pub fn cancel_job(state: State<'_, AppState>, id: u64) -> bool {
    state.jobs.cancel(id)
}

#[tauri::command]
pub fn delete_song(state: State<'_, AppState>, id: String) -> ApiResult<()> {
    let ws = state.workspace()?;
    {
        let mut current = lock(&state.current);
        if current.as_ref().is_some_and(|s| s.project.id == id) {
            *current = None;
            if let Some(engine) = lock(&state.engine).as_mut() {
                engine.load(None)?;
            }
        }
    }
    Ok(ws.delete_song(&id)?)
}

/// Decodes the song's stems and installs them in the audio engine.
#[tauri::command]
pub async fn load_song(app: AppHandle, id: String) -> ApiResult<SongView> {
    tauri::async_runtime::spawn_blocking(move || load_song_blocking(&app, &id))
        .await
        .map_err(|_| ApiError::new("internal", "loader crashed"))?
}

fn load_song_blocking(app: &AppHandle, id: &str) -> ApiResult<SongView> {
    let state = app.state::<AppState>();
    // One load at a time: two overlapping loads could leave the engine
    // playing one song while `current` (and the UI) refer to the other.
    let _loading = lock(&state.load_lock);
    flush_dirty(&state);
    let ws = state.workspace()?;
    let project = ws.load_project(id)?;
    let rate = lock(&state.engine).as_ref().map_or(project.sample_rate, |e| e.info().sample_rate);
    let (session, peaks) = build_session(&ws, &project, rate)?;

    let mut audio_ready = false;
    if let Some(engine) = lock(&state.engine).as_mut() {
        apply_mix(engine.shared(), &project);
        engine.shared().set_loop(None);
        engine.shared().seek(0);
        engine.load(Some(Box::new(session)))?;
        let _ = engine.shared().take_ended(); // stale "ended" of the previous song
        audio_ready = true;
    }
    let cover = if project.has_cover { cover_url(&ws, &project.id, false) } else { None };
    let mut current = lock(&state.current);
    // Edits made to the previous song while this one was decoding.
    if let Some(old) = current.as_ref().filter(|s| s.dirty)
        && let Err(e) = ws.save_project(&old.project)
    {
        log::warn!("could not save previous song: {e}");
    }
    *current = Some(LoadedSong { project: project.clone(), dirty: false, session_rate: rate });
    Ok(SongView { project, peaks, audio_ready, cover })
}

fn with_song<T>(state: &AppState, f: impl FnOnce(&mut LoadedSong) -> ApiResult<T>) -> ApiResult<T> {
    lock(&state.current).as_mut().map_or_else(|| Err(ApiError::no_song()), f)
}

#[tauri::command]
pub fn play(state: State<'_, AppState>) -> ApiResult<()> {
    let shared = state.shared()?;
    with_song(&state, |song| {
        if shared.is_playing() {
            return Ok(());
        }
        // Pressing play at the very end starts over (or at the loop start).
        #[allow(clippy::cast_possible_truncation)]
        let end = (song.project.duration_secs * f64::from(song.session_rate)) as i64;
        if shared.position() >= end - i64::from(song.session_rate / 100) {
            shared.seek(0);
        }
        let m = &song.project.metronome;
        if m.count_in {
            // Count-in continues the song's own grid, so the music enters on time.
            if let Some(plan) =
                plan_count_in(&song.project.analysis, song.session_rate, shared.position(), m.count_in_bars)
            {
                shared.arm_count_in(&plan);
            }
        }
        shared.play();
        Ok(())
    })
}

#[tauri::command]
pub fn pause(state: State<'_, AppState>) -> ApiResult<()> {
    state.shared()?.pause();
    Ok(())
}

#[tauri::command]
pub fn seek(state: State<'_, AppState>, seconds: f64) -> ApiResult<()> {
    let secs = checked_secs(seconds)?;
    let shared = state.shared()?;
    with_song(&state, |song| {
        #[allow(clippy::cast_possible_truncation)]
        shared.seek((secs * f64::from(song.session_rate)) as i64);
        Ok(())
    })
}

#[tauri::command]
pub fn set_loop(state: State<'_, AppState>, start: Option<f64>, end: Option<f64>) -> ApiResult<()> {
    let shared = state.shared()?;
    with_song(&state, |song| {
        let rate = f64::from(song.session_rate);
        let region = match (start, end) {
            (Some(a), Some(b)) => {
                let (a, b) = (checked_secs(a)?, checked_secs(b)?.min(song.project.duration_secs));
                if b - a < 0.05 {
                    return Err(ApiError::invalid("loop too short"));
                }
                #[allow(clippy::cast_possible_truncation)]
                Some(((a * rate) as i64, (b * rate) as i64))
            }
            _ => None,
        };
        shared.set_loop(region);
        Ok(())
    })
}

#[tauri::command]
pub fn update_track(state: State<'_, AppState>, index: usize, patch: TrackPatch) -> ApiResult<()> {
    let gain = checked_db(patch.gain_db)?;
    if patch.pan.is_some_and(|p| !p.is_finite()) {
        return Err(ApiError::invalid("pan must be finite"));
    }
    let shared = state.shared().ok();
    with_song(&state, |song| {
        let track = song.project.tracks.get_mut(index).ok_or_else(|| ApiError::invalid("no such track"))?;
        let ch = shared.as_ref().map(|s| &s.channels[index]);
        if let Some(db) = gain {
            track.gain_db = db;
            ch.inspect(|c| c.set_gain_db(db));
        }
        if let Some(pan) = patch.pan.map(|p| p.clamp(-1.0, 1.0)) {
            track.pan = pan;
            ch.inspect(|c| c.set_pan(pan));
        }
        if let Some(m) = patch.muted {
            track.muted = m;
            ch.inspect(|c| c.set_mute(m));
        }
        if let Some(s) = patch.solo {
            track.solo = s;
            ch.inspect(|c| c.set_solo(s));
        }
        song.dirty = true;
        Ok(())
    })
}

#[tauri::command]
pub fn update_metronome(state: State<'_, AppState>, patch: MetronomePatch) -> ApiResult<()> {
    let gain = checked_db(patch.gain_db)?;
    let shared = state.shared().ok();
    with_song(&state, |song| {
        let m = &mut song.project.metronome;
        let ch = shared.as_ref().map(|s| &s.channels[METRONOME]);
        if let Some(db) = gain {
            m.gain_db = db;
            ch.inspect(|c| c.set_gain_db(db));
        }
        if let Some(muted) = patch.muted {
            m.muted = muted;
            ch.inspect(|c| c.set_mute(muted));
        }
        if let Some(ci) = patch.count_in {
            m.count_in = ci;
        }
        if let Some(bars) = patch.count_in_bars {
            if !(1..=4).contains(&bars) {
                return Err(ApiError::invalid("count-in must be 1–4 bars"));
            }
            m.count_in_bars = bars;
        }
        if let Some(sound) = patch.sound {
            m.sound = sound;
            if let Some(s) = shared.as_ref() {
                s.set_click_voice(sound.index());
            }
        }
        song.dirty = true;
        Ok(())
    })
}

/// Group level / mute of the whole song (all stems).
#[tauri::command]
pub fn update_song_bus(state: State<'_, AppState>, gain_db: Option<f32>, muted: Option<bool>) -> ApiResult<()> {
    let gain = checked_db(gain_db)?;
    let shared = state.shared().ok();
    with_song(&state, |song| {
        if let Some(db) = gain {
            song.project.song_gain_db = db;
            shared.as_ref().inspect(|s| s.song_bus.set_gain_db(db));
        }
        if let Some(m) = muted {
            song.project.song_muted = m;
            shared.as_ref().inspect(|s| s.song_bus.set_mute(m));
        }
        song.dirty = true;
        Ok(())
    })
}

/// Detects the tempo again with the current algorithm.
#[tauri::command]
pub async fn reanalyze_tempo(app: AppHandle, mode: Option<TempoMode>) -> ApiResult<BeatAnalysis> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let ws = state.workspace()?;
        let project = with_song(&state, |song| Ok(song.project.clone()))?;
        let analysis = joshua_core::import::reanalyze_tempo(&ws, &project, mode.unwrap_or(TempoMode::Auto))?;
        let rate = with_song(&state, |song| {
            // The user may have opened another song during the analysis.
            if song.project.id != project.id {
                return Err(ApiError::new("cancelled", "song changed during analysis"));
            }
            song.project.analysis = analysis.clone();
            song.dirty = true;
            Ok(song.session_rate)
        })?;
        if let Some(engine) = lock(&state.engine).as_mut() {
            engine.set_click(ClickTrack::from_analysis(&analysis, rate))?;
        }
        Ok(analysis)
    })
    .await
    .map_err(|_| ApiError::new("internal", "analysis crashed"))?
}

/// Applies a manual tempo correction and swaps the click track live.
#[tauri::command]
pub fn edit_tempo(state: State<'_, AppState>, edit: TempoEdit) -> ApiResult<BeatAnalysis> {
    let (analysis, rate) = with_song(&state, |song| {
        let edited =
            song.project.analysis.edited(edit.clone()).ok_or_else(|| ApiError::invalid("tempo out of range"))?;
        song.project.analysis = edited.clone();
        song.dirty = true;
        Ok((edited, song.session_rate))
    })?;
    if let Some(engine) = lock(&state.engine).as_mut() {
        engine.set_click(ClickTrack::from_analysis(&analysis, rate))?;
    }
    Ok(analysis)
}

#[tauri::command]
pub fn set_master_gain(state: State<'_, AppState>, gain_db: f32) -> ApiResult<()> {
    let db = checked_db(Some(gain_db))?.unwrap_or(0.0);
    if let Ok(shared) = state.shared() {
        shared.master_gain.store(joshua_core::audio::db_to_gain(db));
    }
    with_song(&state, |song| {
        song.project.master_gain_db = db;
        song.dirty = true;
        Ok(())
    })
}

#[tauri::command]
pub fn list_output_devices() -> Vec<DeviceView> {
    joshua_engine::list_output_devices()
        .into_iter()
        .map(|d| DeviceView { id: d.id, name: d.name, is_default: d.is_default })
        .collect()
}

/// Switches the output device and reloads the open song at the new rate.
#[tauri::command]
pub async fn set_output_device(app: AppHandle, id: Option<String>) -> ApiResult<()> {
    if id.as_ref().is_some_and(|d| d.len() > 512) {
        return Err(ApiError::invalid("device id too long"));
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        lock(&state.settings).output_device = id;
        state.save_settings()?;
        state.start_engine();
        crate::devices::restart_inputs(&state);
        let current_id = lock(&state.current).as_ref().map(|s| s.project.id.clone());
        if let Some(id) = current_id {
            load_song_blocking(&app, &id)?;
        }
        Ok(())
    })
    .await
    .map_err(|_| ApiError::new("internal", "device switch crashed"))?
}
