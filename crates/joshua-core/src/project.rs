//! `project.json`: per-song metadata, analysis and mixer settings.
//!
//! Project files are untrusted on load (a user could share a song folder),
//! so parsing is size-limited, strict (`deny_unknown_fields`) and every
//! value is range-checked by [`SongProject::validate`].

use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, RejectReason, Result};
use crate::metadata::SongMetadata;
use crate::metronome::ClickSound;
use crate::mixer::MAX_TRACKS;
use crate::security::{Limits, is_safe_relative_name, is_valid_song_id};
use crate::separation::StemKind;
use crate::tempo::BeatAnalysis;

pub const SCHEMA_VERSION: u32 = 1;
pub const PROJECT_FILE: &str = "project.json";
const MAX_BEATS: usize = 100_000;

/// Mixer settings of one stem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TrackSettings {
    pub stem: StemKind,
    /// Relative path inside the song directory.
    pub file: String,
    pub gain_db: f32,
    pub pan: f32,
    pub muted: bool,
    pub solo: bool,
}

impl TrackSettings {
    #[must_use]
    pub fn new(stem: StemKind, file: String) -> Self {
        Self { stem, file, gain_db: 0.0, pan: 0.0, muted: false, solo: false }
    }
}

/// Most recorded takes kept per song.
pub const MAX_TAKES: usize = 8;

/// A recorded take: one input's audio, aligned to the song timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TakeSettings {
    /// Stable id (`take-<n>`), unique within the song.
    pub id: String,
    /// Display name, e.g. the input it came from ("SPD-SX PRO").
    pub name: String,
    /// Relative path inside the song directory (`takes/<id>.wav`).
    pub file: String,
    /// Song time (seconds) of the first sample in the file.
    pub offset_secs: f64,
    pub created_unix: u64,
    pub gain_db: f32,
    pub pan: f32,
    pub muted: bool,
    pub solo: bool,
    /// How much of the take goes into the shared room (`0..=1`), so a dry
    /// recording sits in the same space as the band.
    #[serde(default = "default_take_room")]
    pub room: f32,
}

/// Room send of a new take: enough to sit with the band, still clear.
pub const DEFAULT_TAKE_ROOM: f32 = 0.3;

const fn default_take_room() -> f32 {
    DEFAULT_TAKE_ROOM
}

impl TakeSettings {
    fn is_valid(&self) -> bool {
        self.id.len() <= 32
            && self.id.starts_with("take-")
            && self.id[5..].bytes().all(|b| b.is_ascii_digit())
            && self.id.len() > 5
            && self.name.chars().count() <= 80
            && !self.name.chars().any(char::is_control)
            && is_safe_relative_name(&self.file)
            && self.file.starts_with("takes/")
            && self.offset_secs.is_finite()
            && (0.0..=24.0 * 3600.0).contains(&self.offset_secs)
            && in_range(self.gain_db, -96.0, 12.0)
            && in_range(self.pan, -1.0, 1.0)
            && in_range(self.room, 0.0, 1.0)
    }
}

/// Metronome settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MetronomeSettings {
    pub gain_db: f32,
    pub muted: bool,
    pub count_in: bool,
    /// Bars of count-in before playback (1..=4).
    #[serde(default = "default_count_in_bars")]
    pub count_in_bars: u8,
    #[serde(default)]
    pub sound: ClickSound,
}

const fn default_count_in_bars() -> u8 {
    1
}

impl Default for MetronomeSettings {
    fn default() -> Self {
        Self { gain_db: -6.0, muted: false, count_in: true, count_in_bars: 1, sound: ClickSound::Classic }
    }
}

/// Everything persisted about a song.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SongProject {
    pub schema_version: u32,
    pub id: String,
    pub title: String,
    /// Copy of the imported file, e.g. `source.mp3`.
    pub source_file: String,
    pub sample_rate: u32,
    pub duration_secs: f64,
    /// Separator identifier (`spectral`, `demucs:htdemucs_6s`).
    pub separator: String,
    pub created_unix: u64,
    pub analysis: BeatAnalysis,
    pub tracks: Vec<TrackSettings>,
    pub metronome: MetronomeSettings,
    pub master_gain_db: f32,
    /// Group level of all stems (collapsed "Song" strip).
    #[serde(default)]
    pub song_gain_db: f32,
    #[serde(default)]
    pub song_muted: bool,
    /// Tags read from the source file (artist, album…).
    #[serde(default)]
    pub metadata: SongMetadata,
    /// `cover.jpg` / `cover-thumb.jpg` exist in the song directory.
    #[serde(default)]
    pub has_cover: bool,
    /// Tags have been read (older songs are back-filled once).
    #[serde(default)]
    pub metadata_scanned: bool,
    /// The user renamed the song; tags must not overwrite the title.
    #[serde(default)]
    pub title_edited: bool,
    /// Folder the song belongs to (`None` = not filed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    /// Recorded takes, mixed after the stems (outside the song bus).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub takes: Vec<TakeSettings>,
    /// The tempo the user last typed. Detecting the tempo again keeps it,
    /// so it can be offered back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typed_bpm: Option<f64>,
}

