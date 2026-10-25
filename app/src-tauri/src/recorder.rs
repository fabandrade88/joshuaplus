//! Recording takes: every armed input is captured to its own file while the
//! song plays, then aligned to the song (latency + clock drift) and added to
//! the project as a take track.
//!
//! Capture never touches the audio threads: input callbacks push into
//! wait-free rings, and one writer thread per input drains them to a raw
//! 24-bit WAV in the workspace's private temp folder.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use joshua_core::decode::decode_file;
use joshua_core::mixer::MAX_TRACKS;
use joshua_core::project::{MAX_TAKES, TakeSettings};
use joshua_core::recording::{Marker, center_one_sided, fit_alignment, render_take};
use joshua_core::security::random_suffix;
use joshua_core::wav::StereoWavWriter;
use joshua_engine::input::{InputControls, RecordRx};

use crate::error::{ApiError, ApiResult};
use crate::state::{AppState, lock};

/// Longest take (safety net against a recording left running).
const MAX_TAKE_SECS: u64 = 60 * 60;
/// Writer wake-up interval (rings hold ~2.7 s).
const DRAIN_EVERY: Duration = Duration::from_millis(20);

/// A recording in progress.
#[derive(Debug)]
pub struct ActiveRecording {
    pub song_id: String,
    /// Song position (session frames) when recording started.
    pub start_pos: i64,
    inputs: Vec<ActiveInput>,
}

#[derive(Debug)]
struct ActiveInput {
    name: String,
    rate: u32,
    controls: Arc<InputControls>,
    stop: Arc<AtomicBool>,
    thread: JoinHandle<Result<RawTake, String>>,
}

/// What a writer thread captured.
#[derive(Debug)]
struct RawTake {
    path: PathBuf,
    frames: u64,
    markers: Vec<Marker>,
}

/// Drains one input's rings to `path` until `stop` (then drains the rest).
fn write_raw(rx: &Mutex<RecordRx>, path: PathBuf, rate: u32, stop: &AtomicBool) -> Result<RawTake, String> {
    let mut rx = lock(rx);
    let mut writer = StereoWavWriter::create(&path, rate).map_err(|e| e.to_string())?;
    let mut markers = Vec::new();
    let mut frames = 0u64;
    let (mut l, mut r) = (Vec::with_capacity(8_192), Vec::with_capacity(8_192));
    let max_frames = MAX_TAKE_SECS * u64::from(rate);
    loop {
        let stopping = stop.load(Ordering::Acquire);
        // Markers first: a marker is pushed after its samples, so every
        // marker read here describes samples that are already in the ring.
        while let Ok(m) = rx.markers.pop() {
            markers.push(m);
        }
        let n = rx.samples.slots() & !1;
        if n > 0
            && let Ok(chunk) = rx.samples.read_chunk(n)
        {
            l.clear();
            r.clear();
            let (a, b) = chunk.as_slices();
            let mut samples = a.iter().chain(b);
            while let (Some(left), Some(right)) = (samples.next(), samples.next()) {
                l.push(*left);
                r.push(*right);
            }
            chunk.commit_all();
            if frames < max_frames {
                writer.write(&l, &r).map_err(|e| e.to_string())?;
                frames += l.len() as u64;
            }
        }
        if stopping {
            break;
        }
        std::thread::sleep(DRAIN_EVERY);
    }
    let path = writer.finish().map_err(|e| e.to_string())?;
    Ok(RawTake { path, frames, markers })
}

