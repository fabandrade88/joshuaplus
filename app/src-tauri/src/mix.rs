//! Whole-mix actions: back to the song's original balance, and the
//! automatic balance of recorded takes against the original instruments.

use joshua_core::balance::{Profile, plan, timeline_frames};
use joshua_core::decode::decode_file;
use joshua_core::project::{DEFAULT_TAKE_ROOM, SongProject, TakeSettings, TrackSettings};
use joshua_core::security::resolve_within;
use joshua_core::separation::StemKind;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::commands::{flush_and_project, with_song};
use crate::error::{ApiError, ApiResult};
use crate::player::apply_mix;
use crate::state::AppState;

/// The open song's mixer levels after a whole-mix change.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MixView {
    pub tracks: Vec<TrackSettings>,
    pub takes: Vec<TakeSettings>,
    pub song_gain_db: f32,
    pub song_muted: bool,
    pub master_gain_db: f32,
}

impl MixView {
    fn of(p: &SongProject) -> Self {
        Self {
            tracks: p.tracks.clone(),
            takes: p.takes.clone(),
            song_gain_db: p.song_gain_db,
            song_muted: p.song_muted,
            master_gain_db: p.master_gain_db,
        }
    }
}

/// What the automatic balance did with one take.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BalancedTake {
    pub id: String,
    /// The original instrument it replaces (now muted), if one matched.
    pub stem: Option<StemKind>,
    pub gain_db: f32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceResult {
    pub mix: MixView,
    pub takes: Vec<BalancedTake>,
}

/// Every level back to the song as it came out of separation: stems, song,
/// takes and master at 0 dB, centred, nothing muted or soloed (takes keep
/// the default room). The metronome is left alone.
fn reset(p: &mut SongProject) {
    for t in &mut p.tracks {
        t.gain_db = 0.0;
        t.pan = 0.0;
        t.muted = false;
        t.solo = false;
    }
    for t in &mut p.takes {
        t.gain_db = 0.0;
        t.pan = 0.0;
        t.muted = false;
        t.solo = false;
        t.room = DEFAULT_TAKE_ROOM;
    }
    p.song_gain_db = 0.0;
    p.song_muted = false;
    p.master_gain_db = 0.0;
}

#[tauri::command]
pub fn reset_mix(state: State<'_, AppState>) -> ApiResult<MixView> {
    let shared = state.shared().ok();
    with_song(&state, |song| {
        reset(&mut song.project);
        song.dirty = true;
        if let Some(s) = &shared {
            apply_mix(s, &song.project);
        }
        Ok(MixView::of(&song.project))
    })
}

/// Balances takes against the original instruments: each take matched to
/// the stem it plays gets that stem's level (with the gain the user gave the
/// stem and the song) and place in the stereo field, and that stem is muted.
/// Nothing else changes: other stems, mutes the user chose and takes left out
/// keep their settings. `takes`: the takes to balance (after recording, the
/// new ones), all when `None`. Analyses the song's files (a few seconds).
#[tauri::command]
pub async fn auto_balance(app: AppHandle, takes: Option<Vec<String>>) -> ApiResult<BalanceResult> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let (ws, project, _) = flush_and_project(&state)?;
        if project.takes.is_empty() {
            return Err(ApiError::invalid("no takes to balance"));
        }
        let song_dir = ws.song_dir(&project.id)?;
        let limits = *ws.limits();
        let frames = timeline_frames(project.duration_secs);
        // One file at a time: only the small profiles stay in memory.
        let profile = |file: &str, offset: f64| -> ApiResult<Profile> {
            let buffer = decode_file(&resolve_within(&song_dir, file)?, &limits)?;
            Ok(Profile::analyse(&buffer, offset, frames))
        };
        let stems = project.tracks.iter().map(|t| profile(&t.file, 0.0)).collect::<ApiResult<Vec<_>>>()?;
        let profiles = project.takes.iter().map(|t| profile(&t.file, t.offset_secs)).collect::<ApiResult<Vec<_>>>()?;
        let plan = plan(&stems, &profiles);

        let shared = state.shared().ok();
        with_song(&state, |song| {
            let p = &mut song.project;
            let same = p.id == project.id
                && p.takes.len() == project.takes.len()
                && p.takes.iter().zip(&project.takes).all(|(a, b)| a.id == b.id);
            if !same {
                return Err(ApiError::invalid("the song changed while it was analysed"));
            }
            let report = apply_plan(p, &plan, takes.as_deref());
            song.dirty = true;
            if let Some(s) = &shared {
                apply_mix(s, p);
            }
            Ok(BalanceResult { mix: MixView::of(p), takes: report })
        })
    })
    .await
    .map_err(|_| ApiError::new("internal", "balance crashed"))?
}

