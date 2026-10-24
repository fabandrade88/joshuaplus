//! Import pipeline: validate → copy → decode → analyse tempo → separate →
//! publish. Work happens in a hidden staging directory that is atomically
//! renamed into `songs/<id>` only on success, so the library never shows a
//! half-imported song, and failures clean up after themselves.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::decode::decode_file;
use crate::error::{CoreError, RejectReason, Result};
use crate::project::{MetronomeSettings, SCHEMA_VERSION, SongProject, TrackSettings};
use crate::security::{Limits, open_audio_file, resolve_within, sanitize_title, slugify};
use crate::separation::{SeparationJob, Separator, StemKind, StemOutput};
use crate::tempo;
use crate::workspace::Workspace;

/// Pipeline stage reported to the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportStage {
    /// First run only: fetching the studio engine model.
    DownloadingModel,
    Validating,
    Decoding,
    AnalyzingTempo,
    Separating,
    Finalizing,
}

/// Collaborators of one import.
pub struct ImportOptions<'a> {
    pub separator: &'a dyn Separator,
    /// Folder to file the song into (must exist).
    pub folder: Option<String>,
    pub progress: &'a (dyn Fn(ImportStage, f32) + Sync),
    pub cancel: &'a AtomicBool,
}

impl std::fmt::Debug for ImportOptions<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImportOptions").field("separator", &self.separator.id()).finish_non_exhaustive()
    }
}

/// Removes staging/scratch directories unless the import succeeded.
struct Cleanup {
    paths: Vec<PathBuf>,
    armed: bool,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        if self.armed {
            for p in &self.paths {
                let _ = std::fs::remove_dir_all(p);
            }
        }
    }
}

/// Unpredictable song ID: `<slug>-<8 hex>`.
#[must_use]
pub fn new_song_id(title: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut h = RandomState::new().build_hasher();
    h.write_u128(SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    h.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    h.write_u32(std::process::id());
    format!("{}-{:08x}", slugify(title), h.finish() & 0xFFFF_FFFF)
}

fn check_cancel(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) { Err(CoreError::Cancelled) } else { Ok(()) }
}

/// Tempo analysis of the drums (+ bass) stems, if present.
fn rhythm_analysis(
    song_dir: &Path,
    stems: &[StemOutput],
    limits: &Limits,
    mode: tempo::TempoMode,
) -> Option<tempo::BeatAnalysis> {
    let load = |kind: StemKind| {
        let out = stems.iter().find(|s| s.kind == kind)?;
        decode_file(&resolve_within(song_dir, &out.relative_path).ok()?, limits).ok()
    };
    let drums = load(StemKind::Drums)?;
    let mut mono = drums.to_mono();
    if let Some(bass) = load(StemKind::Bass).filter(|b| b.sample_rate() == drums.sample_rate()) {
        for (m, b) in mono.iter_mut().zip(bass.to_mono()) {
            *m += 0.5 * b;
        }
    }
    let rhythm = crate::StereoBuffer::from_mono(drums.sample_rate(), mono).ok()?;
    Some(tempo::analyze_with(&rhythm, mode))
}

/// Re-runs tempo detection for an existing song (rhythm stems first, the
/// original mix as fallback). Used to upgrade songs imported earlier.
///
/// # Errors
/// Invalid song directory or undecodable source.
pub fn reanalyze_tempo(ws: &Workspace, project: &SongProject, mode: tempo::TempoMode) -> Result<tempo::BeatAnalysis> {
    let dir = ws.song_dir(&project.id)?;
    let mix = tempo::analyze_with(&decode_file(&resolve_within(&dir, &project.source_file)?, ws.limits())?, mode);
    let stems: Vec<StemOutput> =
        project.tracks.iter().map(|t| StemOutput { kind: t.stem, relative_path: t.file.clone() }).collect();
    Ok(rhythm_analysis(&dir, &stems, ws.limits(), mode).filter(|r| better_than(r, &mix)).unwrap_or(mix))
}

