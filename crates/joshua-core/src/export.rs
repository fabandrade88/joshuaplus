//! Offline mixdown: renders a session through the real [`Mixer`], so an
//! export sounds exactly like playback (same gains, pans, mutes, solos, song
//! bus, takes, tone, room) — minus anything live (inputs, count-in).
//! [`render_mastered`] then masters it for sharing: streaming loudness and a
//! look-ahead limiter instead of the playback safety clipper.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::{CoreError, Result};
use crate::master::{CEILING_DB, LoudnessMeter, MAX_BOOST_DB, PeakLimiter, STREAMING_LUFS};
use crate::mixer::{Mixer, Session, SharedState};

const BLOCK: usize = 4_096;

/// Receives rendered audio as planar stereo blocks.
pub type BlockSink<'a> = dyn FnMut(&[f32], &[f32]) -> Result<()> + 'a;

/// Renders `start..end` (session frames) of `session` with the mixer state
/// set up by `configure`, handing planar stereo blocks to `sink`.
/// `progress` receives `0.0..=1.0`. Gives the session back (to render again).
///
/// # Errors
/// Errors from `sink`, or [`CoreError::Cancelled`].
pub fn render_mix(
    session: Box<Session>,
    configure: &dyn Fn(&SharedState),
    start: i64,
    end: i64,
    sink: &mut BlockSink<'_>,
    progress: &dyn Fn(f32),
    cancel: &AtomicBool,
) -> Result<Box<Session>> {
    let end = end.min(session.length);
    let start = start.clamp(0, end.max(0));
    let shared = SharedState::default();
    configure(&shared);
    let mut mixer = Mixer::new();
    mixer.swap_session(Some(session));
    // Gains ramp from silence after a session swap; a one-frame render
    // settles them so the export does not fade in.
    shared.seek(start);
    shared.play();
    let mut one = [0.0f32; 2];
    mixer.render(&shared, &mut one, 2);
    shared.seek(start);

    let total = (end - start).max(1) as f32;
    let mut inter = vec![0.0f32; BLOCK * 2];
    let (mut l, mut r) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
    let mut pos = start;
    while pos < end {
        if cancel.load(Ordering::Relaxed) {
            return Err(CoreError::Cancelled);
        }
        let n = usize::try_from(end - pos).unwrap_or(BLOCK).min(BLOCK);
        mixer.render(&shared, &mut inter[..n * 2], 2);
        for (i, f) in inter[..n * 2].chunks_exact(2).enumerate() {
            l[i] = f[0];
            r[i] = f[1];
        }
        sink(&l[..n], &r[..n])?;
        pos += n as i64;
        progress((pos - start) as f32 / total);
    }
    mixer.swap_session(None).ok_or_else(|| CoreError::InvalidAudio("no session".into()))
}

/// What [`render_mastered`] did.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mastered {
    /// Loudness of the mix as it plays (LUFS), `None` if silent.
    pub loudness: Option<f32>,
    /// Gain applied to reach streaming loudness (dB).
    pub gain_db: f32,
}

