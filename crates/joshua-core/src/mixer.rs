//! Real-time mixing core.
//!
//! [`Mixer::render`] runs on the audio thread, so it follows real-time rules:
//! no allocation, no locks, no I/O, no panics on any input. The UI talks to it
//! exclusively through the lock-free atomics in [`SharedState`]; sessions are
//! swapped by the engine through a wait-free queue.

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use crate::audio::{StereoBuffer, db_to_gain};
use crate::metronome::{ClickTrack, CountIn, MAX_COUNT_IN_CLICKS, mix_sound};
use crate::room::Room;
use crate::tone::{TONE_BANDS, ToneEq};

/// Maximum number of tracks (stems + recorded takes) in a session.
pub const MAX_TRACKS: usize = 16;
/// Index of the metronome inside [`SharedState::channels`].
pub const METRONOME: usize = MAX_TRACKS;
const NO_SEEK: i64 = i64::MIN;

/// Lock-free `f32` cell.
#[derive(Debug, Default)]
pub struct AtomicF32(AtomicU32);

impl AtomicF32 {
    #[must_use]
    pub fn new(v: f32) -> Self {
        Self(AtomicU32::new(v.to_bits()))
    }

    #[must_use]
    pub fn load(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }

    pub fn store(&self, v: f32) {
        self.0.store(v.to_bits(), Ordering::Relaxed);
    }

    /// Stores `max(current, v)`.
    pub fn fetch_max(&self, v: f32) {
        let mut cur = self.0.load(Ordering::Relaxed);
        while f32::from_bits(cur) < v {
            match self.0.compare_exchange_weak(cur, v.to_bits(), Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(actual) => cur = actual,
            }
        }
    }

    /// Reads and resets to zero (meters).
    #[must_use]
    pub fn take(&self) -> f32 {
        f32::from_bits(self.0.swap(0, Ordering::Relaxed))
    }
}

/// Per-channel controls written by the UI, read by the audio thread.
#[derive(Debug)]
pub struct ChannelControl {
    gain: AtomicF32,
    pan: AtomicF32,
    mute: AtomicBool,
    solo: AtomicBool,
    /// Post-fader send into the shared [`Room`] (recorded takes), `0..=1`.
    send: AtomicF32,
    /// Tone equaliser gains (dB per band, recorded takes) and a counter the
    /// audio thread watches to rebuild its filters only when they change.
    tone: [AtomicF32; TONE_BANDS],
    tone_version: AtomicU32,
    /// Peak level since the UI last read it.
    pub meter: AtomicF32,
}

impl Default for ChannelControl {
    fn default() -> Self {
        Self {
            gain: AtomicF32::new(1.0),
            pan: AtomicF32::new(0.0),
            mute: AtomicBool::new(false),
            solo: AtomicBool::new(false),
            send: AtomicF32::new(0.0),
            tone: std::array::from_fn(|_| AtomicF32::new(0.0)),
            tone_version: AtomicU32::new(0),
            meter: AtomicF32::new(0.0),
        }
    }
}

impl ChannelControl {
    /// Sets gain in dB, clamped to `-96..=+12`.
    pub fn set_gain_db(&self, db: f32) {
        let db = if db.is_finite() { db.clamp(-96.0, 12.0) } else { 0.0 };
        self.gain.store(db_to_gain(db));
    }

    /// Sets stereo balance, clamped to `-1..=1`.
    pub fn set_pan(&self, pan: f32) {
        self.pan.store(if pan.is_finite() { pan.clamp(-1.0, 1.0) } else { 0.0 });
    }

    pub fn set_mute(&self, mute: bool) {
        self.mute.store(mute, Ordering::Relaxed);
    }

    pub fn set_solo(&self, solo: bool) {
        self.solo.store(solo, Ordering::Relaxed);
    }

    #[must_use]
    pub fn is_solo(&self) -> bool {
        self.solo.load(Ordering::Relaxed)
    }

    /// Sets how much of the channel goes into the shared room, `0..=1`.
    /// Only recorded takes use it.
    pub fn set_send(&self, amount: f32) {
        self.send.store(if amount.is_finite() { amount.clamp(0.0, 1.0) } else { 0.0 });
    }

    #[must_use]
    pub fn send(&self) -> f32 {
        self.send.load()
    }

    /// Sets the tone equaliser (dB per band; all zero = off).
    pub fn set_tone(&self, gains_db: &[f32; TONE_BANDS]) {
        for (cell, g) in self.tone.iter().zip(gains_db) {
            cell.store(if g.is_finite() { *g } else { 0.0 });
        }
        self.tone_version.fetch_add(1, Ordering::Release);
    }

    fn tone(&self) -> [f32; TONE_BANDS] {
        std::array::from_fn(|i| self.tone[i].load())
    }

    /// Target `(left, right)` gains.
    fn target(&self, silenced: bool) -> [f32; 2] {
        if silenced || self.mute.load(Ordering::Relaxed) {
            return [0.0, 0.0];
        }
        let g = self.gain.load();
        let pan = self.pan.load();
        [g * (1.0 - pan).min(1.0), g * (1.0 + pan).min(1.0)]
    }
}