/// Arms capture on every armed input and starts playback (with count-in).
/// A video recording may run without armed inputs (camera + song only).
pub fn start(state: &AppState, allow_no_inputs: bool) -> ApiResult<()> {
    let ws = state.workspace()?;
    let shared = state.shared()?;
    let mut active = lock(&state.recording);
    if active.is_some() {
        return Err(ApiError::new("recording", "already recording"));
    }
    let (song_id, take_names, stems) = lock(&state.current)
        .as_ref()
        .map(|s| {
            let names: Vec<String> = s.project.takes.iter().map(|t| t.name.clone()).collect();
            (s.project.id.clone(), names, s.project.tracks.len())
        })
        .ok_or_else(ApiError::no_song)?;
    let existing_takes = take_names.len();

    let inputs = lock(&state.devices.inputs);
    let armed: Vec<_> = inputs
        .iter()
        .filter(|i| i.config.armed)
        .filter_map(|i| i.stream.as_ref().and_then(|s| s.recorder.clone().map(|rx| (i, s, rx))))
        .collect();
    if armed.is_empty() && !allow_no_inputs {
        return Err(ApiError::new("no_armed_input", "arm at least one input to record"));
    }
    // Recording again over an input replaces its previous take: those takes
    // are silenced while recording (the song reload afterwards restores the
    // mix) and do not count against the limit.
    let replaced: Vec<usize> = take_names
        .iter()
        .enumerate()
        .filter(|(_, name)| armed.iter().any(|(i, _, _)| take_name(&i.config.name) == **name))
        .map(|(j, _)| j)
        .collect();
    let kept = existing_takes - replaced.len();
    if kept + armed.len() > MAX_TAKES || stems + kept + armed.len() > MAX_TRACKS {
        return Err(ApiError::new("too_many_takes", "delete some takes first"));
    }
    for j in &replaced {
        if let Some(ch) = shared.channels.get(stems + j).filter(|_| stems + j < MAX_TRACKS) {
            ch.set_mute(true);
        }
    }

    let mut started = Vec::with_capacity(armed.len());
    for (input, stream, rx) in armed {
        // Stale data from an earlier take (should be empty).
        {
            let mut rx = lock(&rx);
            while rx.samples.pop().is_ok() {}
            while rx.markers.pop().is_ok() {}
        }
        stream.controls.overflowed.store(false, Ordering::Relaxed);
        let rate = stream.info.sample_rate;
        let path = ws.tmp_dir().join(format!("rec-{}.wav", random_suffix()));
        let stop = Arc::new(AtomicBool::new(false));
        let stop_t = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("joshua-take-writer".into())
            .spawn(move || write_raw(&rx, path, rate, &stop_t))
            .map_err(|e| ApiError::new("recording", e.to_string()))?;
        stream.controls.set_recording(true);
        started.push(ActiveInput {
            name: input.config.name.clone(),
            rate,
            controls: Arc::clone(&stream.controls),
            stop,
            thread,
        });
    }
    drop(inputs);
    let start_pos = shared.position();
    *active = Some(ActiveRecording { song_id, start_pos, inputs: started });
    drop(active);
    crate::commands::start_playback(state)
}

/// Whether a recording is running.
pub fn is_recording(state: &AppState) -> bool {
    lock(&state.recording).is_some()
}

/// Outcome of [`stop`].
#[derive(Debug, Default)]
pub struct Stopped {
    pub song_id: String,
    pub start_pos: i64,
    pub added: usize,
    /// Some input's writer fell behind and audio was lost.
    pub overflowed: bool,
}

