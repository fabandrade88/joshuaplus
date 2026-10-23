//! # joshua-engine
//!
//! Real-time audio output on top of [`joshua_core::mixer`].
//!
//! Threading model:
//! * A dedicated `joshua-audio` thread owns the `cpal` stream (streams are
//!   not `Send` on every platform).
//! * The device callback runs [`Processor`], which never allocates, locks or
//!   blocks. New sessions arrive through a wait-free SPSC ring buffer, and
//!   replaced sessions travel back through a second ring buffer so their
//!   (large) memory is freed on a control thread, not the audio thread.
//! * Mixer parameters and transport live in lock-free atomics
//!   ([`SharedState`]).
//!
//! The same structure will host audio *input* (recording) later: the
//! callback is the single place where device buffers meet the mixer.
#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};
use joshua_core::metronome::ClickTrack;
use joshua_core::mixer::{MAX_MONITORS, Mixer, Session, SharedState};

pub mod input;

pub use joshua_core::mixer;

/// Errors from the audio engine.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("no audio output device available")]
    NoDevice,
    #[error("audio device error: {0}")]
    Device(String),
    #[error("unsupported sample format: {0}")]
    UnsupportedFormat(String),
    #[error("audio thread terminated unexpectedly")]
    ThreadDied,
}

/// Description of an output device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// Parameters of the running stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamInfo {
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
}

enum Command {
    Load(Option<Box<Session>>),
    /// Replace only the click track (tempo edited) without reloading stems.
    SetClick(Box<ClickTrack>),
    /// Attach (`Some`) or detach (`None`) the monitor ring of input `slot`.
    SetMonitor(usize, Option<rtrb::Consumer<f32>>),
}

/// Memory handed back by the audio thread to be freed elsewhere.
enum Garbage {
    Session(#[allow(dead_code)] Box<Session>),
    Click(#[allow(dead_code)] Box<ClickTrack>),
    Monitor(#[allow(dead_code)] rtrb::Consumer<f32>),
}

const COMMAND_CAPACITY: usize = 16;
/// Largest block rendered at once when the device format is not `f32`.
const SCRATCH_FRAMES: usize = 4_096;

/// Audio-thread side: drains commands and renders blocks. Kept free of any
/// `cpal` types so it can be unit-tested without a sound card.
pub struct Processor {
    mixer: Mixer,
    shared: Arc<SharedState>,
    commands: rtrb::Consumer<Command>,
    garbage: rtrb::Producer<Garbage>,
    scratch: Vec<f32>,
    monitors: [Option<rtrb::Consumer<f32>>; MAX_MONITORS],
}

impl std::fmt::Debug for Processor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Processor").finish_non_exhaustive()
    }
}

/// Control side of the command channel.
pub struct Controller {
    shared: Arc<SharedState>,
    commands: rtrb::Producer<Command>,
    garbage: rtrb::Consumer<Garbage>,
}

impl std::fmt::Debug for Controller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Controller").finish_non_exhaustive()
    }
}

/// Creates a connected controller/processor pair.
#[must_use]
pub fn channel(channels: usize) -> (Controller, Processor) {
    let shared = Arc::new(SharedState::default());
    let (cmd_tx, cmd_rx) = rtrb::RingBuffer::new(COMMAND_CAPACITY);
    let (gc_tx, gc_rx) = rtrb::RingBuffer::new(COMMAND_CAPACITY);
    let controller = Controller { shared: Arc::clone(&shared), commands: cmd_tx, garbage: gc_rx };
    let processor = Processor {
        mixer: Mixer::new(),
        shared,
        commands: cmd_rx,
        garbage: gc_tx,
        scratch: vec![0.0; SCRATCH_FRAMES * channels.max(1)],
        monitors: std::array::from_fn(|_| None),
    };
    (controller, processor)
}

impl Controller {
    #[must_use]
    pub fn shared(&self) -> &Arc<SharedState> {
        &self.shared
    }