/// State shared between UI/control threads and the audio thread.
#[derive(Debug)]
pub struct SharedState {
    /// `0..MAX_TRACKS` are tracks (stems first, then recorded takes),
    /// [`METRONOME`] is the click.
    pub channels: [ChannelControl; MAX_TRACKS + 1],
    /// Group bus of all song stems (the collapsed "Song" strip). Recorded
    /// takes have their own channels outside this bus.
    pub song_bus: ChannelControl,
    pub master_gain: AtomicF32,
    /// Soft safety clipper on the output (on while playing; exports turn it
    /// off and master the mix instead, see [`crate::export::render_mastered`]).
    pub safety_clip: AtomicBool,
    pub master_meter: [AtomicF32; 2],
    playing: AtomicBool,
    position: AtomicI64,
    seek_request: AtomicI64,
    loop_enabled: AtomicBool,
    loop_start: AtomicI64,
    loop_end: AtomicI64,
    /// Pre-roll frames left before the music (re)starts.
    count_in_remaining: AtomicI64,
    /// Count-in clicks still to sound (also during the first music frames).
    count_in_active: AtomicBool,
    count_in_clicks: [AtomicI64; MAX_COUNT_IN_CLICKS],
    count_in_accents: AtomicU64,
    count_in_len: AtomicUsize,
    /// Selected click voice (`ClickSound::index`).
    click_voice: AtomicU32,
    /// Set when playback reached the end of the song (playlist advance).
    ended: AtomicBool,
    /// Monitoring level of each live input (0 = not heard).
    pub monitor_gain: [AtomicF32; MAX_MONITORS],
}

/// Live inputs that can be monitored at once.
pub const MAX_MONITORS: usize = 8;

impl Default for SharedState {
    fn default() -> Self {
        Self {
            channels: std::array::from_fn(|_| ChannelControl::default()),
            song_bus: ChannelControl::default(),
            master_gain: AtomicF32::new(1.0),
            safety_clip: AtomicBool::new(true),
            master_meter: [AtomicF32::new(0.0), AtomicF32::new(0.0)],
            playing: AtomicBool::new(false),
            position: AtomicI64::new(0),
            seek_request: AtomicI64::new(NO_SEEK),
            loop_enabled: AtomicBool::new(false),
            loop_start: AtomicI64::new(0),
            loop_end: AtomicI64::new(0),
            count_in_remaining: AtomicI64::new(0),
            count_in_active: AtomicBool::new(false),
            count_in_clicks: std::array::from_fn(|_| AtomicI64::new(0)),
            count_in_accents: AtomicU64::new(0),
            count_in_len: AtomicUsize::new(0),
            click_voice: AtomicU32::new(0),
            ended: AtomicBool::new(false),
            monitor_gain: std::array::from_fn(|_| AtomicF32::new(0.0)),
        }
    }
}

impl SharedState {
    pub fn play(&self) {
        self.playing.store(true, Ordering::Release);
    }

    pub fn pause(&self) {
        self.playing.store(false, Ordering::Release);
        self.count_in_remaining.store(0, Ordering::Relaxed);
        self.count_in_active.store(false, Ordering::Relaxed);
    }

    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Acquire)
    }

    /// Current playhead in session frames.
    #[must_use]
    pub fn position(&self) -> i64 {
        self.position.load(Ordering::Relaxed)
    }

    /// Requests a seek; applied by the audio thread at the next block.
    pub fn seek(&self, frame: i64) {
        self.seek_request.store(frame.max(0), Ordering::Release);
        self.position.store(frame.max(0), Ordering::Relaxed);
    }

    /// Enables an A–B loop (`end > start`), or disables it.
    pub fn set_loop(&self, region: Option<(i64, i64)>) {
        match region {
            Some((a, b)) if b > a && a >= 0 => {
                self.loop_start.store(a, Ordering::Relaxed);
                self.loop_end.store(b, Ordering::Relaxed);
                self.loop_enabled.store(true, Ordering::Release);
            }
            _ => self.loop_enabled.store(false, Ordering::Release),
        }
    }

    /// Arms a count-in planned by [`crate::metronome::plan_count_in`]:
    /// the pre-roll plays only clicks, then the music starts on the grid.
    pub fn arm_count_in(&self, plan: &CountIn) {
        self.count_in_active.store(false, Ordering::Release);
        let n = plan.clicks.len().min(MAX_COUNT_IN_CLICKS);
        let mut accents = 0u64;
        for (i, (frame, accent)) in plan.clicks.iter().take(n).enumerate() {
            self.count_in_clicks[i].store(*frame, Ordering::Relaxed);
            if *accent {
                accents |= 1 << i;
            }
        }
        self.count_in_accents.store(accents, Ordering::Relaxed);
        self.count_in_len.store(n, Ordering::Relaxed);
        self.count_in_remaining.store(plan.preroll.max(0), Ordering::Relaxed);
        self.count_in_active.store(n > 0, Ordering::Release);
    }

    /// Returns (and clears) whether the song played to its end.
    #[must_use]
    pub fn take_ended(&self) -> bool {
        self.ended.swap(false, Ordering::AcqRel)
    }

    /// Selects the click voice (`ClickSound::index`).
    pub fn set_click_voice(&self, index: usize) {
        self.click_voice.store(u32::try_from(index).unwrap_or(0), Ordering::Relaxed);
    }

    /// `true` while a count-in is running.
    #[must_use]
    pub fn counting_in(&self) -> bool {
        self.count_in_remaining.load(Ordering::Relaxed) > 0
    }

    fn loop_region(&self) -> Option<(i64, i64)> {
        if !self.loop_enabled.load(Ordering::Acquire) {
            return None;
        }
        let (a, b) = (self.loop_start.load(Ordering::Relaxed), self.loop_end.load(Ordering::Relaxed));
        (b > a).then_some((a, b))
    }
}