/// Stops capture and playback, aligns every take and saves it to the song.
pub fn stop(state: &AppState) -> ApiResult<Stopped> {
    let Some(rec) = lock(&state.recording).take() else {
        return Err(ApiError::new("recording", "not recording"));
    };
    for input in &rec.inputs {
        input.controls.set_recording(false);
    }
    if let Ok(shared) = state.shared() {
        shared.pause();
    }
    let ws = state.workspace()?;
    let (out_rate, out_latency) = lock(&state.engine)
        .as_ref()
        .map(|e| (e.info().sample_rate, e.output_latency_frames()))
        .ok_or_else(ApiError::no_audio)?;
    let offset_ms = f64::from(lock(&state.settings).record_offset_ms);

    let mut outcome = Stopped { song_id: rec.song_id.clone(), start_pos: rec.start_pos, ..Stopped::default() };
    let mut new_takes = Vec::new();
    let mut project = ws.load_project(&rec.song_id)?;
    let song_len = (project.duration_secs * f64::from(out_rate)) as i64;
    let takes_dir = ws.takes_dir(&rec.song_id)?;
    let mut next_id = project.takes.iter().filter_map(|t| t.id[5..].parse::<u32>().ok()).max().unwrap_or(0) + 1;

    for input in rec.inputs {
        input.stop.store(true, Ordering::Release);
        outcome.overflowed |= input.controls.overflowed.load(Ordering::Relaxed);
        let raw = match input.thread.join() {
            Ok(Ok(raw)) => raw,
            Ok(Err(e)) => {
                log::warn!("take writer failed: {e}");
                continue;
            }
            Err(_) => continue,
        };
        let result = (|| -> ApiResult<Option<TakeSettings>> {
            if raw.frames < 16 {
                return Ok(None);
            }
            let audio = decode_file(&raw.path, ws.limits())?;
            let Some(align) = fit_alignment(&raw.markers, f64::from(out_rate) / f64::from(input.rate)) else {
                return Ok(None); // the song never played
            };
            let in_latency = input.controls.latency_us.load(Ordering::Relaxed) as f64 * f64::from(out_rate) / 1e6;
            let latency = out_latency + in_latency + offset_ms * f64::from(out_rate) / 1000.0;
            let Some(take) = render_take(&audio, align, latency, out_rate, song_len)? else {
                return Ok(None);
            };
            let id = format!("take-{next_id}");
            let file = format!("takes/{id}.wav");
            // A mic or guitar on one input of a stereo pair: centre it.
            let audio = center_one_sided(take.audio);
            let mut w = StereoWavWriter::create(&takes_dir.join(format!("{id}.wav")), out_rate)?;
            w.write(audio.left(), audio.right())?;
            w.finish()?;
            next_id += 1;
            Ok(Some(TakeSettings {
                id,
                name: take_name(&input.name),
                file,
                offset_secs: take.start as f64 / f64::from(out_rate),
                created_unix: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs()),
                gain_db: 0.0,
                pan: 0.0,
                muted: false,
                solo: false,
                room: joshua_core::project::DEFAULT_TAKE_ROOM,
                tone: 0.0,
                tone_eq: [0.0; joshua_core::tone::TONE_BANDS],
                body: 0.0,
            }))
        })();
        let _ = std::fs::remove_file(&raw.path);
        match result {
            Ok(Some(t)) => new_takes.push(t),
            Ok(None) => {}
            Err(e) => log::warn!("could not save take: {e}"),
        }
    }

    outcome.added = new_takes.len();
    if !new_takes.is_empty() {
        // Keep the in-memory mixer edits, then add the takes.
        crate::ticker::flush_dirty(state);
        project = ws.load_project(&rec.song_id)?;
        let old_files = merge_takes(&mut project.takes, new_takes);
        ws.save_project(&project)?;
        if let Ok(dir) = ws.song_dir(&rec.song_id) {
            for file in old_files {
                let _ = std::fs::remove_file(dir.join(file));
            }
        }
    }
    Ok(outcome)
}

/// Name of the take recorded from an input (also how a new take finds the
/// one it replaces).
fn take_name(input_name: &str) -> String {
    joshua_core::security::sanitize_title(input_name).chars().take(80).collect()
}