/// Like [`render_mix`], mastered for sharing: a first pass measures the
/// mix's loudness, the second plays it at [`STREAMING_LUFS`] (turned down,
/// or up by at most [`MAX_BOOST_DB`]) through a look-ahead limiter at
/// [`CEILING_DB`] instead of the playback clipper, so hits keep their shape.
/// The output stays sample-aligned with the timeline (videos).
///
/// # Errors
/// As [`render_mix`].
pub fn render_mastered(
    session: Box<Session>,
    configure: &dyn Fn(&SharedState),
    start: i64,
    end: i64,
    sink: &mut BlockSink<'_>,
    progress: &dyn Fn(f32),
    cancel: &AtomicBool,
) -> Result<Mastered> {
    let rate = session.sample_rate;
    let unclipped = |shared: &SharedState| {
        configure(shared);
        shared.safety_clip.store(false, Ordering::Relaxed);
    };
    let mut meter = LoudnessMeter::new(rate);
    let session = render_mix(
        session,
        &unclipped,
        start,
        end,
        &mut |l, r| {
            meter.add(l, r);
            Ok(())
        },
        &|p| progress(p * 0.35),
        cancel,
    )?;
    let loudness = meter.integrated();
    let gain_db = loudness.map_or(0.0, |l| (STREAMING_LUFS - l).min(MAX_BOOST_DB));
    let gain = 10f32.powf(gain_db / 20.0);
    let mut limiter = PeakLimiter::new(rate, CEILING_DB);
    let latency = limiter.latency();
    // The limiter delays by `latency` frames: drop that much at the start and
    // flush it at the end, so the output lines up with the timeline.
    let mut skip = latency;
    let (mut ol, mut or) = (Vec::with_capacity(BLOCK), Vec::with_capacity(BLOCK));
    let mut emit = |l: &[f32], r: &[f32], ol: &mut Vec<f32>, or: &mut Vec<f32>, sink: &mut BlockSink<'_>| {
        ol.clear();
        or.clear();
        for (a, b) in l.iter().zip(r) {
            let (x, y) = limiter.process(a * gain, b * gain);
            if skip > 0 {
                skip -= 1;
            } else {
                ol.push(x);
                or.push(y);
            }
        }
        if ol.is_empty() { Ok(()) } else { sink(ol, or) }
    };
    render_mix(
        session,
        &unclipped,
        start,
        end,
        &mut |l, r| emit(l, r, &mut ol, &mut or, sink),
        &|p| progress(0.35 + p * 0.65),
        cancel,
    )?;
    let zeros = vec![0.0f32; latency];
    emit(&zeros, &zeros, &mut ol, &mut or, sink)?;
    Ok(Mastered { loudness, gain_db })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StereoBuffer;
    use crate::metronome::ClickTrack;
    use crate::mixer::METRONOME;
    use crate::tempo::BeatAnalysis;

    fn grid() -> BeatAnalysis {
        BeatAnalysis {
            bpm: 120.0,
            beats: (0..20).map(|k| f64::from(k) * 0.5).collect(),
            beats_per_bar: 4,
            downbeat_phase: 0,
            confidence: 1.0,
            variable_tempo: false,
        }
    }

    fn session() -> Box<Session> {
        let stem = StereoBuffer::new(8_000, vec![0.25; 40_000], vec![0.25; 40_000]).unwrap();
        let take = StereoBuffer::new(8_000, vec![0.5; 40_000], vec![0.0; 40_000]).unwrap();
        Box::new(Session::with_takes(8_000, vec![stem], vec![take], ClickTrack::from_analysis(&grid(), 8_000)))
    }

    fn export(configure: &dyn Fn(&SharedState), start: i64, end: i64) -> (Vec<f32>, Vec<f32>) {
        let (mut l, mut r) = (Vec::new(), Vec::new());
        render_mix(
            session(),
            configure,
            start,
            end,
            &mut |a, b| {
                l.extend_from_slice(a);
                r.extend_from_slice(b);
                Ok(())
            },
            &|_| {},
            &AtomicBool::new(false),
        )
        .unwrap();
        (l, r)
    }

    #[test]
    fn exports_the_mix_without_fade_in_and_with_exact_length() {
        let (l, r) = export(&|s| s.channels[METRONOME].set_mute(true), 1_000, 21_000);
        assert_eq!(l.len(), 20_000);
        assert!((l[0] - 0.75).abs() < 1e-6 && (r[0] - 0.25).abs() < 1e-6, "no fade-in: {} {}", l[0], r[0]);
        assert!(l.iter().all(|s| (s - 0.75).abs() < 1e-6));
    }

    #[test]
    fn click_is_included_only_when_not_muted_and_mutes_apply() {
        let (with_click, _) = export(&|_| {}, 0, 8_000);
        let (no_click, _) = export(&|s| s.channels[METRONOME].set_mute(true), 0, 8_000);
        assert!(with_click.iter().zip(&no_click).any(|(a, b)| (a - b).abs() > 1e-3), "click audible");
        let (take_muted, _) = export(
            &|s| {
                s.channels[METRONOME].set_mute(true);
                s.channels[1].set_mute(true);
            },
            0,
            8_000,
        );
        assert!(take_muted.iter().all(|s| (s - 0.25).abs() < 1e-6));
    }

    #[test]
    fn mastering_reaches_streaming_loudness_under_the_ceiling() {
        // A loud mix (stem + take sum to 1.0 on the left, way over -14 LUFS)
        // with a hard hit, as an export of a record plus a take.
        let stem = StereoBuffer::new(8_000, vec![0.6; 40_000], vec![0.5; 40_000]).unwrap();
        let mut hit = vec![0.0f32; 40_000];
        for (i, v) in hit.iter_mut().enumerate() {
            *v = 0.5 * ((i as f32) * 0.3).sin();
        }
        hit[20_000] = 0.9;
        let take = StereoBuffer::new(8_000, hit.clone(), hit).unwrap();
        let s = Box::new(Session::with_takes(8_000, vec![stem], vec![take], ClickTrack::from_analysis(&grid(), 8_000)));
        let (mut l, mut r) = (Vec::new(), Vec::new());
        let m = render_mastered(
            s,
            &|s| s.channels[METRONOME].set_mute(true),
            0,
            40_000,
            &mut |a, b| {
                l.extend_from_slice(a);
                r.extend_from_slice(b);
                Ok(())
            },
            &|_| {},
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(l.len(), 40_000, "same length as the timeline");
        assert!(m.gain_db < 0.0, "turned down: {m:?}");
        let ceiling = 10f32.powf(CEILING_DB / 20.0);
        assert!(l.iter().chain(&r).all(|x| x.abs() <= ceiling + 1e-6));
        let mut meter = LoudnessMeter::new(8_000);
        meter.add(&l, &r);
        let out = meter.integrated().unwrap();
        assert!((out - STREAMING_LUFS).abs() < 1.0, "{out} LUFS");
    }

    #[test]
    fn range_is_clamped_and_cancel_stops() {
        let (l, _) = export(&|_| {}, -500, 1_000_000);
        assert_eq!(l.len(), 40_000);
        let r = render_mix(session(), &|_| {}, 0, 40_000, &mut |_, _| Ok(()), &|_| {}, &AtomicBool::new(true));
        assert!(matches!(r, Err(CoreError::Cancelled)));
    }
}