/// Audio material of the currently loaded song, at the device sample rate.
#[derive(Debug)]
pub struct Session {
    pub sample_rate: u32,
    /// Stems first (`..stems`), then recorded takes.
    pub tracks: Vec<StereoBuffer>,
    /// How many of `tracks` are song stems (routed through the song bus).
    pub stems: usize,
    pub click: Box<ClickTrack>,
    pub length: i64,
}

impl Session {
    /// A session of song stems only. Truncates to [`MAX_TRACKS`].
    #[must_use]
    pub fn new(sample_rate: u32, tracks: Vec<StereoBuffer>, click: ClickTrack) -> Self {
        Self::with_takes(sample_rate, tracks, Vec::new(), click)
    }

    /// Stems plus recorded takes (takes are never longer than the song: they
    /// are truncated to the longest stem). Truncates to [`MAX_TRACKS`].
    #[must_use]
    pub fn with_takes(
        sample_rate: u32,
        mut stems: Vec<StereoBuffer>,
        takes: Vec<StereoBuffer>,
        click: ClickTrack,
    ) -> Self {
        stems.truncate(MAX_TRACKS);
        let length = stems.iter().map(StereoBuffer::frames).max().unwrap_or(0);
        let n_stems = stems.len();
        let room = MAX_TRACKS - n_stems;
        stems.extend(takes.into_iter().take(room));
        Self {
            sample_rate,
            tracks: stems,
            stems: n_stems,
            click: Box::new(click),
            length: i64::try_from(length).unwrap_or(i64::MAX),
        }
    }
}

/// Frames rendered per pass (longer device blocks are split): the size of
/// the preallocated room send buffers.
const BLOCK: usize = 2_048;

/// Audio-thread mixer. Owns the session; smooths gain changes. Build it off
/// the audio thread (it allocates the room); rendering never allocates.
#[derive(Debug)]
pub struct Mixer {
    session: Option<Box<Session>>,
    gains: [[f32; 2]; MAX_TRACKS + 1],
    /// Room send level of each track at the end of the last block.
    sends: [f32; MAX_TRACKS],
    room: Room,
    send_l: Vec<f32>,
    send_r: Vec<f32>,
    /// Tone equaliser of each track and the control version it was built from.
    tone: [ToneEq; MAX_TRACKS],
    tone_seen: [u32; MAX_TRACKS],
}

impl Default for Mixer {
    fn default() -> Self {
        Self {
            session: None,
            gains: [[0.0; 2]; MAX_TRACKS + 1],
            sends: [0.0; MAX_TRACKS],
            room: Room::new(),
            send_l: vec![0.0; BLOCK],
            send_r: vec![0.0; BLOCK],
            tone: [ToneEq::default(); MAX_TRACKS],
            tone_seen: [u32::MAX; MAX_TRACKS],
        }
    }
}

impl Mixer {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Installs a new session and returns the previous one so it can be
    /// dropped *off* the audio thread.
    pub fn swap_session(&mut self, session: Option<Box<Session>>) -> Option<Box<Session>> {
        self.gains = [[0.0; 2]; MAX_TRACKS + 1];
        self.sends = [0.0; MAX_TRACKS];
        // New tracks, maybe a new rate: rebuild every tone filter.
        self.tone = [ToneEq::default(); MAX_TRACKS];
        self.tone_seen = [u32::MAX; MAX_TRACKS];
        if let Some(s) = session.as_deref() {
            if s.sample_rate != self.room.rate() {
                self.room.tune(s.sample_rate);
            }
        }
        self.room.clear();
        std::mem::replace(&mut self.session, session)
    }

    /// Replaces the click track (tempo edited by the user). Returns the old
    /// one for off-thread dropping, or gives the new one back if no session.
    pub fn swap_click(&mut self, click: Box<ClickTrack>) -> Box<ClickTrack> {
        match self.session.as_deref_mut() {
            Some(session) => std::mem::replace(&mut session.click, click),
            None => click,
        }
    }

    #[must_use]
    pub fn session(&self) -> Option<&Session> {
        self.session.as_deref()
    }

    /// Fills `out` (interleaved, `channels` wide) with the next block.
    pub fn render(&mut self, shared: &SharedState, out: &mut [f32], channels: usize) {
        let channels = channels.max(1);
        for part in out.chunks_mut(BLOCK * channels) {
            self.render_block(shared, part, channels);
        }
    }