/// Applies a balance plan to the chosen takes (all when `only` is `None`).
/// Only those takes' level and pan change, and the stems they replace are
/// muted; nothing is ever unmuted or reset.
fn apply_plan(p: &mut SongProject, plan: &joshua_core::balance::Plan, only: Option<&[String]>) -> Vec<BalancedTake> {
    let song_gain = p.song_gain_db;
    let mut report = Vec::new();
    for (j, tp) in plan.takes.iter().enumerate() {
        let Some(take) = p.takes.get(j) else { break };
        if only.is_some_and(|ids| !ids.contains(&take.id)) {
            continue;
        }
        // The take stands in for the stem as the user hears it: through the
        // stem's own fader and the song's.
        let stem_gain = tp.stem.and_then(|i| p.tracks.get(i)).map_or(0.0, |s| s.gain_db);
        let gain = (tp.gain_db + stem_gain + song_gain).clamp(-96.0, 12.0);
        let id = take.id.clone();
        let take = &mut p.takes[j];
        take.gain_db = gain;
        take.pan = tp.pan;
        if let Some(stem) = tp.stem.and_then(|i| p.tracks.get_mut(i)) {
            stem.muted = true;
        }
        report.push(BalancedTake { id, stem: tp.stem.and_then(|i| p.tracks.get(i)).map(|s| s.stem), gain_db: gain });
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> SongProject {
        let track = |k| TrackSettings::new(k, format!("stems/{}.wav", k.as_str()));
        SongProject {
            schema_version: joshua_core::project::SCHEMA_VERSION,
            id: "song-0123abcd".into(),
            title: "Song".into(),
            source_file: "source.mp3".into(),
            sample_rate: 44_100,
            duration_secs: 60.0,
            separator: "spectral".into(),
            created_unix: 0,
            analysis: joshua_core::tempo::BeatAnalysis {
                bpm: 120.0,
                beats: vec![0.5, 1.0],
                beats_per_bar: 4,
                downbeat_phase: 0,
                confidence: 0.5,
                variable_tempo: false,
            },
            tracks: vec![track(StemKind::Vocals), track(StemKind::Drums), track(StemKind::Bass)],
            metronome: joshua_core::project::MetronomeSettings::default(),
            master_gain_db: 0.0,
            song_gain_db: 0.0,
            song_muted: false,
            metadata: joshua_core::metadata::SongMetadata::default(),
            has_cover: false,
            metadata_scanned: true,
            title_edited: false,
            folder: None,
            takes: vec![TakeSettings {
                id: "take-1".into(),
                name: "SPD-SX PRO".into(),
                file: "takes/take-1.wav".into(),
                offset_secs: 0.0,
                created_unix: 0,
                gain_db: 0.0,
                pan: 0.0,
                muted: false,
                solo: false,
                room: DEFAULT_TAKE_ROOM,
            }],
            typed_bpm: None,
        }
    }

    #[test]
    fn balancing_never_unmutes_or_resets_what_the_user_set() {
        use joshua_core::balance::{Plan, TakePlan};
        let mut p = project();
        let mut second = p.takes[0].clone();
        second.id = "take-2".into();
        second.file = "takes/take-2.wav".into();
        second.muted = true;
        second.gain_db = -9.0;
        p.takes.push(second);
        p.tracks[0].muted = true; // the user muted the vocals
        p.tracks[1].gain_db = -4.0; // and turned the drums down
        p.tracks[2].gain_db = -2.0;
        p.song_gain_db = -1.0;
        let plan = Plan {
            stem_muted: vec![false, true, false],
            takes: vec![
                TakePlan { stem: Some(1), gain_db: 6.0, pan: -0.2 },
                TakePlan { stem: Some(2), gain_db: 3.0, pan: 0.0 },
            ],
        };
        // After recording, only the new take is balanced.
        let report = apply_plan(&mut p, &plan, Some(&["take-1".to_string()]));
        assert_eq!(report.len(), 1);
        assert_eq!(p.takes[0].gain_db, 6.0 - 4.0 - 1.0, "stem and song faders count");
        assert_eq!(p.takes[0].pan, -0.2);
        assert!(p.tracks[0].muted, "a stem the user muted stays muted");
        assert!(p.tracks[1].muted, "the replaced drums are muted");
        assert_eq!(p.tracks[1].gain_db, -4.0, "stem levels are left alone");
        assert!(!p.tracks[2].muted, "a stem no balanced take replaces is untouched");
        assert!(p.takes[1].muted && p.takes[1].gain_db == -9.0, "takes left out keep everything");
        // A huge correction stays within the channel limit.
        let loud = Plan { stem_muted: vec![false; 3], takes: vec![TakePlan { stem: None, gain_db: 40.0, pan: 0.0 }] };
        apply_plan(&mut p, &loud, None);
        assert_eq!(p.takes[0].gain_db, 12.0);
    }

    #[test]
    fn reset_puts_every_level_back_but_leaves_the_click_alone() {
        let mut p = project();
        p.tracks[0].gain_db = -12.0;
        p.tracks[1].muted = true;
        p.tracks[2].solo = true;
        p.takes[0].gain_db = 6.0;
        p.takes[0].room = 0.9;
        p.song_gain_db = -3.0;
        p.master_gain_db = -6.0;
        p.metronome.gain_db = -20.0;
        reset(&mut p);
        assert!(p.tracks.iter().all(|t| t.gain_db == 0.0 && t.pan == 0.0 && !t.muted && !t.solo));
        assert!(p.takes.iter().all(|t| t.gain_db == 0.0 && t.room == DEFAULT_TAKE_ROOM));
        assert_eq!((p.song_gain_db, p.master_gain_db), (0.0, 0.0));
        assert_eq!(p.metronome.gain_db, -20.0);
    }
}