/// Adds freshly recorded takes to a song. A take from an input that already
/// has one replaces it in place, keeping its level, pan, room and solo, so
/// recording again is "record over"; older extra takes of that input (from
/// before takes were replaced) go too. Returns the replaced files (relative
/// to the song folder) to delete once the project is saved.
fn merge_takes(takes: &mut Vec<TakeSettings>, new: Vec<TakeSettings>) -> Vec<String> {
    let mut old_files = Vec::new();
    for mut t in new {
        let Some(first) = takes.iter().position(|o| o.name == t.name) else {
            takes.push(t);
            continue;
        };
        let old = &takes[first];
        (t.gain_db, t.pan, t.room, t.solo) = (old.gain_db, old.pan, old.room, old.solo);
        // Same input, same instrument: its tone match still applies.
        (t.tone, t.tone_eq, t.body) = (old.tone, old.tone_eq, old.body);
        let name = t.name.clone();
        old_files.push(std::mem::replace(&mut takes[first], t).file);
        let mut j = 0;
        takes.retain(|o| {
            let extra = j != first && o.name == name;
            j += 1;
            if extra {
                old_files.push(o.file.clone());
            }
            !extra
        });
    }
    old_files
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use joshua_core::mixer::SharedState;
    use joshua_engine::input::InputTap;

    fn take(id: u32, name: &str) -> TakeSettings {
        TakeSettings {
            id: format!("take-{id}"),
            name: name.into(),
            file: format!("takes/take-{id}.wav"),
            offset_secs: 0.0,
            created_unix: 0,
            gain_db: 0.0,
            pan: 0.0,
            muted: false,
            solo: false,
            room: 0.3,
            tone: 0.0,
            tone_eq: [0.0; joshua_core::tone::TONE_BANDS],
            body: 0.0,
        }
    }

    #[test]
    fn recording_again_replaces_the_take_of_the_same_input() {
        let mut old_drums = take(1, "Yamaha AG06MK2");
        old_drums.gain_db = -4.0;
        old_drums.room = 0.5;
        old_drums.muted = true;
        // take-5 is an older duplicate of the same input.
        let mut takes = vec![old_drums, take(2, "SPD-SX PRO"), take(5, "Yamaha AG06MK2")];
        let removed = merge_takes(&mut takes, vec![take(3, "Yamaha AG06MK2"), take(4, "Guitar DI")]);
        assert_eq!(removed, ["takes/take-1.wav", "takes/take-5.wav"]);
        let ids: Vec<_> = takes.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["take-3", "take-2", "take-4"], "replaced in place, duplicates gone, new input appended");
        // The new drums take keeps the old mix but is heard.
        assert_eq!((takes[0].gain_db, takes[0].room, takes[0].muted), (-4.0, 0.5, false));
    }

    #[test]
    fn writer_captures_every_frame_and_marker_until_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let controls = Arc::new(InputControls::default());
        controls.set_routing(0, true);
        let clock = Arc::new(SharedState::default());
        let (mut tap, rx) = InputTap::new(Arc::clone(&controls), None).with_recorder(Arc::clone(&clock));
        let rx = Arc::new(Mutex::new(rx));
        let stop = Arc::new(AtomicBool::new(false));
        let (rx_t, stop_t, path) = (Arc::clone(&rx), Arc::clone(&stop), dir.path().join("raw.wav"));
        let writer = std::thread::spawn(move || write_raw(&rx_t, path, 48_000, &stop_t));

        controls.set_recording(true);
        let buffer: Vec<f32> = (0..512).flat_map(|i| [i as f32 / 1024.0, -0.5]).collect();
        // Feed blocks at a realistic pace (512 frames ≈ 10.7 ms at 48 kHz;
        // here ~5× faster than real time). Pushing as fast as the CPU can
        // would overflow the ring on slow CI machines — that is the input
        // outrunning any real device, not something the writer must absorb.
        for k in 0..400 {
            clock.seek(k * 512);
            tap.process(&buffer, 2);
            std::thread::sleep(Duration::from_millis(2));
        }
        controls.set_recording(false);
        stop.store(true, Ordering::Release);
        let raw = writer.join().unwrap().unwrap();
        assert_eq!(raw.frames, 400 * 512);
        assert_eq!(raw.markers.len(), 400);
        assert_eq!(raw.markers.last().map(|m| (m.input_frames, m.song_pos)), Some((400 * 512, 399 * 512)));
        let audio = decode_file(&raw.path, &joshua_core::security::Limits::default()).unwrap();
        assert_eq!(audio.frames(), 400 * 512);
        assert!((audio.right()[1000] + 0.5).abs() < 1e-4);
        assert!(!controls.overflowed.load(Ordering::Relaxed));
    }
}