    fn render_block(&mut self, shared: &SharedState, out: &mut [f32], channels: usize) {
        out.fill(0.0);
        let frames = out.len() / channels;
        let Some(session) = self.session.as_deref() else { return };
        if frames == 0 {
            return;
        }

        let seek = shared.seek_request.swap(NO_SEEK, Ordering::Acquire);
        let mut pos = shared.position.load(Ordering::Relaxed);
        if seek != NO_SEEK {
            pos = seek.clamp(0, session.length);
            self.room.clear();
            self.tone.iter_mut().for_each(ToneEq::reset);
        }
        // Takes whose tone changed get their filters rebuilt (no allocation).
        for i in session.stems..session.tracks.len().min(MAX_TRACKS) {
            let ch = &shared.channels[i];
            let version = ch.tone_version.load(Ordering::Acquire);
            if version != self.tone_seen[i] {
                self.tone[i].set(session.sample_rate, &ch.tone());
                self.tone_seen[i] = version;
            }
        }
        if !shared.is_playing() {
            shared.position.store(pos, Ordering::Relaxed);
            // A paused song has no room tail (nor filter memory) to resume into.
            self.room.clear();
            self.tone.iter_mut().for_each(ToneEq::reset);
            return;
        }

        // Target gains (solo applies to stems only; the click is never soloed out).
        let any_solo = shared.channels[..session.tracks.len()].iter().any(ChannelControl::is_solo);
        let [bus_l, bus_r] = shared.song_bus.target(false);
        let mut targets = [[0.0f32; 2]; MAX_TRACKS + 1];
        for (i, t) in targets.iter_mut().enumerate().take(session.tracks.len()) {
            let ch = &shared.channels[i];
            let [l, r] = ch.target(any_solo && !ch.is_solo());
            // Stems go through the song bus; recorded takes do not.
            *t = if i < session.stems { [l * bus_l, r * bus_r] } else { [l, r] };
        }
        targets[METRONOME] = shared.channels[METRONOME].target(false);
        let start_gains = self.gains;
        let ramp = Ramp { start: &start_gains, end: &targets, frames };
        // Room sends (post-fader) of the recorded takes.
        let mut send_targets = [0.0f32; MAX_TRACKS];
        for (i, t) in send_targets.iter_mut().enumerate().take(session.tracks.len()).skip(session.stems) {
            *t = shared.channels[i].send();
        }
        let start_sends = self.sends;
        let any_send = start_sends.iter().chain(&send_targets).any(|s| *s > 0.0);
        let mut sends = Sends {
            start: &start_sends,
            end: &send_targets,
            frames,
            l: &mut self.send_l[..frames],
            r: &mut self.send_r[..frames],
            active: any_send,
        };
        sends.l.fill(0.0);
        sends.r.fill(0.0);

        let mut done = 0usize;
        while done < frames {
            let remaining_ci = shared.count_in_remaining.load(Ordering::Relaxed);
            if remaining_ci > 0 {
                let n = usize::try_from(remaining_ci).unwrap_or(usize::MAX).min(frames - done);
                // Virtual timeline position: the music is `remaining_ci` frames away.
                render_count_in(shared, session, &ramp, pos - remaining_ci, done, n, out, channels);
                // CAS so a concurrent pause() (which zeroes it) is not overwritten.
                let _ = shared.count_in_remaining.compare_exchange(
                    remaining_ci,
                    remaining_ci - n as i64,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                );
                done += n;
                continue;
            }
            // A loop ending past the audio (bar math vs. resampled length)
            // wraps at the real end instead of stopping playback.
            let loop_region = shared.loop_region().map(|(a, b)| (a, b.min(session.length))).filter(|(a, b)| a < b);
            if let Some((a, b)) = loop_region {
                if pos >= b {
                    pos = a;
                }
            }
            // With a loop active, playback before B runs into the loop and wraps.
            let seg_end = match loop_region {
                Some((_, b)) if pos < b => b.min(session.length),
                _ => session.length,
            };
            if pos >= session.length {
                shared.playing.store(false, Ordering::Release);
                shared.ended.store(true, Ordering::Release);
                pos = session.length;
                break;
            }
            let n = usize::try_from(seg_end - pos).unwrap_or(0).min(frames - done);
            if n == 0 {
                pos = loop_region.map_or(session.length, |(a, _)| a);
                continue;
            }
            render_segment(shared, session, &ramp, &mut sends, &mut self.tone, pos, done, n, out, channels);
            pos += n as i64;
            done += n;
        }
        self.gains = targets;
        self.sends = send_targets;
        shared.position.store(pos, Ordering::Relaxed);

        // The shared room, added under the dry signal.
        let (send_l, send_r) = (&mut self.send_l[..frames], &mut self.send_r[..frames]);
        self.room.process(send_l, send_r);
        for (f, (l, r)) in send_l.iter().zip(send_r.iter()).enumerate() {
            if *l != 0.0 || *r != 0.0 {
                write_frame(out, channels, f, *l, *r);
            }
        }

        // Master gain, soft safety limiter and meters.
        let master = shared.master_gain.load();
        let clip = shared.safety_clip.load(Ordering::Relaxed);
        let mut peak = [0.0f32; 2];
        for frame in out.chunks_exact_mut(channels) {
            for (c, s) in frame.iter_mut().enumerate() {
                let x = *s * master;
                *s = if clip {
                    crate::audio::soft_clip(x)
                } else if x.is_finite() {
                    x
                } else {
                    0.0
                };
                if c < 2 {
                    peak[c] = peak[c].max(s.abs());
                }
            }
        }
        shared.master_meter[0].fetch_max(peak[0]);
        shared.master_meter[1].fetch_max(if channels > 1 { peak[1] } else { peak[0] });
    }
}

