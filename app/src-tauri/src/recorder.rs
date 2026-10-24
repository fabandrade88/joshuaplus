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
use joshua_core::recording::{Marker, fit_alignment, render_take};
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
    let (song_id, existing_takes, stems) = lock(&state.current)
        .as_ref()
        .map(|s| (s.project.id.clone(), s.project.takes.len(), s.project.tracks.len()))
        .ok_or_else(ApiError::no_song)?;

    let inputs = lock(&state.devices.inputs);
    let armed: Vec<_> = inputs
        .iter()
        .filter(|i| i.config.armed)
        .filter_map(|i| i.stream.as_ref().and_then(|s| s.recorder.clone().map(|rx| (i, s, rx))))
        .collect();
    if armed.is_empty() && !allow_no_inputs {
        return Err(ApiError::new("no_armed_input", "arm at least one input to record"));
    }
    if existing_takes + armed.len() > MAX_TAKES || stems + existing_takes + armed.len() > MAX_TRACKS {
        return Err(ApiError::new("too_many_takes", "delete some takes first"));
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
            let mut w = StereoWavWriter::create(&takes_dir.join(format!("{id}.wav")), out_rate)?;
            w.write(take.audio.left(), take.audio.right())?;
            w.finish()?;
            next_id += 1;
            Ok(Some(TakeSettings {
                id,
                name: joshua_core::security::sanitize_title(&input.name).chars().take(80).collect(),
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
        project.takes.extend(new_takes);
        ws.save_project(&project)?;
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use joshua_core::mixer::SharedState;
    use joshua_engine::input::InputTap;

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
