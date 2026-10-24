//! Backend side of the video studio. The webview records the webcam and
//! composes the final clips (crop per format, overlay); the backend provides
//! the audio — the real mix of the song and the takes, never the click — and
//! saves the encoded files into `<workspace>/Exports`.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Cursor, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use joshua_core::mixer::METRONOME;
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{AppHandle, Manager, State};

use crate::commands::flush_and_project;
use crate::error::{ApiError, ApiResult};
use crate::player::{apply_mix, build_session};
use crate::state::{AppState, lock};

/// Longest clip the studio renders audio for.
const MAX_CLIP_SECS: f64 = 30.0 * 60.0;
/// Largest video file accepted (a 30-minute 4K clip stays well below this).
const MAX_VIDEO_BYTES: u64 = 8 * 1024 * 1024 * 1024;
/// Clips that can be saved at the same time (one per export format).
const MAX_OPEN_SAVES: usize = 8;

/// Video files being written.
#[derive(Debug, Default)]
pub struct VideoSaves {
    next: AtomicU32,
    open: Mutex<HashMap<u32, OpenSave>>,
}

#[derive(Debug)]
struct OpenSave {
    file: File,
    part: PathBuf,
    path: PathBuf,
    written: u64,
}

/// Renders `duration_secs` of the open song's mix starting at song time
/// `start_secs` (negative = silence first, e.g. the count-in), without the
/// click, as a 16-bit stereo WAV the webview can decode.
#[tauri::command]
pub async fn render_clip_audio(app: AppHandle, start_secs: f64, duration_secs: f64) -> ApiResult<Response> {
    if !start_secs.is_finite() || !duration_secs.is_finite() || duration_secs <= 0.0 || duration_secs > MAX_CLIP_SECS {
        return Err(ApiError::invalid("bad clip range"));
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let (ws, project, rate) = flush_and_project(&state)?;
        let (session, _) = build_session(&ws, &project, rate)?;
        let r = f64::from(rate);
        let total = (duration_secs * r) as usize;
        let lead = if start_secs < 0.0 { ((-start_secs * r) as usize).min(total) } else { 0 };
        let from = (start_secs.max(0.0) * r) as i64;
        let to = from + (total - lead) as i64;

        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut bytes = Cursor::new(Vec::with_capacity(total * 4 + 64));
        {
            let mut w = hound::WavWriter::new(&mut bytes, spec).map_err(|e| ApiError::new("io", e.to_string()))?;
            let mut frames = 0usize;
            for _ in 0..lead * 2 {
                w.write_sample(0i16).map_err(|e| ApiError::new("io", e.to_string()))?;
            }
            frames += lead;
            joshua_core::export::render_mix(
                Box::new(session),
                &|shared| {
                    apply_mix(shared, &project);
                    shared.channels[METRONOME].set_mute(true); // videos never carry the click
                },
                from,
                to,
                &mut |l, rr| {
                    for (a, b) in l.iter().zip(rr) {
                        w.write_sample(to_i16(*a)).map_err(|e| joshua_core::CoreError::InvalidAudio(e.to_string()))?;
                        w.write_sample(to_i16(*b)).map_err(|e| joshua_core::CoreError::InvalidAudio(e.to_string()))?;
                    }
                    frames += l.len();
                    Ok(())
                },
                &|_| {},
                &AtomicBool::new(false),
            )?;
            // Past the song's end: silence up to the clip length.
            for _ in frames..total {
                w.write_sample(0i16).map_err(|e| ApiError::new("io", e.to_string()))?;
                w.write_sample(0i16).map_err(|e| ApiError::new("io", e.to_string()))?;
            }
            w.finalize().map_err(|e| ApiError::new("io", e.to_string()))?;
        }
        Ok(Response::new(bytes.into_inner()))
    })
    .await
    .map_err(|_| ApiError::new("internal", "audio render crashed"))?
}

/// Format tags become part of a file name: short, ASCII letters, digits and
/// dashes only. Containers are limited to what the studio produces.
fn check_clip_name(tag: &str, ext: &str) -> ApiResult<()> {
    if tag.is_empty() || tag.len() > 32 || !tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(ApiError::invalid("bad format tag"));
    }
    if !matches!(ext, "mp4" | "webm") {
        return Err(ApiError::invalid("unsupported container"));
    }
    Ok(())
}