fn in_range(v: f32, lo: f32, hi: f32) -> bool {
    v.is_finite() && (lo..=hi).contains(&v)
}

impl SongProject {
    /// Checks every invariant; called on load *and* before save.
    ///
    /// # Errors
    /// [`RejectReason::InvalidProject`] when anything is out of bounds.
    pub fn validate(&self) -> Result<()> {
        let a = &self.analysis;
        let ok = self.schema_version == SCHEMA_VERSION
            && is_valid_song_id(&self.id)
            && self.title.chars().count() <= 200
            && !self.title.chars().any(char::is_control)
            && is_safe_relative_name(&self.source_file)
            && (8_000..=384_000).contains(&self.sample_rate)
            && self.duration_secs.is_finite()
            && (0.0..=24.0 * 3600.0).contains(&self.duration_secs)
            && self.separator.len() <= 64
            && a.bpm.is_finite()
            && (20.0..=400.0).contains(&a.bpm)
            && self.typed_bpm.is_none_or(|b| b.is_finite() && (20.0..=400.0).contains(&b))
            && a.beats.len() <= MAX_BEATS
            && a.beats.iter().all(|b| b.is_finite() && *b >= 0.0 && *b <= self.duration_secs + 1.0)
            && a.beats.windows(2).all(|w| w[1] > w[0])
            && (1..=16).contains(&a.beats_per_bar)
            && a.downbeat_phase < a.beats_per_bar
            && a.confidence.is_finite()
            && !self.tracks.is_empty()
            && self.tracks.len() <= MAX_TRACKS
            && self.tracks.iter().enumerate().all(|(i, t)| self.tracks[..i].iter().all(|o| o.file != t.file))
            && self.tracks.iter().all(|t| {
                is_safe_relative_name(&t.file) && in_range(t.gain_db, -96.0, 12.0) && in_range(t.pan, -1.0, 1.0)
            })
            && in_range(self.metronome.gain_db, -96.0, 12.0)
            && (1..=4).contains(&self.metronome.count_in_bars)
            && in_range(self.master_gain_db, -96.0, 12.0)
            && in_range(self.song_gain_db, -96.0, 12.0)
            && self.folder.as_deref().is_none_or(is_valid_song_id)
            && self.takes.len() <= MAX_TAKES
            && self.tracks.len() + self.takes.len() <= MAX_TRACKS
            && self.takes.iter().all(TakeSettings::is_valid)
            && self.takes.iter().enumerate().all(|(i, t)| {
                self.takes[..i].iter().all(|o| o.id != t.id && o.file != t.file)
                    && self.tracks.iter().all(|s| s.file != t.file)
            })
            && self.metadata.is_valid();
        if ok { Ok(()) } else { Err(RejectReason::InvalidProject.into()) }
    }

    /// Reads and validates `dir/project.json`.
    ///
    /// # Errors
    /// I/O, size limit, JSON or validation failures.
    pub fn load(dir: &Path, limits: &Limits) -> Result<Self> {
        let file = crate::security::open_regular(&dir.join(PROJECT_FILE))?;
        let mut text = String::new();
        file.take(limits.max_project_bytes + 1).read_to_string(&mut text)?;
        if text.len() as u64 > limits.max_project_bytes {
            return Err(RejectReason::TooLarge.into());
        }
        let project: Self = serde_json::from_str(&text).map_err(|e| CoreError::Project(e.to_string()))?;
        project.validate()?;
        Ok(project)
    }

