//! Turns a song project into an engine session.

use joshua_core::decode::decode_file;
use joshua_core::metronome::ClickTrack;
use joshua_core::mixer::{MAX_TRACKS, METRONOME, Session, SharedState};
use joshua_core::peaks::compute_peaks;
use joshua_core::project::SongProject;
use joshua_core::resample::resample;
use joshua_core::security::resolve_within;
use joshua_core::workspace::Workspace;
use joshua_core::{Result, StereoBuffer};

/// Waveform resolution sent to the UI.
pub const PEAK_BUCKETS: usize = 2_000;

/// Decodes all stems in parallel, resamples them to `target_rate` and builds
/// the click track. Returns the session and per-track waveform peaks.
pub fn build_session(ws: &Workspace, project: &SongProject, target_rate: u32) -> Result<(Session, Vec<Vec<u8>>)> {
    let song_dir = ws.song_dir(&project.id)?;
    let limits = *ws.limits();
    // Decode at most 3 stems at once: bounds peak memory on long songs and
    // tampered projects while keeping most of the speed-up.
    let decode = |track: &joshua_core::project::TrackSettings| -> Result<(StereoBuffer, Vec<u8>)> {
        let path = resolve_within(&song_dir, &track.file)?;
        let buffer = decode_file(&path, &limits)?;
        let peaks = compute_peaks(&buffer, PEAK_BUCKETS);
        Ok((resample(buffer, target_rate)?, peaks))
    };
    let tracks_in: Vec<_> = project.tracks.iter().take(MAX_TRACKS).collect();
    let mut results: Vec<Result<(StereoBuffer, Vec<u8>)>> = Vec::with_capacity(tracks_in.len());
    for batch in tracks_in.chunks(3) {
        let batch_results: Vec<_> = std::thread::scope(|scope| {
            let handles: Vec<_> = batch.iter().map(|t| scope.spawn(|| decode(t))).collect();
            handles
                .into_iter()
                .map(|h| h.join().unwrap_or_else(|_| Err(joshua_core::CoreError::Decode("worker panicked".into()))))
                .collect()
        });
        results.extend(batch_results);
    }
    let mut tracks = Vec::with_capacity(results.len());
    let mut peaks = Vec::with_capacity(results.len());
    for r in results {
        let (buffer, p) = r?;
        tracks.push(buffer);
        peaks.push(p);
    }
    let click = ClickTrack::from_analysis(&project.analysis, target_rate);
    Ok((Session::new(target_rate, tracks, click), peaks))
}

/// Pushes the project's mixer settings into the real-time controls.
pub fn apply_mix(shared: &SharedState, project: &SongProject) {
    for (i, ch) in shared.channels.iter().enumerate().take(MAX_TRACKS) {
        match project.tracks.get(i) {
            Some(t) => {
                ch.set_gain_db(t.gain_db);
                ch.set_pan(t.pan);
                ch.set_mute(t.muted);
                ch.set_solo(t.solo);
            }
            None => {
                ch.set_gain_db(0.0);
                ch.set_pan(0.0);
                ch.set_mute(false);
                ch.set_solo(false);
            }
        }
    }
    let click = &shared.channels[METRONOME];
    click.set_gain_db(project.metronome.gain_db);
    click.set_mute(project.metronome.muted);
    click.set_pan(0.0);
    shared.set_click_voice(project.metronome.sound.index());
    shared.song_bus.set_gain_db(project.song_gain_db);
    shared.song_bus.set_mute(project.song_muted);
    shared.master_gain.store(joshua_core::audio::db_to_gain(project.master_gain_db));
}