/// Linear per-block gain ramp (avoids zipper noise on fader moves).
struct Ramp<'a> {
    start: &'a [[f32; 2]; MAX_TRACKS + 1],
    end: &'a [[f32; 2]; MAX_TRACKS + 1],
    frames: usize,
}

impl Ramp<'_> {
    #[inline]
    fn at(&self, ch: usize, side: usize, frame: usize) -> f32 {
        let t = frame as f32 / self.frames as f32;
        self.start[ch][side] + (self.end[ch][side] - self.start[ch][side]) * t
    }
}

/// Per-block room send ramp and the buffers the takes are sent into.
struct Sends<'a> {
    start: &'a [f32; MAX_TRACKS],
    end: &'a [f32; MAX_TRACKS],
    frames: usize,
    l: &'a mut [f32],
    r: &'a mut [f32],
    /// Some take sends (or was sending) into the room.
    active: bool,
}

impl Sends<'_> {
    #[inline]
    fn at(&self, ch: usize, frame: usize) -> f32 {
        let t = frame as f32 / self.frames as f32;
        self.start[ch] + (self.end[ch] - self.start[ch]) * t
    }
}

#[inline]
fn write_frame(out: &mut [f32], channels: usize, frame: usize, l: f32, r: f32) {
    let base = frame * channels;
    if channels == 1 {
        out[base] += 0.5 * (l + r);
    } else {
        out[base] += l;
        out[base + 1] += r;
    }
}

#[allow(clippy::too_many_arguments)]
fn render_segment(
    shared: &SharedState,
    session: &Session,
    ramp: &Ramp<'_>,
    sends: &mut Sends<'_>,
    tone: &mut [ToneEq; MAX_TRACKS],
    pos: i64,
    offset: usize,
    n: usize,
    out: &mut [f32],
    channels: usize,
) {
    let Ok(start) = usize::try_from(pos) else { return };
    for (i, track) in session.tracks.iter().enumerate() {
        let (left, right) = (track.left(), track.right());
        let to_room = sends.active && i >= session.stems && (sends.start[i] > 0.0 || sends.end[i] > 0.0);
        let eq = tone.get_mut(i).filter(|e| e.is_active());
        let mut eq = eq;
        let mut peak = 0.0f32;
        for f in 0..n {
            let idx = start + f;
            let (Some(&l), Some(&r)) = (left.get(idx), right.get(idx)) else { break };
            let (l, r) = match eq.as_deref_mut() {
                Some(e) => e.process(l, r),
                None => (l, r),
            };
            let (gl, gr) = (ramp.at(i, 0, offset + f), ramp.at(i, 1, offset + f));
            let (l, r) = (l * gl, r * gr);
            peak = peak.max(l.abs()).max(r.abs());
            write_frame(out, channels, offset + f, l, r);
            if to_room {
                let s = sends.at(i, offset + f);
                sends.l[offset + f] += l * s;
                sends.r[offset + f] += r * s;
            }
        }
        shared.channels[i].meter.fetch_max(peak);
        if i < session.stems {
            shared.song_bus.meter.fetch_max(peak);
        }
    }
    // Click: render into a small stack buffer chunk by chunk (no allocation).
    let gains = ramp.end[METRONOME];
    let voice = shared.click_voice.load(Ordering::Relaxed) as usize;
    if gains[0] > 0.0 || gains[1] > 0.0 {
        render_click_chunks(n, out, channels, offset, |l, r, at| {
            session.click.render_add(pos + at as i64, l, r, gains, voice);
        });
    }
    // Count-in clicks that fall after the start (intro before the first beat).
    if shared.count_in_active.load(Ordering::Acquire) {
        render_count_in(shared, session, ramp, pos, offset, n, out, channels);
    }
}

/// Renders the armed count-in clicks falling in `[virtual_pos, virtual_pos + n)`.
#[allow(clippy::too_many_arguments)]
fn render_count_in(
    shared: &SharedState,
    session: &Session,
    ramp: &Ramp<'_>,
    virtual_pos: i64,
    offset: usize,
    n: usize,
    out: &mut [f32],
    channels: usize,
) {
    if !shared.count_in_active.load(Ordering::Acquire) {
        return;
    }
    let len = shared.count_in_len.load(Ordering::Relaxed).min(MAX_COUNT_IN_CLICKS);
    let accents = shared.count_in_accents.load(Ordering::Relaxed);
    let voice = session.click.voice(shared.click_voice.load(Ordering::Relaxed) as usize);
    let [gl, gr] = ramp.end[METRONOME];
    // The count-in stays audible even when the metronome lane is muted.
    let (gl, gr) = if gl == 0.0 && gr == 0.0 { (0.8, 0.8) } else { (gl, gr) };
    let tail = voice.accent.len().max(voice.regular.len()) as i64;
    let mut last = i64::MIN;
    render_click_chunks(n, out, channels, offset, |l, r, at| {
        let chunk_start = virtual_pos + at as i64;
        for i in 0..len {
            let frame = shared.count_in_clicks[i].load(Ordering::Relaxed);
            last = last.max(frame);
            mix_sound(voice.get(accents >> i & 1 == 1), frame - chunk_start, l, r, gl, gr);
        }
    });
    if virtual_pos + n as i64 > last + tail {
        shared.count_in_active.store(false, Ordering::Release);
    }
}

