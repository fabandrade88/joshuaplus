//! Reading a live input into the output with clock-drift compensation.
//!
//! An input on another device (e.g. a drum module's USB audio while the
//! output is a separate interface) runs on its own crystal: nominally both
//! are 48 kHz, but one is always a little faster. Reading the ring 1:1 then
//! either underruns (gaps) or piles up (dropped blocks), heard as clicks and
//! a "robotic" stutter. Instead the reader resamples by a ratio within
//! ±0.2 % that a slow controller steers to keep the ring at a constant fill,
//! with 4-point cubic interpolation so the correction is inaudible.
//!
//! Everything here runs on the audio thread: no allocation, no locks.

/// Largest speed correction (±0.2 %, far beyond real crystal drift).
const MAX_CORRECTION: f64 = 0.002;
/// Smoothing of the measured fill level (one-pole, per block).
const FILL_SMOOTHING: f64 = 0.01;
/// Extra frames kept in the ring beyond one output block and one input burst.
const MARGIN_FRAMES: usize = 128;
/// Slow forgetting of the largest input burst seen (per output block).
const BURST_DECAY: f64 = 0.9995;
/// Starting guess for the input burst (a common device buffer size) and the
/// largest burst we adapt to.
const INITIAL_BURST: f64 = 512.0;
const MAX_BURST: f64 = 8_192.0;

/// Audio-thread reader of one monitored input ring (interleaved stereo).
pub struct MonitorReader {
    ring: rtrb::Consumer<f32>,
    /// The two most recently consumed frames (interpolation history).
    hist: [(f32, f32); 2],
    /// Fractional read position relative to the next unread frame.
    pos: f64,
    ratio: f64,
    fill: f64,
    primed: bool,
    /// Frames left in the ring after the previous block.
    left: usize,
    /// Largest input burst seen recently: the input device delivers audio in
    /// chunks, so the ring must hold at least one chunk of slack.
    burst: f64,
}

impl std::fmt::Debug for MonitorReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MonitorReader").field("ratio", &self.ratio).field("primed", &self.primed).finish()
    }
}

impl MonitorReader {
    #[must_use]
    pub fn new(ring: rtrb::Consumer<f32>) -> Self {
        Self {
            ring,
            hist: [(0.0, 0.0); 2],
            pos: 0.0,
            ratio: 1.0,
            fill: 0.0,
            primed: false,
            left: 0,
            burst: INITIAL_BURST,
        }
    }

    /// Gives the ring back (so it can be freed off the audio thread).
    #[must_use]
    pub fn into_ring(self) -> rtrb::Consumer<f32> {
        self.ring
    }

    /// Current resampling ratio (input frames consumed per output frame).
    #[must_use]
    pub fn ratio(&self) -> f64 {
        self.ratio
    }

    /// Adds `frames` of the input, times `gain`, to `out`. The ring is
    /// consumed even when `gain` is 0, so unmuting is never delayed.
    pub fn mix_into(&mut self, out: &mut [f32], channels: usize, gain: f32) {
        let channels = channels.max(1);
        let frames = out.len() / channels;
        if frames == 0 {
            return;
        }
        let avail = self.ring.slots() / 2;
        if self.primed {
            // Only steady-state arrivals measure the burst; a backlog does not.
            let arrived = avail.saturating_sub(self.left) as f64;
            self.burst = (self.burst * BURST_DECAY).max(arrived.min(MAX_BURST)).max(INITIAL_BURST);
        }
        let target = frames + self.burst as usize + MARGIN_FRAMES;

        // Way behind (stalled device, first start after a long gap): jump to
        // the target latency instead of slowly catching up.
        if avail > target * 4 + 4_800 {
            self.skip_frames(avail - target);
            self.reset_control(target);
        }
        let avail = self.ring.slots() / 2;
        if avail < self.left {
            self.left = avail;
        }
        if !self.primed {
            if avail < target {
                self.left = avail;
                return; // still buffering
            }
            self.primed = true;
            self.reset_control(target);
        }

        // Steer the ratio so the fill level settles at the target.
        self.fill += (avail as f64 - self.fill) * FILL_SMOOTHING;
        let error = (self.fill - target as f64) / target as f64;
        self.ratio = 1.0 + (error * MAX_CORRECTION * 2.0).clamp(-MAX_CORRECTION, MAX_CORRECTION);

        let last = self.pos + (frames - 1) as f64 * self.ratio;
        let need = last as usize + 2;
        let (rendered, end) = if avail >= need {
            (frames, self.pos + frames as f64 * self.ratio)
        } else {
            // Underrun: play what there is, then re-buffer.
            self.primed = false;
            let can = ((avail as f64 - 1.0 - self.pos) / self.ratio).floor().max(0.0) as usize;
            let can = can.min(frames);
            (can, self.pos + can as f64 * self.ratio)
        };
        let read = need.min(avail);
        let Ok(chunk) = self.ring.read_chunk(read * 2) else { return };
        let (a, b) = chunk.as_slices();
        let sample = |k: usize| if k < a.len() { a[k] } else { b.get(k - a.len()).copied().unwrap_or(0.0) };
        // Virtual frame sequence: v(-1), v(0) = history; v(k ≥ 1) = ring frame k-1.
        let hist = self.hist;
        let frame = |k: isize| -> (f32, f32) {
            match k {
                -1 => hist[0],
                0 => hist[1],
                _ => {
                    let i = (k as usize - 1) * 2;
                    (sample(i), sample(i + 1))
                }
            }
        };

        for t in 0..rendered {
            let x = self.pos + t as f64 * self.ratio;
            let i = x.floor();
            let f = (x - i) as f32;
            let i = i as isize;
            let (l, r) = cubic(frame(i - 1), frame(i), frame(i + 1), frame(i + 2), f);
            if gain > 0.0 {
                let base = t * channels;
                if channels == 1 {
                    out[base] += 0.5 * (l + r) * gain;
                } else {
                    out[base] += l * gain;
                    out[base + 1] += r * gain;
                }
            }
        }

        let consumed = (end.floor() as usize).min(read);
        if consumed >= 1 {
            self.hist = [frame(consumed as isize - 1), frame(consumed as isize)];
        }
        self.pos = (end - consumed as f64).clamp(0.0, 1.0);
        chunk.commit(consumed * 2);
        self.left = avail - consumed;
    }

