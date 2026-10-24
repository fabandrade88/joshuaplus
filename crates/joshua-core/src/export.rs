//! Offline mixdown: renders a session through the real [`Mixer`], so an
//! export sounds exactly like playback (same gains, pans, mutes, solos, song
//! bus, takes, soft limiter) — minus anything live (inputs, count-in).

use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::{CoreError, Result};
use crate::mixer::{Mixer, Session, SharedState};

const BLOCK: usize = 4_096;

/// Receives rendered audio as planar stereo blocks.
pub type BlockSink<'a> = dyn FnMut(&[f32], &[f32]) -> Result<()> + 'a;

/// Renders `start..end` (session frames) of `session` with the mixer state
/// set up by `configure`, handing planar stereo blocks to `sink`.
/// `progress` receives `0.0..=1.0`.
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
) -> Result<()> {
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
    Ok(())
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
    fn range_is_clamped_and_cancel_stops() {
        let (l, _) = export(&|_| {}, -500, 1_000_000);
        assert_eq!(l.len(), 40_000);
        let r = render_mix(session(), &|_| {}, 0, 40_000, &mut |_, _| Ok(()), &|_| {}, &AtomicBool::new(true));
        assert!(matches!(r, Err(CoreError::Cancelled)));
    }
}
