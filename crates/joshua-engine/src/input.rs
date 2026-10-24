//! Audio inputs: any number of interfaces at once (a guitar interface, the
//! e-drum module's USB audio, a USB mic…), each with per-channel meters and
//! optional monitoring through the output.
//!
//! Each input runs its own `cpal` stream on a dedicated thread. The input
//! callback is real-time safe: it updates atomic meters and, when monitoring
//! is on, pushes the routed stereo signal into a wait-free ring buffer that
//! the output [`crate::Processor`] mixes in. While recording, the same tap
//! also feeds a second ring read by the take writer, plus a [`Marker`] per
//! callback that ties input frames to the song position for alignment.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, StreamConfig};
use joshua_core::mixer::{AtomicF32, SharedState};
use joshua_core::recording::Marker;

use crate::EngineError;

/// Inputs that can be open at the same time.
pub const MAX_INPUTS: usize = 8;
/// Channels metered per input (large interfaces have 18–32).
pub const MAX_INPUT_CHANNELS: usize = 64;
/// Recording ring: ~2.7 s of stereo at 48 kHz between writer wake-ups.
const RECORD_RING_SAMPLES: usize = 1 << 18;
const MARKER_RING: usize = 1 << 14;

/// An input device as offered to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDeviceInfo {
    pub id: String,
    pub name: String,
    pub channels: u16,
    pub sample_rate: u32,
    pub is_default: bool,
}