    fn skip_frames(&mut self, n: usize) {
        if let Ok(chunk) = self.ring.read_chunk((n * 2).min(self.ring.slots() & !1)) {
            chunk.commit_all();
        }
    }

    fn reset_control(&mut self, target: usize) {
        self.fill = target as f64;
        self.ratio = 1.0;
    }
}

/// 4-point, 3rd-order Hermite interpolation between `p1` and `p2`.
#[inline]
fn cubic(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32), t: f32) -> (f32, f32) {
    let h = |y0: f32, y1: f32, y2: f32, y3: f32| {
        let c1 = 0.5 * (y2 - y0);
        let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
        let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
        ((c3 * t + c2) * t + c1) * t + y1
    };
    (h(p0.0, p1.0, p2.0, p3.0), h(p0.1, p1.1, p2.1, p3.1))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeds a sine produced at `in_rate` while the output consumes at
    /// 48 kHz, in uneven input chunks, and returns the output.
    fn run(in_rate: f64, seconds: f64, block: usize) -> (Vec<f32>, MonitorReader) {
        let (mut tx, rx) = rtrb::RingBuffer::new(48_000);
        let mut reader = MonitorReader::new(rx);
        let mut out_all = Vec::new();
        let mut n_in = 0usize;
        let blocks = (seconds * 48_000.0 / block as f64) as usize;
        for b in 0..blocks {
            // Input arrives in bursts of 512 frames at its own clock.
            let due = (b + 1) as f64 * block as f64 * in_rate / 48_000.0;
            while (n_in as f64) < due {
                for _ in 0..512 {
                    let v = (std::f64::consts::TAU * 440.0 * n_in as f64 / in_rate).sin() as f32 * 0.5;
                    let _ = tx.push(v);
                    let _ = tx.push(-v);
                    n_in += 1;
                }
            }
            let mut out = vec![0.0f32; block * 2];
            reader.mix_into(&mut out, 2, 1.0);
            out_all.extend_from_slice(&out);
        }
        (out_all, reader)
    }

    fn gaps(out: &[f32]) -> usize {
        // After start-up, a continuous sine never has 8 zero frames in a row.
        let start = out.iter().position(|s| *s != 0.0).unwrap_or(out.len());
        out[start..].chunks(2).collect::<Vec<_>>().windows(8).filter(|w| w.iter().all(|f| f[0] == 0.0)).count()
    }

    fn max_step(out: &[f32]) -> f32 {
        let start = out.iter().position(|s| *s != 0.0).unwrap_or(out.len());
        out[start..]
            .chunks(2)
            .map(|f| f[0])
            .collect::<Vec<_>>()
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0, f32::max)
    }

    #[test]
    fn drifting_clocks_play_without_gaps_or_clicks() {
        // ±0.05 % drift: far more than real USB devices.
        for (in_rate, block) in [(47_976.0, 256), (48_000.0, 256), (48_024.0, 256), (47_990.0, 128), (48_010.0, 1_024)]
        {
            let (out, reader) = run(in_rate, 20.0, block);
            assert_eq!(gaps(&out), 0, "gaps at {in_rate} Hz");
            // A 440 Hz sine at amplitude 0.5 moves at most ~0.029 per sample.
            assert!(max_step(&out) < 0.035, "click at {in_rate} Hz: {}", max_step(&out));
            assert!((reader.ratio() - in_rate / 48_000.0).abs() < 0.0005, "ratio {} for {in_rate}", reader.ratio());
        }
    }

    #[test]
    fn channels_stay_in_order_and_mono_downmixes() {
        let (mut tx, rx) = rtrb::RingBuffer::new(48_000);
        let mut reader = MonitorReader::new(rx);
        for _ in 0..4_000 {
            tx.push(0.5).unwrap();
            tx.push(-0.25).unwrap();
        }
        let mut out = vec![0.0f32; 256 * 2];
        for _ in 0..4 {
            out.fill(0.0);
            reader.mix_into(&mut out, 2, 1.0);
        }
        assert!(out.chunks(2).all(|f| (f[0] - 0.5).abs() < 1e-4 && (f[1] + 0.25).abs() < 1e-4));
        let mut mono = vec![0.0f32; 128];
        reader.mix_into(&mut mono, 1, 2.0);
        assert!(mono.iter().all(|s| (s - 0.25).abs() < 1e-4));
    }

    #[test]
    fn muted_input_keeps_draining_and_backlog_is_dropped() {
        let (mut tx, rx) = rtrb::RingBuffer::new(48_000);
        let mut reader = MonitorReader::new(rx);
        while tx.push(0.7).is_ok() {}
        let mut out = vec![0.0f32; 512];
        reader.mix_into(&mut out, 2, 0.0);
        assert!(out.iter().all(|s| *s == 0.0));
        assert!(48_000 - tx.slots() < 2 * (256 * 4 + 4_800), "backlog trimmed to bounded latency");
    }
}