    /// Validates and atomically writes `dir/project.json`.
    ///
    /// # Errors
    /// I/O or validation failures.
    pub fn save(&self, dir: &Path) -> Result<()> {
        self.validate()?;
        let json = serde_json::to_vec_pretty(self).map_err(|e| CoreError::Project(e.to_string()))?;
        crate::security::atomic_write(dir, PROJECT_FILE, &json, false)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn sample_project() -> SongProject {
        SongProject {
            schema_version: SCHEMA_VERSION,
            id: "my-song-0123abcd".into(),
            title: "My Song".into(),
            source_file: "source.mp3".into(),
            sample_rate: 44_100,
            duration_secs: 180.0,
            separator: "spectral".into(),
            created_unix: 0,
            analysis: BeatAnalysis {
                bpm: 120.0,
                beats: vec![0.5, 1.0],
                beats_per_bar: 4,
                downbeat_phase: 0,
                confidence: 0.5,
                variable_tempo: false,
            },
            tracks: vec![TrackSettings::new(StemKind::Vocals, "stems/vocals.wav".into())],
            metronome: MetronomeSettings::default(),
            master_gain_db: 0.0,
            song_gain_db: 0.0,
            song_muted: false,
            metadata: SongMetadata::default(),
            has_cover: false,
            metadata_scanned: true,
            title_edited: false,
            folder: None,
            takes: Vec::new(),
            typed_bpm: None,
        }
    }

    pub fn sample_take(n: u32) -> TakeSettings {
        TakeSettings {
            id: format!("take-{n}"),
            name: "SPD-SX PRO".into(),
            file: format!("takes/take-{n}.wav"),
            offset_secs: 12.5,
            created_unix: 0,
            gain_db: 0.0,
            pan: 0.0,
            muted: false,
            solo: false,
            room: DEFAULT_TAKE_ROOM,
        }
    }

    #[test]
    fn takes_round_trip_and_hostile_takes_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = sample_project();
        p.takes = vec![sample_take(1), sample_take(2)];
        p.save(dir.path()).unwrap();
        assert_eq!(SongProject::load(dir.path(), &Limits::default()).unwrap(), p);

        type Tamper = Box<dyn Fn(&mut SongProject)>;
        let cases: Vec<Tamper> = vec![
            Box::new(|p| p.takes[0].file = "../../etc/passwd".into()),
            Box::new(|p| p.takes[0].file = "stems/vocals.wav".into()),
            Box::new(|p| p.takes[0].file = "takes/../project.json".into()),
            Box::new(|p| p.takes[1].id = "take-1".into()),
            Box::new(|p| p.takes[1].file = "takes/take-1.wav".into()),
            Box::new(|p| p.takes[0].id = "take-../x".into()),
            Box::new(|p| p.takes[0].id = "take-".into()),
            Box::new(|p| p.takes[0].offset_secs = f64::NAN),
            Box::new(|p| p.takes[0].offset_secs = -1.0),
            Box::new(|p| p.takes[0].gain_db = 99.0),
            Box::new(|p| p.takes[0].name = "x\u{0}".into()),
            Box::new(|p| p.takes = (1..=MAX_TAKES as u32 + 1).map(sample_take).collect()),
        ];
        for (i, mutate) in cases.iter().enumerate() {
            let mut q = p.clone();
            mutate(&mut q);
            assert!(q.validate().is_err(), "take case {i} should be rejected");
        }
    }

    #[test]
    fn a_typed_tempo_is_kept_and_checked() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = sample_project();
        p.typed_bpm = Some(100.0);
        p.save(dir.path()).unwrap();
        assert_eq!(SongProject::load(dir.path(), &Limits::default()).unwrap().typed_bpm, Some(100.0));
        for bad in [f64::NAN, f64::INFINITY, 5.0, 900.0] {
            p.typed_bpm = Some(bad);
            assert!(p.validate().is_err(), "typed {bad} accepted");
        }
    }

    #[test]
    fn save_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let p = sample_project();
        p.save(dir.path()).unwrap();
        assert_eq!(SongProject::load(dir.path(), &Limits::default()).unwrap(), p);
    }

    #[test]
    fn rejects_tampered_projects() {
        type Tamper = Box<dyn Fn(&mut SongProject)>;
        let cases: Vec<Tamper> = vec![
            Box::new(|p| p.tracks[0].file = "../../../etc/passwd".into()),
            Box::new(|p| p.tracks[0].file = "/etc/passwd".into()),
            Box::new(|p| p.source_file = "..\\..\\x.mp3".into()),
            Box::new(|p| p.id = "../other".into()),
            Box::new(|p| p.tracks[0].gain_db = f32::INFINITY),
            Box::new(|p| p.tracks[0].gain_db = 200.0),
            Box::new(|p| p.analysis.bpm = f64::NAN),
            Box::new(|p| p.analysis.beats = vec![2.0, 1.0]),
            Box::new(|p| p.analysis.beats_per_bar = 0),
            Box::new(|p| p.tracks.clear()),
            Box::new(|p| p.tracks = vec![p.tracks[0].clone(); MAX_TRACKS + 1]),
            Box::new(|p| p.title = "bad\u{7}title".into()),
            Box::new(|p| p.schema_version = 99),
            Box::new(|p| p.folder = Some("../escape".into())),
            Box::new(|p| p.metadata.artist = Some("evil\u{1b}[31m".into())),
        ];
        for (i, mutate) in cases.iter().enumerate() {
            let mut p = sample_project();
            mutate(&mut p);
            assert!(p.validate().is_err(), "case {i} should be rejected");
        }
    }

    #[test]
    fn rejects_unknown_fields_oversized_and_malformed_json() {
        let dir = tempfile::tempdir().unwrap();
        let mut value = serde_json::to_value(sample_project()).unwrap();
        value["__proto__"] = serde_json::json!({"polluted": true});
        std::fs::write(dir.path().join(PROJECT_FILE), value.to_string()).unwrap();
        assert!(SongProject::load(dir.path(), &Limits::default()).is_err());

        let small = Limits { max_project_bytes: 16, ..Limits::default() };
        sample_project().save(dir.path()).unwrap();
        assert!(matches!(SongProject::load(dir.path(), &small), Err(CoreError::Rejected(RejectReason::TooLarge))));

        // Deeply nested JSON must fail cleanly (serde_json has a recursion limit).
        let bomb = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
        std::fs::write(dir.path().join(PROJECT_FILE), bomb).unwrap();
        assert!(SongProject::load(dir.path(), &Limits::default()).is_err());
    }
}