/// Lists audio input devices of the default host.
#[must_use]
pub fn list_input_devices() -> Vec<InputDeviceInfo> {
    let host = cpal::default_host();
    let default_id = host.default_input_device().and_then(|d| d.id().ok()).map(|id| id.to_string());
    host.input_devices()
        .map(|devices| {
            devices
                .filter_map(|d| {
                    let id = d.id().ok()?.to_string();
                    let cfg = d.default_input_config().ok()?;
                    let name = d.description().map(|desc| desc.name().to_owned()).unwrap_or_else(|_| id.clone());
                    Some(InputDeviceInfo {
                        is_default: Some(&id) == default_id.as_ref(),
                        id,
                        name,
                        channels: cfg.channels(),
                        sample_rate: cfg.sample_rate(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Live controls shared between the UI and one input callback.
#[derive(Debug)]
pub struct InputControls {
    /// Peak per hardware channel since last read.
    pub meters: [AtomicF32; MAX_INPUT_CHANNELS],
    /// First hardware channel routed to the monitor/recorder (0-based).
    first_channel: AtomicU32,
    /// Route `first_channel` and the next one as a stereo pair.
    stereo: AtomicBool,
    /// Feed the recording ring.
    recording: AtomicBool,
    /// Set when the recording ring was full (the writer fell behind).
    pub overflowed: AtomicBool,
    /// Capture → callback latency reported by the device, in microseconds.
    pub latency_us: AtomicU64,
}

impl Default for InputControls {
    fn default() -> Self {
        Self {
            meters: std::array::from_fn(|_| AtomicF32::new(0.0)),
            first_channel: AtomicU32::new(0),
            stereo: AtomicBool::new(false),
            recording: AtomicBool::new(false),
            overflowed: AtomicBool::new(false),
            latency_us: AtomicU64::new(0),
        }
    }
}

impl InputControls {
    pub fn set_routing(&self, first_channel: u16, stereo: bool) {
        self.first_channel.store(u32::from(first_channel), Ordering::Relaxed);
        self.stereo.store(stereo, Ordering::Relaxed);
    }

    /// Starts or stops feeding the recording ring.
    pub fn set_recording(&self, on: bool) {
        self.recording.store(on, Ordering::Release);
    }

    #[must_use]
    pub fn is_recording(&self) -> bool {
        self.recording.load(Ordering::Acquire)
    }

    /// Peak of the routed channels since the last call (resets them).
    #[must_use]
    pub fn take_routed_level(&self) -> f32 {
        let first = self.first_channel.load(Ordering::Relaxed) as usize;
        let n = if self.stereo.load(Ordering::Relaxed) { 2 } else { 1 };
        let mut peak = 0.0f32;
        for m in self.meters.iter().skip(first).take(n) {
            peak = peak.max(m.take());
        }
        peak
    }
}

/// Consumer side of an input's recording rings (owned by the take writer
/// while recording).
pub struct RecordRx {
    /// Interleaved stereo of the routed channels.
    pub samples: rtrb::Consumer<f32>,
    pub markers: rtrb::Consumer<Marker>,
}

impl std::fmt::Debug for RecordRx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecordRx").finish_non_exhaustive()
    }
}

/// Producer side, inside the input callback.
struct RecordTx {
    samples: rtrb::Producer<f32>,
    markers: rtrb::Producer<Marker>,
    /// Song clock to stamp markers with.
    clock: Arc<SharedState>,
    /// Frames delivered since recording started.
    frames: u64,
    active: bool,
}

/// Creates connected recording rings for one input.
fn record_rings(clock: Arc<SharedState>) -> (RecordTx, RecordRx) {
    let (stx, srx) = rtrb::RingBuffer::new(RECORD_RING_SAMPLES);
    let (mtx, mrx) = rtrb::RingBuffer::new(MARKER_RING);
    (RecordTx { samples: stx, markers: mtx, clock, frames: 0, active: false }, RecordRx { samples: srx, markers: mrx })
}

/// Real-time part of an input: meters + routing into the monitor ring and,
/// while recording, into the recording ring.
pub struct InputTap {
    controls: Arc<InputControls>,
    monitor: Option<rtrb::Producer<f32>>,
    record: Option<RecordTx>,
}

impl std::fmt::Debug for InputTap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputTap").field("monitor", &self.monitor.is_some()).finish_non_exhaustive()
    }
}

impl InputTap {
    #[must_use]
    pub fn new(controls: Arc<InputControls>, monitor: Option<rtrb::Producer<f32>>) -> Self {
        Self { controls, monitor, record: None }
    }

    /// Adds recording rings stamped with `clock`'s song position.
    #[must_use]
    pub fn with_recorder(mut self, clock: Arc<SharedState>) -> (Self, RecordRx) {
        let (tx, rx) = record_rings(clock);
        self.record = Some(tx);
        (self, rx)
    }

    /// Processes one interleaved input buffer (any sample format).
    pub fn process<T: Sample>(&mut self, data: &[T], channels: usize)
    where
        f32: FromSample<T>,
    {
        let channels = channels.max(1);
        let first = (self.controls.first_channel.load(Ordering::Relaxed) as usize).min(channels - 1);
        let stereo = self.controls.stereo.load(Ordering::Relaxed) && first + 1 < channels;
        let mut peaks = [0.0f32; MAX_INPUT_CHANNELS];
        let recording = self.controls.recording.load(Ordering::Acquire);
        if let Some(rec) = self.record.as_mut()
            && recording != rec.active
        {
            rec.active = recording;
            rec.frames = 0;
        }
        for frame in data.chunks_exact(channels) {
            for (p, s) in peaks.iter_mut().zip(frame) {
                *p = p.max(f32::from_sample(*s).abs());
            }
            if let Some(ring) = self.monitor.as_mut() {
                let l = f32::from_sample(frame[first]);
                let r = if stereo { f32::from_sample(frame[first + 1]) } else { l };
                // A full ring means the output is not consuming: drop, never block.
                if ring.slots() >= 2 {
                    let _ = ring.push(l);
                    let _ = ring.push(r);
                }
            }
            if recording && let Some(rec) = self.record.as_mut() {
                let l = f32::from_sample(frame[first]);
                let r = if stereo { f32::from_sample(frame[first + 1]) } else { l };
                if rec.samples.slots() >= 2 {
                    let _ = rec.samples.push(l);
                    let _ = rec.samples.push(r);
                    rec.frames += 1;
                } else {
                    self.controls.overflowed.store(true, Ordering::Relaxed);
                }
            }
        }
        if recording && let Some(rec) = self.record.as_mut() {
            let marker = Marker { input_frames: rec.frames, song_pos: rec.clock.position() };
            if rec.markers.push(marker).is_err() {
                self.controls.overflowed.store(true, Ordering::Relaxed);
            }
        }
        for (m, p) in self.controls.meters.iter().zip(peaks).take(channels) {
            m.fetch_max(p);
        }
    }
}

/// A running input stream.
pub struct InputStream {
    pub info: InputDeviceInfo,
    pub controls: Arc<InputControls>,
    /// Recording rings (present when started with a song clock).
    pub recorder: Option<Arc<Mutex<RecordRx>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for InputStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputStream").field("info", &self.info).finish_non_exhaustive()
    }
}

impl InputStream {
    /// Opens input `device_id` and starts metering. `monitor` receives the
    /// routed stereo signal (pass one only when sample rates match). With a
    /// song `clock`, the input can also record.
    ///
    /// # Errors
    /// Unknown device or stream errors.
    pub fn start(
        device_id: &str,
        monitor: Option<rtrb::Producer<f32>>,
        clock: Option<Arc<SharedState>>,
    ) -> Result<Self, EngineError> {
        let controls = Arc::new(InputControls::default());
        let tap = InputTap::new(Arc::clone(&controls), monitor);
        let (tap, recorder) = match clock {
            Some(clock) => {
                let (tap, rx) = tap.with_recorder(clock);
                (tap, Some(Arc::new(Mutex::new(rx))))
            }
            None => (tap, None),
        };
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel::<Result<InputDeviceInfo, EngineError>>();
        let id = device_id.to_owned();
        let stop_t = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("joshua-input".into())
            .spawn(move || match open(&id, tap) {
                Ok((stream, info)) => {
                    if ready_tx.send(Ok(info)).is_err() {
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
        let info = ready_rx.recv().map_err(|_| EngineError::ThreadDied)??;
        Ok(Self { info, controls, recorder, stop, thread: Some(thread) })
    }
}

impl Drop for InputStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            t.thread().unpark();
            let _ = t.join();
        }
    }
}

fn open(device_id: &str, tap: InputTap) -> Result<(cpal::Stream, InputDeviceInfo), EngineError> {
    let host = cpal::default_host();
    let parsed = device_id.parse().map_err(|_| EngineError::NoDevice)?;
    let device = host.device_by_id(&parsed).ok_or(EngineError::NoDevice)?;
    let name = device.description().map(|d| d.name().to_owned()).unwrap_or_else(|_| device_id.to_owned());
    let supported = device.default_input_config().map_err(|e| EngineError::Device(e.to_string()))?;
    let format = supported.sample_format();
    let config: StreamConfig = supported.into();
    let info = InputDeviceInfo {
        id: device_id.to_owned(),
        name,
        channels: config.channels,
        sample_rate: config.sample_rate,
        is_default: false,
    };
    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, config, tap)?,
        SampleFormat::I16 => build::<i16>(&device, config, tap)?,
        SampleFormat::I32 => build::<i32>(&device, config, tap)?,
        SampleFormat::U16 => build::<u16>(&device, config, tap)?,
        SampleFormat::F64 => build::<f64>(&device, config, tap)?,
        other => return Err(EngineError::UnsupportedFormat(other.to_string())),
    };
    stream.play().map_err(|e| EngineError::Device(e.to_string()))?;
    Ok((stream, info))
}

fn build<T: SizedSample + 'static>(
    device: &cpal::Device,
    config: StreamConfig,
    mut tap: InputTap,
) -> Result<cpal::Stream, EngineError>
where
    f32: FromSample<T>,
{
    let channels = usize::from(config.channels);
    device
        .build_input_stream(
            config,
            move |data: &[T], info: &cpal::InputCallbackInfo| {
                let ts = info.timestamp();
                if let Some(lat) = ts.callback.checked_duration_since(ts.capture) {
                    tap.controls.latency_us.store(u64::try_from(lat.as_micros()).unwrap_or(0), Ordering::Relaxed);
                }
                tap.process(data, channels);
            },
            |_err| {},
            None,
        )
        .map_err(|e| EngineError::Device(e.to_string()))
}

/// Whether an input and the output are the same physical interface.
///
/// Such an interface already sends its inputs to its own outputs with zero
/// latency (direct monitoring, and often a loopback switch). Playing the
/// input again through the app would double it ~20 ms late (a hollow,
/// "robotic" comb-filter sound) or, with loopback on, feed back on itself.
/// Matches the backend id, or the name; Windows decorates endpoint names as
/// "Microphone (Device)" / "Speakers (Device)", so the bracketed part counts.
#[must_use]
pub fn same_interface(input_id: &str, input_name: &str, output_id: Option<&str>, output_name: &str) -> bool {
    if output_id == Some(input_id) {
        return true;
    }
    let core = |name: &str| -> String {
        let inner = match (name.rfind('('), name.rfind(')')) {
            (Some(a), Some(b)) if b > a + 1 => &name[a + 1..b],
            _ => name,
        };
        inner.chars().filter(char::is_ascii_alphanumeric).flat_map(|c| c.to_lowercase()).collect()
    };
    let (a, b) = (core(input_name), core(output_name));
    a.len() >= 3 && a == b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_the_output_interface_among_inputs() {
        let ag06 = "coreaudio:AppleUSBAudioEngine:Yamaha Corporation:Yamaha AG06MK2:1131000:1,2";
        assert!(same_interface(ag06, "Yamaha AG06MK2", Some(ag06), "Yamaha AG06MK2"));
        assert!(!same_interface("coreaudio:spd", "SPD-SX PRO", Some(ag06), "Yamaha AG06MK2"));
        assert!(same_interface(
            "wasapi:{in}",
            "Microphone (Yamaha AG06MK2)",
            Some("wasapi:{out}"),
            "Speakers (Yamaha AG06MK2)"
        ));
        assert!(!same_interface(
            "wasapi:{in}",
            "Microphone (Realtek Audio)",
            Some("wasapi:{out}"),
            "Speakers (Yamaha AG06MK2)"
        ));
        assert!(!same_interface("x", "In", None, "In"), "names too short to trust");
        assert!(!same_interface("mic", "MacBook Pro Microphone", None, "MacBook Pro Speakers"));
    }

    #[test]
    fn meters_and_routes_selected_channels() {
        let controls = Arc::new(InputControls::default());
        let (tx, mut rx) = rtrb::RingBuffer::new(64);
        let mut tap = InputTap::new(Arc::clone(&controls), Some(tx));
        controls.set_routing(1, true);
        // 3 channels: ch0 = 0.1, ch1 = 0.5, ch2 = -0.8
        tap.process(&[0.1f32, 0.5, -0.8, 0.1, 0.5, -0.8], 3);
        assert!((controls.meters[2].load() - 0.8).abs() < 1e-6);
        let routed: Vec<f32> = std::iter::from_fn(|| rx.pop().ok()).collect();
        assert_eq!(routed, vec![0.5, -0.8, 0.5, -0.8], "channels 2+3 as a stereo pair");
        assert!((controls.take_routed_level() - 0.8).abs() < 1e-6);
        assert_eq!(controls.take_routed_level(), 0.0, "meters reset after reading");
    }

    #[test]
    fn mono_routing_duplicates_and_bad_channels_are_clamped() {
        let controls = Arc::new(InputControls::default());
        let (tx, mut rx) = rtrb::RingBuffer::new(8);
        let mut tap = InputTap::new(Arc::clone(&controls), Some(tx));
        controls.set_routing(40, true); // beyond a 2-channel device
        tap.process(&[0.2f32, 0.4], 2);
        let routed: Vec<f32> = std::iter::from_fn(|| rx.pop().ok()).collect();
        assert_eq!(routed, vec![0.4, 0.4], "clamped to last channel, mono");
    }

    #[test]
    fn full_monitor_ring_never_blocks() {
        let controls = Arc::new(InputControls::default());
        let (tx, _rx) = rtrb::RingBuffer::new(4);
        let mut tap = InputTap::new(controls, Some(tx));
        tap.process(&vec![0.5f32; 2_000], 2); // far more than the ring holds
    }

    #[test]
    fn records_routed_audio_with_song_position_markers_only_while_armed() {
        let controls = Arc::new(InputControls::default());
        controls.set_routing(2, true);
        let clock = Arc::new(SharedState::default());
        let (mut tap, rx) = InputTap::new(Arc::clone(&controls), None).with_recorder(Arc::clone(&clock));
        let mut rx = rx;
        // 4 hardware channels; channels 3+4 carry (0.25, -0.75).
        let buffer: Vec<f32> = (0..256).flat_map(|_| [0.9, 0.9, 0.25, -0.75]).collect();
        tap.process(&buffer, 4);
        assert_eq!(rx.samples.slots(), 0, "nothing recorded before arming");

        controls.set_recording(true);
        clock.seek(1_000);
        tap.process(&buffer, 4);
        clock.seek(1_256);
        tap.process(&buffer, 4);
        assert_eq!(rx.samples.slots(), 512 * 2);
        let first = (rx.samples.pop().unwrap(), rx.samples.pop().unwrap());
        assert_eq!(first, (0.25, -0.75));
        let m1 = rx.markers.pop().unwrap();
        let m2 = rx.markers.pop().unwrap();
        assert_eq!((m1.input_frames, m1.song_pos), (256, 1_000));
        assert_eq!((m2.input_frames, m2.song_pos), (512, 1_256));

        // Re-arming starts counting from zero again.
        controls.set_recording(false);
        tap.process(&buffer, 4);
        controls.set_recording(true);
        tap.process(&buffer, 4);
        while let Ok(m) = rx.markers.pop() {
            assert_eq!(m.input_frames, 256);
        }
    }

    #[test]
    fn full_recording_ring_flags_overflow_instead_of_blocking() {
        let controls = Arc::new(InputControls::default());
        let (mut tap, _rx) = InputTap::new(Arc::clone(&controls), None).with_recorder(Arc::new(SharedState::default()));
        controls.set_recording(true);
        let big = vec![0.1f32; RECORD_RING_SAMPLES + 1_000];
        tap.process(&big, 1);
        assert!(controls.overflowed.load(Ordering::Relaxed));
    }
}