/// Calls `f(left, right, frame_offset)` on stack chunks and mixes them into `out`.
fn render_click_chunks(
    n: usize,
    out: &mut [f32],
    channels: usize,
    offset: usize,
    mut f: impl FnMut(&mut [f32], &mut [f32], usize),
) {
    const CHUNK: usize = 256;
    let mut l = [0.0f32; CHUNK];
    let mut r = [0.0f32; CHUNK];
    let mut at = 0;
    while at < n {
        let len = CHUNK.min(n - at);
        l[..len].fill(0.0);
        r[..len].fill(0.0);
        f(&mut l[..len], &mut r[..len], at);
        for i in 0..len {
            write_frame(out, channels, offset + at + i, l[i], r[i]);
        }
        at += len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tempo::BeatAnalysis;

    const SR: u32 = 8_000;

    fn no_click() -> ClickTrack {
        ClickTrack::from_analysis(
            &BeatAnalysis {
                bpm: 120.0,
                beats: vec![],
                beats_per_bar: 4,
                downbeat_phase: 0,
                confidence: 0.0,
                variable_tempo: false,
            },
            SR,
        )
    }

    fn session(tracks: Vec<(f32, f32)>, frames: usize) -> Box<Session> {
        let bufs =
            tracks.into_iter().map(|(l, r)| StereoBuffer::new(SR, vec![l; frames], vec![r; frames]).unwrap()).collect();
        Box::new(Session::new(SR, bufs, no_click()))
    }

    fn render(mixer: &mut Mixer, shared: &SharedState, frames: usize) -> Vec<f32> {
        let mut out = vec![0.0; frames * 2];
        mixer.render(shared, &mut out, 2);
        out
    }

    /// Renders twice so gain ramps settle, returns the second block.
    fn settled(mixer: &mut Mixer, shared: &SharedState) -> Vec<f32> {
        render(mixer, shared, 64);
        render(mixer, shared, 64)
    }

    #[test]
    fn silent_when_stopped_or_empty() {
        let shared = SharedState::default();
        let mut mixer = Mixer::new();
        assert!(render(&mut mixer, &shared, 32).iter().all(|s| *s == 0.0));
        mixer.swap_session(Some(session(vec![(0.5, 0.5)], 1000)));
        assert!(render(&mut mixer, &shared, 32).iter().all(|s| *s == 0.0));
        assert_eq!(shared.position(), 0);
    }

    #[test]
    fn sums_tracks_and_advances_position() {
        let shared = SharedState::default();
        let mut mixer = Mixer::new();
        mixer.swap_session(Some(session(vec![(0.25, 0.1), (0.25, 0.2)], 1000)));
        shared.play();
        let out = settled(&mut mixer, &shared);
        assert!((out[0] - 0.5).abs() < 1e-6 && (out[1] - 0.3).abs() < 1e-6);
        assert_eq!(shared.position(), 128);
    }

    #[test]
    fn mute_solo_and_gain() {
        let shared = SharedState::default();
        let mut mixer = Mixer::new();
        mixer.swap_session(Some(session(vec![(0.2, 0.2), (0.4, 0.4)], 10_000)));
        shared.play();

        shared.channels[0].set_mute(true);
        assert!((settled(&mut mixer, &shared)[0] - 0.4).abs() < 1e-6);

        shared.channels[0].set_mute(false);
        shared.channels[0].set_solo(true);
        assert!((settled(&mut mixer, &shared)[0] - 0.2).abs() < 1e-6);

        shared.channels[0].set_solo(false);
        shared.channels[1].set_gain_db(-6.0206);
        assert!((settled(&mut mixer, &shared)[0] - 0.4).abs() < 1e-3);

        shared.channels[1].set_gain_db(f32::NAN); // hostile input → 0 dB
        assert!((settled(&mut mixer, &shared)[0] - 0.6).abs() < 1e-6);
    }

    #[test]
    fn song_bus_scales_and_mutes_all_stems_but_not_the_click() {
        let shared = SharedState::default();
        let mut mixer = Mixer::new();
        mixer.swap_session(Some(session(vec![(0.2, 0.2), (0.4, 0.4)], 10_000)));
        shared.play();
        shared.song_bus.set_gain_db(-6.0206);
        assert!((settled(&mut mixer, &shared)[0] - 0.3).abs() < 1e-3);
        shared.song_bus.set_mute(true);
        assert!(settled(&mut mixer, &shared).iter().all(|s| s.abs() < 1e-6));
    }

    #[test]
    fn takes_bypass_the_song_bus_but_follow_solo_and_never_outlast_the_song() {
        let shared = SharedState::default();
        let mut mixer = Mixer::new();
        let stem = StereoBuffer::new(SR, vec![0.2; 1_000], vec![0.2; 1_000]).unwrap();
        // A take longer than the song is cut at the song's end.
        let take = StereoBuffer::new(SR, vec![0.4; 5_000], vec![0.4; 5_000]).unwrap();
        let s = Session::with_takes(SR, vec![stem], vec![take], no_click());
        assert_eq!((s.stems, s.tracks.len(), s.length), (1, 2, 1_000));
        mixer.swap_session(Some(Box::new(s)));
        shared.play();
        shared.song_bus.set_mute(true);
        assert!((settled(&mut mixer, &shared)[0] - 0.4).abs() < 1e-6, "take still heard with the song muted");
        assert!(shared.song_bus.meter.take() == 0.0, "takes do not drive the song meter");
        shared.song_bus.set_mute(false);
        shared.channels[0].set_solo(true);
        assert!((settled(&mut mixer, &shared)[0] - 0.2).abs() < 1e-6, "solo on a stem silences the take");
    }

    #[test]
    fn takes_send_into_the_room_and_stems_never_do() {
        let impulse = |at: usize| {
            let mut v = vec![0.0f32; 48_000];
            v[at] = 1.0;
            StereoBuffer::new(SR, v.clone(), v).unwrap()
        };
        let run = |stem_send: f32, take_send: f32| {
            let shared = SharedState::default();
            let mut mixer = Mixer::new();
            let s = Session::with_takes(SR, vec![impulse(5_000)], vec![impulse(6_000)], no_click());
            mixer.swap_session(Some(Box::new(s)));
            shared.channels[0].set_send(stem_send);
            shared.channels[1].set_send(take_send);
            shared.play();
            // Device blocks larger than the internal block are split; the
            // impulses come after the first block, once the gains have settled.
            let mut out = vec![0.0f32; 12_000 * 2];
            mixer.render(&shared, &mut out, 2);
            out
        };
        let dry = run(1.0, 0.0);
        // Stems ignore the send: only the two dry impulses are heard.
        let nz: Vec<_> = dry.iter().enumerate().filter(|(_, s)| **s != 0.0).collect();
        assert_eq!(nz.len(), 4, "{nz:?}");
        let wet = run(0.0, 0.8);
        let tail: f32 = wet[2 * 6_100..].iter().map(|s| s * s).sum();
        assert!(tail > 1e-6, "the take rings in the room");
        assert!(wet.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    }

    #[test]
    fn the_tone_equaliser_shapes_takes_only() {
        // A 1 kHz tone on a stem and on a take, each heard alone in turn.
        let tone = || {
            let v: Vec<f32> =
                (0..48_000).map(|i| 0.3 * (std::f32::consts::TAU * 1_000.0 * i as f32 / SR as f32).sin()).collect();
            StereoBuffer::new(SR, v.clone(), v).unwrap()
        };
        let mut boost = [0.0f32; crate::tone::TONE_BANDS];
        boost[5] = 6.0; // the 1 kHz band
        let run = |mute: usize, eq_on_take: bool| {
            let shared = SharedState::default();
            let mut mixer = Mixer::new();
            mixer.swap_session(Some(Box::new(Session::with_takes(SR, vec![tone()], vec![tone()], no_click()))));
            shared.channels[mute].set_mute(true);
            shared.channels[0].set_tone(&boost); // a stem: ignored
            if eq_on_take {
                shared.channels[1].set_tone(&boost);
            }
            shared.play();
            let mut out = vec![0.0f32; 24_000 * 2];
            mixer.render(&shared, &mut out, 2);
            out[2 * 12_000..].iter().fold(0.0f32, |m, x| m.max(x.abs()))
        };
        let db = |a: f32, b: f32| 20.0 * (a / b).log10();
        let stem_plain = run(1, false);
        assert!(db(run(1, true), stem_plain).abs() < 0.1, "stems are never equalised");
        let take_plain = run(0, false);
        assert!((db(run(0, true), take_plain) - 6.0).abs() < 0.7, "the take's 1 kHz band is lifted");
    }

    #[test]
    fn with_takes_never_exceeds_max_tracks() {
        let buf = || StereoBuffer::new(SR, vec![0.0; 10], vec![0.0; 10]).unwrap();
        let s = Session::with_takes(SR, (0..10).map(|_| buf()).collect(), (0..10).map(|_| buf()).collect(), no_click());
        assert_eq!((s.stems, s.tracks.len()), (10, MAX_TRACKS));
    }

    #[test]
    fn pan_balances_channels() {
        let shared = SharedState::default();
        let mut mixer = Mixer::new();
        mixer.swap_session(Some(session(vec![(0.5, 0.5)], 10_000)));
        shared.play();
        shared.channels[0].set_pan(-1.0);
        let out = settled(&mut mixer, &shared);
        assert!((out[0] - 0.5).abs() < 1e-6 && out[1].abs() < 1e-6);
    }

    #[test]
    fn gain_changes_are_ramped_not_stepped() {
        let shared = SharedState::default();
        let mut mixer = Mixer::new();
        mixer.swap_session(Some(session(vec![(1.0, 1.0)], 10_000)));
        shared.play();
        let first = render(&mut mixer, &shared, 100);
        assert!(first[0] < 0.05 && first[198] > 0.9, "fade-in after load");
        let diffs = first.chunks(2).map(|f| f[0]).collect::<Vec<_>>();
        assert!(diffs.windows(2).all(|w| (w[1] - w[0]).abs() < 0.02));
    }

    #[test]
    fn stops_at_end_and_clamps_seek() {
        let shared = SharedState::default();
        let mut mixer = Mixer::new();
        mixer.swap_session(Some(session(vec![(0.1, 0.1)], 100)));
        shared.play();
        let out = render(&mut mixer, &shared, 300);
        assert!(!shared.is_playing());
        assert!(shared.take_ended(), "end of song is reported once");
        assert!(!shared.take_ended());
        assert_eq!(shared.position(), 100);
        assert!(out[250 * 2..].iter().all(|s| *s == 0.0));
        shared.seek(1_000_000);
        shared.play();
        render(&mut mixer, &shared, 10);
        assert_eq!(shared.position(), 100);
    }

    #[test]
    fn loops_between_a_and_b() {
        let shared = SharedState::default();
        let mut mixer = Mixer::new();
        mixer.swap_session(Some(session(vec![(0.1, 0.1)], 10_000)));
        shared.set_loop(Some((100, 200)));
        shared.seek(150);
        shared.play();
        render(&mut mixer, &shared, 120); // 150..200 then 100..170
        assert_eq!(shared.position(), 170);
        assert!(shared.is_playing());
        shared.set_loop(Some((300, 200))); // invalid → disabled
        render(&mut mixer, &shared, 50);
        assert_eq!(shared.position(), 220);
    }

    #[test]
    fn loop_past_the_end_wraps_instead_of_stopping() {
        let shared = SharedState::default();
        let mut mixer = Mixer::new();
        mixer.swap_session(Some(session(vec![(0.1, 0.1)], 1_000)));
        shared.set_loop(Some((800, 1_050))); // end beyond the audio
        shared.seek(900);
        shared.play();
        render(&mut mixer, &shared, 300);
        assert!(shared.is_playing(), "must keep looping");
        assert!(!shared.take_ended());
        assert!((800..=1_000).contains(&shared.position()));
        render(&mut mixer, &shared, 50);
        assert!((800..1_000).contains(&shared.position()), "wrapped back into the loop");
        assert!(shared.is_playing());
    }

    #[test]
    fn count_in_delays_the_music() {
        let shared = SharedState::default();
        let mut mixer = Mixer::new();
        mixer.swap_session(Some(session(vec![(0.0, 0.0)], 10_000)));
        shared.arm_count_in(&CountIn {
            clicks: vec![(-400, true), (-300, false), (-200, false), (-100, false)],
            preroll: 400,
        });
        shared.play();
        let out = render(&mut mixer, &shared, 450);
        assert_eq!(shared.position(), 50, "music starts after 400 count-in frames");
        assert!(out.iter().any(|s| s.abs() > 0.1), "count-in clicks audible");
        assert!(!shared.counting_in());
    }

    #[test]
    fn count_in_clicks_land_on_exact_frames() {
        // Clicks start from zero amplitude (sine phase 0 + 1 ms fade), so the
        // first audible sample is at most two frames after the click position.
        let onset = |out: &[f32]| out.chunks(2).position(|f| f[0].abs() > 1e-4);
        // Pre-roll of 400 frames, single click at virtual -100 → output frame 300.
        let shared = SharedState::default();
        let mut mixer = Mixer::new();
        mixer.swap_session(Some(session(vec![(0.0, 0.0)], 10_000)));
        shared.arm_count_in(&CountIn { clicks: vec![(-100, true)], preroll: 400 });
        shared.play();
        let at = onset(&render(&mut mixer, &shared, 1_000)).unwrap();
        assert!((300..=302).contains(&at), "onset {at}");

        // Intro case: click after the start position sounds during the music.
        let shared = SharedState::default();
        let mut mixer = Mixer::new();
        mixer.swap_session(Some(session(vec![(0.0, 0.0)], 10_000)));
        shared.arm_count_in(&CountIn { clicks: vec![(50, true)], preroll: 0 });
        shared.play();
        let at = onset(&render(&mut mixer, &shared, 1_000)).unwrap();
        assert!((50..=52).contains(&at), "onset {at}");
    }

    #[test]
    fn output_is_clamped_and_supports_mono_and_multichannel_devices() {
        let shared = SharedState::default();
        let mut mixer = Mixer::new();
        mixer.swap_session(Some(session(vec![(3.0, -3.0)], 10_000)));
        shared.play();
        let mut mono = vec![0.0; 64];
        mixer.render(&shared, &mut mono, 1);
        let mut six = vec![0.0; 64 * 6];
        mixer.render(&shared, &mut six, 6);
        assert!(six.iter().all(|s| s.abs() <= 1.0));
        assert!(six.chunks(6).skip(4).all(|f| f[0] > 0.99 && f[1] < -0.99 && f[2..].iter().all(|s| *s == 0.0)));
        let mut odd = vec![0.0; 7]; // not a whole number of frames
        mixer.render(&shared, &mut odd, 2);
        mixer.render(&shared, &mut [], 0);
    }

    #[test]
    fn meters_report_and_reset() {
        let shared = SharedState::default();
        let mut mixer = Mixer::new();
        mixer.swap_session(Some(session(vec![(0.5, 0.5)], 10_000)));
        shared.play();
        settled(&mut mixer, &shared);
        assert!((shared.channels[0].meter.take() - 0.5).abs() < 1e-6);
        assert_eq!(shared.channels[0].meter.take(), 0.0);
        assert!(shared.master_meter[0].take() > 0.4);
    }
}