/// Prefers the rhythm-section analysis unless it is clearly worse
/// (e.g. a song without drums).
fn better_than(rhythm: &tempo::BeatAnalysis, mix: &tempo::BeatAnalysis) -> bool {
    rhythm.beats.len() >= 8 && rhythm.confidence >= 0.6 * mix.confidence
}

/// Imports one audio file into the workspace.
///
/// # Errors
/// Security rejections, decode/separation failures or [`CoreError::Cancelled`].
pub fn import_song(ws: &Workspace, source: &Path, opts: &ImportOptions<'_>) -> Result<SongProject> {
    let progress = opts.progress;
    progress(ImportStage::Validating, 0.0);
    if let Some(f) = &opts.folder {
        if !ws.list_folders()?.iter().any(|x| &x.id == f) {
            return Err(CoreError::NotFound);
        }
    }
    let validated = open_audio_file(source, ws.limits())?;
    let raw_title = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let title = sanitize_title(&raw_title);
    let id = new_song_id(&title);

    let staging = ws.songs_dir().join(format!(".staging-{id}"));
    let scratch = ws.tmp_dir().join(&id);
    let mut cleanup = Cleanup { paths: vec![staging.clone(), scratch.clone()], armed: true };
    std::fs::create_dir(&staging)?;
    std::fs::create_dir_all(&scratch)?;

    // Copy from the *validated handle* with a hard size cap.
    let source_file = format!("source.{}", validated.extension);
    let source_copy = staging.join(&source_file);
    {
        let max = ws.limits().max_file_bytes;
        let mut dst = std::fs::File::create(&source_copy)?;
        let copied = std::io::copy(&mut validated.file.take(max + 1), &mut dst)?;
        if copied > max {
            return Err(RejectReason::TooLarge.into());
        }
        dst.flush()?;
    }
    check_cancel(opts.cancel)?;

    // Tags and cover art (best effort: a file without tags is fine).
    let tags = crate::metadata::read_tags(&source_copy).unwrap_or_default();
    let has_cover = tags.picture.as_deref().is_some_and(|p| crate::metadata::write_cover(p, &staging).unwrap_or(false));
    let title = tags.metadata.title.clone().unwrap_or(title);

    progress(ImportStage::Decoding, 0.0);
    let audio = decode_file(&source_copy, ws.limits())?;
    check_cancel(opts.cancel)?;

    progress(ImportStage::AnalyzingTempo, 0.0);
    let analysis = tempo::analyze(&audio);
    check_cancel(opts.cancel)?;

    progress(ImportStage::Separating, 0.0);
    let stems_dir = staging.join("stems");
    let stem_progress = |p: f32| progress(ImportStage::Separating, p);
    let job = SeparationJob {
        source_path: &source_copy,
        audio: &audio,
        stems_dir: &stems_dir,
        scratch_dir: &scratch,
        progress: &stem_progress,
        cancel: opts.cancel,
    };
    let stems = opts.separator.separate(&job)?;
    check_cancel(opts.cancel)?;

    // The separated rhythm section gives far cleaner onsets than the full
    // mix (no vocals or pads smearing the transients): re-analyse on it.
    progress(ImportStage::AnalyzingTempo, 0.5);
    let analysis = rhythm_analysis(&staging, &stems, ws.limits(), tempo::TempoMode::Auto)
        .filter(|r| better_than(r, &analysis))
        .unwrap_or(analysis);

    progress(ImportStage::Finalizing, 0.0);
    let project = SongProject {
        schema_version: SCHEMA_VERSION,
        id: id.clone(),
        title,
        source_file,
        sample_rate: audio.sample_rate(),
        duration_secs: audio.duration_secs(),
        separator: opts.separator.id(),
        created_unix: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        analysis,
        tracks: stems.into_iter().map(|s| TrackSettings::new(s.kind, s.relative_path)).collect(),
        metronome: MetronomeSettings::default(),
        master_gain_db: 0.0,
        song_gain_db: 0.0,
        song_muted: false,
        metadata: tags.metadata,
        has_cover,
        metadata_scanned: true,
        title_edited: false,
        folder: opts.folder.clone(),
    };
    project.save(&staging)?;
    std::fs::rename(&staging, ws.song_dir(&id)?)?;
    cleanup.paths.retain(|p| p != &staging);
    drop(cleanup); // removes scratch only
    progress(ImportStage::Finalizing, 1.0);
    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StereoBuffer;
    use crate::security::Limits;
    use crate::separation::spectral::SpectralSeparator;
    use crate::wav::write_stereo_wav;

    fn workspace() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(&dir.path().join("ws"), Limits::default()).unwrap();
        (dir, ws)
    }

    fn song(dir: &Path, name: &str) -> PathBuf {
        let sr = 22_050;
        let samples: Vec<f32> = (0..sr * 3)
            .map(|i| {
                let t = i as f32 / sr as f32;
                let beat = (t * 2.0).fract();
                0.3 * (std::f32::consts::TAU * 220.0 * t).sin() + if beat < 0.02 { 0.8 } else { 0.0 }
            })
            .collect();
        let path = dir.join(name);
        write_stereo_wav(&path, &StereoBuffer::from_mono(sr as u32, samples).unwrap()).unwrap();
        path
    }

    fn opts<'a>(sep: &'a SpectralSeparator, cancel: &'a AtomicBool) -> ImportOptions<'a> {
        ImportOptions { separator: sep, folder: None, progress: &|_, _| {}, cancel }
    }

    #[test]
    fn imports_a_song_end_to_end() {
        let (dir, ws) = workspace();
        let src = song(dir.path(), "Minha Canção.wav");
        let sep = SpectralSeparator::default();
        let cancel = AtomicBool::new(false);
        let project = import_song(&ws, &src, &opts(&sep, &cancel)).unwrap();

        assert_eq!(project.title, "Minha Canção");
        assert!(project.id.starts_with("minha-cancao-"));
        assert_eq!(project.tracks.len(), 4);
        let song_dir = ws.song_dir(&project.id).unwrap();
        for t in &project.tracks {
            assert!(song_dir.join(&t.file).is_file());
        }
        assert_eq!(ws.list_songs().unwrap().len(), 1);
        assert!(src.exists(), "original must stay untouched");
        // No leftovers.
        assert_eq!(std::fs::read_dir(ws.tmp_dir()).unwrap().count(), 0);
        assert_eq!(std::fs::read_dir(ws.songs_dir()).unwrap().count(), 1);
    }

    #[test]
    fn failed_or_cancelled_imports_leave_no_trace() {
        let (dir, ws) = workspace();
        let sep = SpectralSeparator::default();

        let cancel = AtomicBool::new(true);
        let src = song(dir.path(), "a.wav");
        assert!(matches!(import_song(&ws, &src, &opts(&sep, &cancel)), Err(CoreError::Cancelled)));

        let fake = dir.path().join("fake.mp3");
        std::fs::write(&fake, b"ID3\x04\x00\x00\x00\x00\x00\x00garbage-not-audio").unwrap();
        let cancel = AtomicBool::new(false);
        assert!(import_song(&ws, &fake, &opts(&sep, &cancel)).is_err());

        let exe = dir.path().join("evil.wav");
        std::fs::write(&exe, b"MZ\x90\x00\x03\x00\x00\x00\x04\x00\x00\x00").unwrap();
        assert!(matches!(
            import_song(&ws, &exe, &opts(&sep, &cancel)),
            Err(CoreError::Rejected(RejectReason::UnrecognizedSignature))
        ));

        assert_eq!(std::fs::read_dir(ws.songs_dir()).unwrap().count(), 0);
        assert_eq!(std::fs::read_dir(ws.tmp_dir()).unwrap().count(), 0);
    }

    #[test]
    fn ids_are_unique_and_valid() {
        let ids: std::collections::HashSet<_> = (0..1000).map(|_| new_song_id("Song")).collect();
        assert_eq!(ids.len(), 1000);
        assert!(ids.iter().all(|id| crate::security::is_valid_song_id(id)));
    }
}