    /// Queues a session swap (`None` unloads). Frees sessions the audio
    /// thread has handed back.
    ///
    /// # Errors
    /// [`EngineError::ThreadDied`] if the queue stays full (audio thread gone).
    pub fn load(&mut self, session: Option<Box<Session>>) -> Result<(), EngineError> {
        self.shared.pause();
        self.send(Command::Load(session))
    }

    /// Creates a monitor ring for input `slot` and hands its consumer to the
    /// audio thread. Feed the returned producer to the input stream.
    ///
    /// # Errors
    /// [`EngineError::ThreadDied`] if the audio thread is gone.
    pub fn attach_monitor(&mut self, slot: usize, sample_rate: u32) -> Result<rtrb::Producer<f32>, EngineError> {
        // Half a second of stereo audio: plenty of slack, latency is trimmed on read.
        let (tx, rx) = rtrb::RingBuffer::new(sample_rate as usize);
        self.send(Command::SetMonitor(slot, Some(rx)))?;
        Ok(tx)
    }

    /// Stops mixing input `slot` into the output.
    ///
    /// # Errors
    /// [`EngineError::ThreadDied`] if the audio thread is gone.
    pub fn detach_monitor(&mut self, slot: usize) -> Result<(), EngineError> {
        self.send(Command::SetMonitor(slot, None))
    }

    /// Swaps the click track of the running session (no audio dropout).
    ///
    /// # Errors
    /// [`EngineError::ThreadDied`] if the audio thread is gone.
    pub fn set_click(&mut self, click: ClickTrack) -> Result<(), EngineError> {
        self.send(Command::SetClick(Box::new(click)))
    }

    fn send(&mut self, mut cmd: Command) -> Result<(), EngineError> {
        self.collect_garbage();
        for _ in 0..200 {
            match self.commands.push(cmd) {
                Ok(()) => return Ok(()),
                Err(rtrb::PushError::Full(back)) => {
                    cmd = back;
                    std::thread::sleep(Duration::from_millis(5));
                    self.collect_garbage();
                }
            }
        }
        Err(EngineError::ThreadDied)
    }

    /// Drops sessions returned by the audio thread.
    pub fn collect_garbage(&mut self) {
        while self.garbage.pop().is_ok() {}
    }
}

impl Processor {
    fn drain_commands(&mut self) {
        // If the return queue is full the old value is dropped here; this
        // cannot happen with a well-behaved controller.
        while let Ok(cmd) = self.commands.pop() {
            match cmd {
                Command::Load(session) => {
                    if let Some(old) = self.mixer.swap_session(session) {
                        let _ = self.garbage.push(Garbage::Session(old));
                    }
                }
                Command::SetClick(click) => {
                    let old = self.mixer.swap_click(click);
                    let _ = self.garbage.push(Garbage::Click(old));
                }
                Command::SetMonitor(slot, ring) => {
                    if let Some(entry) = self.monitors.get_mut(slot) {
                        if let Some(old) = std::mem::replace(entry, ring) {
                            let _ = self.garbage.push(Garbage::Monitor(old));
                        }
                    }
                }
            }
        }
    }