fn to_i16(x: f32) -> i16 {
    (x.clamp(-1.0, 1.0) * 32_767.0).round() as i16
}

/// Opens a new export file for a clip. `tag` names the format (e.g.
/// "youtube"), `ext` is `mp4` or `webm`. Returns a handle for the chunks.
#[tauri::command]
pub fn video_save_begin(state: State<'_, AppState>, tag: String, ext: String) -> ApiResult<u32> {
    check_clip_name(&tag, &ext)?;
    let title = lock(&state.current).as_ref().map(|s| s.project.title.clone()).ok_or_else(ApiError::no_song)?;
    let path = joshua_core::workspace::new_export_path_in(&state.export_dir()?, &title, &tag, &ext)?;
    let part = PathBuf::from(format!("{}.part", path.display()));
    let _ = std::fs::remove_file(&part);
    let file = joshua_core::security::create_exclusive(&part, false)?;
    let mut open = lock(&state.videos.open);
    if open.len() >= MAX_OPEN_SAVES {
        return Err(ApiError::invalid("too many exports at once"));
    }
    let id = state.videos.next.fetch_add(1, Ordering::Relaxed);
    open.insert(id, OpenSave { file, part, path, written: 0 });
    Ok(id)
}

/// Appends one encoded chunk (raw request body; handle in `x-handle`).
#[tauri::command]
pub fn video_save_chunk(state: State<'_, AppState>, request: Request<'_>) -> ApiResult<()> {
    let id: u32 = request
        .headers()
        .get("x-handle")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| ApiError::invalid("missing handle"))?;
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err(ApiError::invalid("expected binary body"));
    };
    let mut open = lock(&state.videos.open);
    let save = open.get_mut(&id).ok_or_else(|| ApiError::invalid("unknown handle"))?;
    save.written += bytes.len() as u64;
    if save.written > MAX_VIDEO_BYTES {
        let save = open.remove(&id);
        if let Some(s) = save {
            let _ = std::fs::remove_file(&s.part);
        }
        return Err(ApiError::new("too_large", "video too large"));
    }
    save.file.write_all(bytes).map_err(|e| ApiError::new("io", e.kind().to_string()))
}

/// Finishes (`keep`) or discards a clip. Returns the saved file's path.
#[tauri::command]
pub fn video_save_end(state: State<'_, AppState>, handle: u32, keep: bool) -> ApiResult<Option<String>> {
    let save = lock(&state.videos.open).remove(&handle).ok_or_else(|| ApiError::invalid("unknown handle"))?;
    if !keep || save.written == 0 {
        let _ = std::fs::remove_file(&save.part);
        return Ok(None);
    }
    save.file.sync_all().map_err(|e| ApiError::new("io", e.kind().to_string()))?;
    drop(save.file);
    std::fs::rename(&save.part, &save.path).map_err(|e| ApiError::new("io", e.kind().to_string()))?;
    Ok(Some(save.path.to_string_lossy().into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_names_cannot_escape_or_smuggle_types() {
        assert!(check_clip_name("youtube", "mp4").is_ok());
        assert!(check_clip_name("vertical", "webm").is_ok());
        for (tag, ext) in [
            ("../../x", "mp4"),
            ("a/b", "mp4"),
            ("a\\b", "mp4"),
            ("", "mp4"),
            ("youtube", "exe"),
            ("youtube", "mp4/../../x"),
            ("youtube", "html"),
            ("x\u{0}", "mp4"),
            ("ç", "mp4"),
        ] {
            assert!(check_clip_name(tag, ext).is_err(), "{tag:?}.{ext:?} accepted");
        }
        assert!(check_clip_name(&"a".repeat(33), "mp4").is_err());
    }

    #[test]
    fn samples_are_clamped_to_16_bit() {
        assert_eq!(to_i16(2.0), i16::MAX);
        assert_eq!(to_i16(-2.0), -i16::MAX);
        assert_eq!(to_i16(0.0), 0);
        assert_eq!(to_i16(f32::NAN), 0);
    }
}