    /// Adds the monitored live inputs to a rendered block.
    fn mix_monitors(&mut self, out: &mut [f32], channels: usize) {
        let channels = channels.max(1);
        let frames = out.len() / channels;
        for (slot, ring) in self.monitors.iter_mut().enumerate() {
            let Some(ring) = ring else { continue };
            // Keep latency bounded: drop anything beyond ~2 blocks + 5 ms.
            let target = (frames * 2 + 256) * 2;
            // Even count: the ring holds interleaved L/R pairs; dropping an odd
            // number of samples would swap the channels from then on.
            let excess = ring.slots().saturating_sub(target) & !1;
            if excess > 0 {
                if let Ok(chunk) = ring.read_chunk(excess) {
                    chunk.commit_all();
                }
            }
            let gain = self.shared.monitor_gain[slot].load();
            let n = (ring.slots() / 2).min(frames);
            let Ok(chunk) = ring.read_chunk(n * 2) else { continue };
            if gain > 0.0 {
                let (a, b) = chunk.as_slices();
                for (f, pair) in a.iter().chain(b).copied().collect_pairs().take(n).enumerate() {
                    let base = f * channels;
                    if channels == 1 {
                        out[base] += 0.5 * (pair.0 + pair.1) * gain;
                    } else {
                        out[base] += pair.0 * gain;
                        out[base + 1] += pair.1 * gain;
                    }
                }
            }
            chunk.commit_all();
        }
        for s in out.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }
    }

    /// Renders an `f32` device buffer.
    pub fn process_f32(&mut self, out: &mut [f32], channels: usize) {
        self.drain_commands();
        self.mixer.render(&self.shared, out, channels);
        self.mix_monitors(out, channels);
    }

    /// Renders into any device sample format via the scratch buffer.
    pub fn process<T: SizedSample + FromSample<f32>>(&mut self, out: &mut [T], channels: usize) {
        self.drain_commands();
        let channels = channels.max(1);
        let chunk = (self.scratch.len() / channels) * channels;
        for block in out.chunks_mut(chunk.max(channels)) {
            let len = block.len().min(chunk);
            let mut scratch = std::mem::take(&mut self.scratch);
            self.mixer.render(&self.shared, &mut scratch[..len], channels);
            self.mix_monitors(&mut scratch[..len], channels);
            for (o, s) in block.iter_mut().zip(scratch[..len].iter()) {
                *o = T::from_sample(*s);
            }
            self.scratch = scratch;
        }
    }
}

/// Iterator adapter yielding consecutive `(left, right)` pairs.
trait CollectPairs: Iterator<Item = f32> + Sized {
    fn collect_pairs(self) -> Pairs<Self> {
        Pairs(self)
    }
}

impl<I: Iterator<Item = f32>> CollectPairs for I {}

struct Pairs<I>(I);

impl<I: Iterator<Item = f32>> Iterator for Pairs<I> {
    type Item = (f32, f32);
    fn next(&mut self) -> Option<(f32, f32)> {
        Some((self.0.next()?, self.0.next()?))
    }
}

/// Lists output devices of the default host.
#[must_use]
pub fn list_output_devices() -> Vec<DeviceInfo> {
    let host = cpal::default_host();
    let default_id = host.default_output_device().and_then(|d| d.id().ok()).map(|id| id.to_string());
    host.output_devices()
        .map(|devices| {
            devices
                .filter_map(|d| {
                    let id = d.id().ok()?.to_string();
                    let name = d.description().map(|desc| desc.name().to_owned()).unwrap_or_else(|_| id.clone());
                    Some(DeviceInfo { is_default: Some(&id) == default_id.as_ref(), id, name })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A running output stream.
pub struct Engine {
    controller: Controller,
    info: StreamInfo,
    stop: Arc<AtomicBool>,
    device_error: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine").field("info", &self.info).finish_non_exhaustive()
    }
}

impl Engine {
    /// Opens `device_id` (or the system default) and starts streaming silence.
    ///
    /// # Errors
    /// When no device exists or the stream cannot be built.
    pub fn start(device_id: Option<&str>) -> Result<Self, EngineError> {
        let stop = Arc::new(AtomicBool::new(false));
        let device_error = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(StreamInfo, Controller), EngineError>>();
        let device_id = device_id.map(str::to_owned);
        let (stop_t, err_t) = (Arc::clone(&stop), Arc::clone(&device_error));

        let thread = std::thread::Builder::new()
            .name("joshua-audio".into())
            .spawn(move || match open_stream(device_id.as_deref(), &err_t) {
                Ok((stream, info, controller)) => {
                    if ready_tx.send(Ok((info, controller))).is_err() {
                        return;
                    }
                    while !stop_t.load(Ordering::Relaxed) {
                        std::thread::park_timeout(Duration::from_millis(250));
                    }
                    drop(stream);
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            })
            .map_err(|e| EngineError::Device(e.to_string()))?;

        let (info, controller) = ready_rx.recv().map_err(|_| EngineError::ThreadDied)??;
        Ok(Self { controller, info, stop, device_error, thread: Some(thread) })
    }

    #[must_use]
    pub fn info(&self) -> &StreamInfo {
        &self.info
    }

    #[must_use]
    pub fn shared(&self) -> &Arc<SharedState> {
        self.controller.shared()
    }

    /// `true` once the device reported a fatal error (e.g. unplugged).
    #[must_use]
    pub fn has_device_error(&self) -> bool {
        self.device_error.load(Ordering::Relaxed)
    }

    /// Replaces the playing session.
    ///
    /// # Errors
    /// If the audio thread is gone.
    pub fn load(&mut self, session: Option<Box<Session>>) -> Result<(), EngineError> {
        self.controller.load(session)
    }

    /// See [`Controller::attach_monitor`].
    ///
    /// # Errors
    /// If the audio thread is gone.
    pub fn attach_monitor(&mut self, slot: usize) -> Result<rtrb::Producer<f32>, EngineError> {
        let rate = self.info.sample_rate;
        self.controller.attach_monitor(slot, rate)
    }

    /// See [`Controller::detach_monitor`].
    ///
    /// # Errors
    /// If the audio thread is gone.
    pub fn detach_monitor(&mut self, slot: usize) -> Result<(), EngineError> {
        self.controller.detach_monitor(slot)
    }

    /// Replaces the click track (after a tempo correction).
    ///
    /// # Errors
    /// If the audio thread is gone.
    pub fn set_click(&mut self, click: ClickTrack) -> Result<(), EngineError> {
        self.controller.set_click(click)
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            t.thread().unpark();
            let _ = t.join();
        }
    }
}

fn open_stream(
    device_id: Option<&str>,
    device_error: &Arc<AtomicBool>,
) -> Result<(cpal::Stream, StreamInfo, Controller), EngineError> {
    let host = cpal::default_host();
    let device = match device_id {
        Some(id) => {
            let parsed = id.parse().map_err(|_| EngineError::NoDevice)?;
            host.device_by_id(&parsed)
        }
        None => host.default_output_device(),
    }
    .ok_or(EngineError::NoDevice)?;
    let device_name = device.description().map(|d| d.name().to_owned()).unwrap_or_else(|_| "Audio".to_owned());
    let supported = device.default_output_config().map_err(|e| EngineError::Device(e.to_string()))?;
    let format = supported.sample_format();
    let config: StreamConfig = supported.into();
    let channels = usize::from(config.channels);
    let info = StreamInfo { device_name, sample_rate: config.sample_rate, channels: config.channels };
    let (controller, processor) = channel(channels);

    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, config, processor, device_error)?,
        SampleFormat::F64 => build::<f64>(&device, config, processor, device_error)?,
        SampleFormat::I16 => build::<i16>(&device, config, processor, device_error)?,
        SampleFormat::I32 => build::<i32>(&device, config, processor, device_error)?,
        SampleFormat::U16 => build::<u16>(&device, config, processor, device_error)?,
        SampleFormat::I8 => build::<i8>(&device, config, processor, device_error)?,
        SampleFormat::U8 => build::<u8>(&device, config, processor, device_error)?,
        other => return Err(EngineError::UnsupportedFormat(other.to_string())),
    };
    stream.play().map_err(|e| EngineError::Device(e.to_string()))?;
    Ok((stream, info, controller))
}

fn build<T: SizedSample + FromSample<f32> + 'static>(
    device: &cpal::Device,
    config: StreamConfig,
    mut processor: Processor,
    device_error: &Arc<AtomicBool>,
) -> Result<cpal::Stream, EngineError> {
    let channels = usize::from(config.channels);
    let err_flag = Arc::clone(device_error);
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| processor.process(data, channels),
            move |_err| err_flag.store(true, Ordering::Relaxed),
            None,
        )
        .map_err(|e| EngineError::Device(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use joshua_core::StereoBuffer;
    use joshua_core::metronome::ClickTrack;
    use joshua_core::tempo::BeatAnalysis;

    fn session(level: f32) -> Box<Session> {
        let click = ClickTrack::from_analysis(
            &BeatAnalysis {
                bpm: 120.0,
                beats: vec![],
                beats_per_bar: 4,
                downbeat_phase: 0,
                confidence: 0.0,
                variable_tempo: false,
            },
            48_000,
        );
        let track = StereoBuffer::new(48_000, vec![level; 48_000], vec![level; 48_000]).unwrap();
        Box::new(Session::new(48_000, vec![track], click))
    }

    #[test]
    fn sessions_are_swapped_and_returned_for_off_thread_drop() {
        let (mut ctl, mut proc) = channel(2);
        ctl.load(Some(session(0.5))).unwrap();
        ctl.shared().play();
        let mut out = vec![0.0f32; 256];
        proc.process_f32(&mut out, 2);
        proc.process_f32(&mut out, 2);
        assert!((out[0] - 0.5).abs() < 1e-6);

        ctl.load(Some(session(0.25))).unwrap();
        proc.process_f32(&mut out, 2);
        assert!(ctl.garbage.pop().is_ok(), "old session returned to the controller");
    }

    #[test]
    fn converts_to_integer_device_formats() {
        let (mut ctl, mut proc) = channel(2);
        ctl.load(Some(session(0.5))).unwrap();
        ctl.shared().play();
        let mut out = vec![0i16; 20_000]; // larger than the scratch buffer
        proc.process(&mut out, 2);
        proc.process(&mut out, 2);
        assert!(out.iter().all(|s| (i32::from(*s) - 16_384).abs() < 2));
    }

    #[test]
    fn trimming_latency_never_swaps_left_and_right() {
        let (mut ctl, mut proc) = channel(2);
        let mut ring = ctl.attach_monitor(0, 48_000).unwrap();
        ctl.shared().monitor_gain[0].store(1.0);
        for _ in 0..4_000 {
            ring.push(0.5).unwrap();
            ring.push(-0.5).unwrap();
        }
        ring.push(0.5).unwrap(); // half a frame: the producer was mid-pair
        let mut out = vec![0.0f32; 256];
        proc.process_f32(&mut out, 2);
        assert!((out[0] - 0.5).abs() < 1e-6 && (out[1] + 0.5).abs() < 1e-6, "channels swapped: {:?}", &out[..2]);
    }

    #[test]
    fn monitored_input_is_mixed_even_when_stopped() {
        let (mut ctl, mut proc) = channel(2);
        let mut ring = ctl.attach_monitor(0, 48_000).unwrap();
        ctl.shared().monitor_gain[0].store(0.5);
        for _ in 0..64 {
            ring.push(0.8).unwrap();
            ring.push(-0.4).unwrap();
        }
        let mut out = vec![0.0f32; 128];
        proc.process_f32(&mut out, 2);
        assert!((out[0] - 0.4).abs() < 1e-6 && (out[1] + 0.2).abs() < 1e-6);
        // Muted monitor still drains, so unmuting later is not delayed.
        ctl.shared().monitor_gain[0].store(0.0);
        for _ in 0..64 {
            ring.push(0.8).unwrap();
            ring.push(0.8).unwrap();
        }
        proc.process_f32(&mut out, 2);
        assert!(out.iter().all(|s| *s == 0.0));
        assert_eq!(ring.slots(), 48_000, "ring fully drained");
    }

    #[test]
    fn load_pauses_transport() {
        let (mut ctl, _proc) = channel(2);
        ctl.shared().play();
        ctl.load(None).unwrap();
        assert!(!ctl.shared().is_playing());
    }
}
