//! Play-along guides for the open song: lyrics for singers, chords for
//! players.
//!
//! Lyrics come from LRCLIB (lrclib.net, an open lyrics database with no key
//! and time-synced lines), only when the user switches lyrics on: the song's
//! title, artist, album and length are sent, nothing else. What is found is
//! kept in the song's directory (`lyrics.json`) and shows offline from then
//! on; the user can also paste lyrics. With "download lyrics automatically"
//! on, a background worker looks up every song of the library that has none
//! yet (one request a second), and tries again later when offline. Chords
//! never leave the machine: they are recognised from the stems and kept in
//! `chords.json`.

use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use joshua_core::chords::{ChordChart, detect};
use joshua_core::decode::decode_file;
use joshua_core::lyrics::{LyricLine, Lyrics, MAX_LYRICS_BYTES, parse_lrc};
use joshua_core::project::SongProject;
use joshua_core::security::{atomic_write, resolve_within};
use joshua_core::separation::StemKind;
use joshua_core::workspace::Workspace;
use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::flush_and_project;
use crate::error::{ApiError, ApiResult};
use crate::state::{AppState, lock};

const LYRICS_FILE: &str = "lyrics.json";
const CHORDS_FILE: &str = "chords.json";
const LRCLIB: &str = "https://lrclib.net/api";
/// A recording found online this much longer or shorter is another edit.
const DURATION_TOLERANCE_SECS: f64 = 4.0;
/// A song whose lyrics were not found is looked up again after this long.
const MISS_RETRY_SECS: u64 = 14 * 24 * 3600;
/// Background pass: pause between songs, and between passes.
const PREFETCH_GAP: Duration = Duration::from_secs(1);
const PREFETCH_EVERY: Duration = Duration::from_secs(30 * 60);
/// After a network failure, try again sooner.
const PREFETCH_OFFLINE_RETRY: Duration = Duration::from_secs(5 * 60);

/// One LRCLIB record (fields we use).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    #[serde(default)]
    duration: f64,
    #[serde(default)]
    instrumental: bool,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .http_status_as_error(false)
        .user_agent(concat!("Joshua+/", env!("CARGO_PKG_VERSION"), " (https://github.com/fabandrade88/joshuaplus)"))
        .build()
        .into()
}

/// GET with the query; a busy server (it often is, briefly) gets two more
/// tries, a little further apart each time.
fn get(agent: &ureq::Agent, path: &str, query: &[(&str, String)]) -> Result<Option<String>, String> {
    let mut backoff = [1500u64, 4000].into_iter();
    loop {
        let mut req = agent.get(&format!("{LRCLIB}/{path}"));
        for (k, v) in query {
            req = req.query(*k, v);
        }
        let mut res = req.call().map_err(|e| e.to_string())?;
        match res.status().as_u16() {
            200 => {
                let body =
                    res.body_mut().with_config().limit(4 * 1024 * 1024).read_to_string().map_err(|e| e.to_string())?;
                return Ok(Some(body));
            }
            404 => return Ok(None),
            503 | 502 | 429 => match backoff.next() {
                Some(ms) => std::thread::sleep(Duration::from_millis(ms)),
                None => return Err("server busy".to_owned()),
            },
            s => return Err(format!("HTTP {s}")),
        }
    }
}

/// "Paparazzi - triple j Like A Version" → "Paparazzi"; "Song (Live)" → "Song".
fn core_title(title: &str) -> &str {
    let cut = [" - ", " (", " ["].iter().filter_map(|s| title.find(s)).min().unwrap_or(title.len());
    title[..cut].trim()
}

/// The record that best fits the user's file: closest length, synced first.
fn pick(records: Vec<Record>, duration: f64) -> Option<Record> {
    records
        .into_iter()
        .filter(|r| r.instrumental || r.plain_lyrics.as_deref().is_some_and(|t| !t.trim().is_empty()))
        .filter(|r| r.duration <= 0.0 || (r.duration - duration).abs() <= DURATION_TOLERANCE_SECS)
        .min_by(|a, b| {
            let key = |r: &Record| (r.synced_lyrics.is_none(), ((r.duration - duration).abs() * 10.0) as i64);
            key(a).cmp(&key(b))
        })
}

/// Looks the song up on LRCLIB: exact match first, then a search.
fn fetch(project: &SongProject) -> Result<Option<Lyrics>, String> {
    let agent = agent();
    let duration = project.duration_secs;
    let artist = project.metadata.artist.clone().unwrap_or_default();
    let title = project.title.clone();
    let mut found: Option<Record> = None;
    if !artist.is_empty() {
        let mut q = vec![("track_name", title.clone()), ("artist_name", artist.clone())];
        if let Some(album) = &project.metadata.album {
            q.push(("album_name", album.clone()));
        }
        #[allow(clippy::cast_possible_truncation)]
        q.push(("duration", format!("{}", duration.round() as i64)));
        if let Some(body) = get(&agent, "get", &q)? {
            found = serde_json::from_str::<Record>(&body).ok().and_then(|r| pick(vec![r], duration));
        }
    }
    let searches: Vec<Vec<(&str, String)>> = if artist.is_empty() {
        vec![vec![("q", title.clone())], vec![("q", core_title(&title).to_owned())]]
    } else {
        vec![
            vec![("track_name", title.clone()), ("artist_name", artist.clone())],
            vec![("q", format!("{} {artist}", core_title(&title)))],
        ]
    };
    for q in searches {
        if found.is_some() {
            break;
        }
        if let Some(body) = get(&agent, "search", &q)? {
            found = pick(serde_json::from_str::<Vec<Record>>(&body).unwrap_or_default(), duration);
        }
    }
    Ok(found.map(|r| Lyrics {
        source: "lrclib".to_owned(),
        synced: r.synced_lyrics.as_deref().map(parse_lrc).unwrap_or_default(),
        plain: r.plain_lyrics.unwrap_or_default(),
        instrumental: r.instrumental,
        offset_secs: 0.0,
        checked_unix: now(),
    }))
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Looks the song up and stores what was found, or that nothing was.
fn lookup_and_store(dir: &std::path::Path, project: &SongProject) -> Result<Option<Lyrics>, String> {
    match fetch(project)? {
        Some(l) => {
            let l = sanitise(l).map_err(|e| e.message)?;
            save_lyrics(dir, &l).map_err(|e| e.message)?;
            Ok(Some(l))
        }
        None => {
            let miss = Lyrics { source: "lrclib".to_owned(), checked_unix: now(), ..Lyrics::default() };
            save_lyrics(dir, &miss).map_err(|e| e.message)?;
            Ok(None)
        }
    }
}

/// The song has no lyrics yet, or the last lookup found none long ago.
fn needs_lookup(dir: &std::path::Path) -> bool {
    match read_json::<Lyrics>(dir, LYRICS_FILE) {
        None => true,
        Some(l) => l.is_miss() && now().saturating_sub(l.checked_unix) > MISS_RETRY_SECS,
    }
}

fn wake_signal() -> &'static (Mutex<bool>, Condvar) {
    static WAKE: OnceLock<(Mutex<bool>, Condvar)> = OnceLock::new();
    WAKE.get_or_init(|| (Mutex::new(false), Condvar::new()))
}

/// Asks the background worker for a pass now (new song, setting switched on).
pub fn wake() {
    let (flag, cv) = wake_signal();
    *flag.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = true;
    cv.notify_all();
}

fn wait(timeout: Duration) {
    let (flag, cv) = wake_signal();
    let guard = flag.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let (mut guard, _) =
        cv.wait_timeout_while(guard, timeout, |woken| !*woken).unwrap_or_else(std::sync::PoisonError::into_inner);
    *guard = false;
}

/// One pass over the library. `Err`: the network failed, try again soon.
fn prefetch_pass(app: &AppHandle, ws: &Workspace) -> Result<(), String> {
    let songs = ws.list_songs().map_err(|e| e.to_string())?;
    for song in songs {
        if !lock(&app.state::<AppState>().settings).lyrics_auto {
            return Ok(());
        }
        let Ok(dir) = ws.song_dir(&song.id) else { continue };
        if !needs_lookup(&dir) {
            continue;
        }
        let Ok(project) = ws.load_project(&song.id) else { continue };
        if lookup_and_store(&dir, &project)?.is_some() {
            log::info!("lyrics found for {}", song.id);
            let _ = app.emit("lyrics-ready", &song.id);
        }
        std::thread::sleep(PREFETCH_GAP);
    }
    Ok(())
}

/// Starts the background lyrics worker (idle while the setting is off).
pub fn start_prefetch(app: AppHandle) {
    let spawned = std::thread::Builder::new().name("joshua-lyrics".into()).spawn(move || {
        // Let the app finish starting first.
        wait(Duration::from_secs(10));
        loop {
            let state = app.state::<AppState>();
            let enabled = lock(&state.settings).lyrics_auto;
            let next = match (enabled, state.workspace()) {
                (true, Ok(ws)) => match prefetch_pass(&app, &ws) {
                    Ok(()) => PREFETCH_EVERY,
                    Err(e) => {
                        log::info!("lyrics prefetch paused: {e}");
                        PREFETCH_OFFLINE_RETRY
                    }
                },
                _ => PREFETCH_EVERY,
            };
            wait(next);
        }
    });
    if let Err(e) = spawned {
        log::error!("could not start the lyrics worker: {e}");
    }
}

/// Download lyrics for the whole library in the background.
#[tauri::command]
pub fn set_lyrics_auto(state: State<'_, AppState>, enabled: bool) -> ApiResult<()> {
    lock(&state.settings).lyrics_auto = enabled;
    state.save_settings()?;
    if enabled {
        wake();
    }
    Ok(())
}

/// Keeps text and line counts sane, whatever the source.
fn sanitise(mut l: Lyrics) -> ApiResult<Lyrics> {
    if l.plain.len() > MAX_LYRICS_BYTES || l.synced.iter().map(|x| x.text.len()).sum::<usize>() > MAX_LYRICS_BYTES {
        return Err(ApiError::invalid("lyrics too long"));
    }
    l.plain = l.plain.chars().filter(|c| !c.is_control() || *c == '\n').collect();
    l.offset_secs = if l.offset_secs.is_finite() { l.offset_secs.clamp(-30.0, 30.0) } else { 0.0 };
    Ok(l)
}

fn read_json<T: for<'de> Deserialize<'de>>(dir: &std::path::Path, name: &str) -> Option<T> {
    let path = dir.join(name);
    let meta = std::fs::metadata(&path).ok()?;
    if meta.len() > 8 * 1024 * 1024 {
        return None;
    }
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn save_lyrics(dir: &std::path::Path, l: &Lyrics) -> ApiResult<()> {
    let bytes = serde_json::to_vec_pretty(l).map_err(|e| ApiError::new("io", e.to_string()))?;
    Ok(atomic_write(dir, LYRICS_FILE, &bytes, false)?)
}

/// The open song's lyrics: the stored ones, else (with `online`) looked up
/// on LRCLIB and stored. `None`: nothing found.
#[tauri::command]
pub async fn get_lyrics(app: AppHandle, online: bool, refresh: Option<bool>) -> ApiResult<Option<Lyrics>> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let (ws, project, _) = flush_and_project(&state)?;
        let dir = ws.song_dir(&project.id)?;
        if !refresh.unwrap_or(false) && !needs_lookup(&dir) {
            return Ok(read_json::<Lyrics>(&dir, LYRICS_FILE).filter(|l| !l.is_miss()));
        }
        if !online {
            return Ok(None);
        }
        match lookup_and_store(&dir, &project) {
            Ok(found) => Ok(found),
            Err(e) => {
                log::info!("lyrics lookup: {e}");
                Err(ApiError::new("offline", "could not reach the lyrics service"))
            }
        }
    })
    .await
    .map_err(|e| ApiError::new("internal", e.to_string()))?
}

/// Lyrics typed or pasted by the user (plain text or LRC with time stamps).
#[tauri::command]
pub async fn set_lyrics(app: AppHandle, text: String) -> ApiResult<Option<Lyrics>> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let (ws, project, _) = flush_and_project(&state)?;
        let dir = ws.song_dir(&project.id)?;
        if text.trim().is_empty() {
            match std::fs::remove_file(dir.join(LYRICS_FILE)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(ApiError::new("io", e.to_string())),
            }
            return Ok(None);
        }
        let synced = parse_lrc(&text);
        let plain = if synced.is_empty() {
            text.clone()
        } else {
            synced.iter().map(|l: &LyricLine| l.text.as_str()).collect::<Vec<_>>().join("\n")
        };
        let l = sanitise(Lyrics {
            source: "user".to_owned(),
            synced,
            plain,
            instrumental: false,
            offset_secs: 0.0,
            checked_unix: now(),
        })?;
        save_lyrics(&dir, &l)?;
        Ok(Some(l))
    })
    .await
    .map_err(|e| ApiError::new("internal", e.to_string()))?
}

/// Shifts the synced lines (seconds), when they run early or late.
#[tauri::command]
pub async fn set_lyrics_offset(app: AppHandle, offset_secs: f64) -> ApiResult<Option<Lyrics>> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let (ws, project, _) = flush_and_project(&state)?;
        let dir = ws.song_dir(&project.id)?;
        let Some(mut l) = read_json::<Lyrics>(&dir, LYRICS_FILE).filter(|l| !l.is_miss()) else { return Ok(None) };
        l.offset_secs = offset_secs;
        let l = sanitise(l)?;
        save_lyrics(&dir, &l)?;
        Ok(Some(l))
    })
    .await
    .map_err(|e| ApiError::new("internal", e.to_string()))?
}

/// The open song's chord chart, recognised from its stems (once per beat
/// grid; kept in `chords.json`).
#[tauri::command]
pub async fn get_chords(app: AppHandle) -> ApiResult<ChordChart> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let (ws, project, _) = flush_and_project(&state)?;
        let dir = ws.song_dir(&project.id)?;
        if let Some(chart) = read_json::<ChordChart>(&dir, CHORDS_FILE).filter(|c| c.is_current(&project.analysis)) {
            return Ok(chart);
        }
        let limits = *ws.limits();
        let (mut harmony, mut bass, mut rate) = (Vec::<f32>::new(), Vec::<f32>::new(), project.sample_rate);
        let mut add = |target: &mut Vec<f32>, file: &str| -> ApiResult<()> {
            let buffer = decode_file(&resolve_within(&dir, file)?, &limits)?;
            rate = buffer.sample_rate();
            let mono = buffer.to_mono();
            if target.len() < mono.len() {
                target.resize(mono.len(), 0.0);
            }
            for (o, x) in target.iter_mut().zip(mono) {
                *o += x;
            }
            Ok(())
        };
        let harmonic: Vec<_> =
            project.tracks.iter().filter(|t| !matches!(t.stem, StemKind::Drums | StemKind::Vocals)).collect();
        for t in &harmonic {
            if t.stem == StemKind::Bass {
                add(&mut bass, &t.file)?;
            } else {
                add(&mut harmony, &t.file)?;
            }
        }
        if harmony.is_empty() {
            // No separated instruments: the whole record.
            add(&mut harmony, &project.source_file)?;
        }
        let chart = detect(&harmony, &bass, rate, &project.analysis, project.duration_secs);
        let bytes = serde_json::to_vec(&chart).map_err(|e| ApiError::new("io", e.to_string()))?;
        atomic_write(&dir, CHORDS_FILE, &bytes, false)?;
        Ok(chart)
    })
    .await
    .map_err(|e| ApiError::new("internal", e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(duration: f64, synced: bool) -> Record {
        Record {
            duration,
            instrumental: false,
            plain_lyrics: Some("words".to_owned()),
            synced_lyrics: synced.then(|| "[00:01.00]words".to_owned()),
        }
    }

    #[test]
    fn the_closest_synced_recording_wins() {
        let got = pick(vec![rec(248.0, true), rec(216.0, false), rec(215.0, true), rec(219.0, true)], 215.5).unwrap();
        assert!((got.duration - 215.0).abs() < 1e-9 && got.synced_lyrics.is_some());
        assert!(pick(vec![rec(248.0, true)], 215.5).is_none(), "another edit of the song");
        assert!(pick(vec![Record { plain_lyrics: None, ..rec(215.0, true) }], 215.0).is_none());
    }

    /// Live lookup: `cargo test -p joshua-app lrclib -- --ignored`.
    #[test]
    #[ignore = "needs the internet"]
    fn lrclib_finds_a_known_song() {
        let mut p: SongProject = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1, "id": "x-00000000", "title": "Paparazzi - triple j Like A Version",
            "sourceFile": "source.mp3", "sampleRate": 44100, "durationSecs": 243.2, "separator": "spectral",
            "createdUnix": 0, "analysis": {"bpm": 105.0, "beats": [], "beatsPerBar": 4, "downbeatPhase": 0, "confidence": 0.0},
            "tracks": [], "metronome": {"gainDb": 0.0, "muted": false, "countIn": false, "countInBars": 1, "sound": "woodblock"},
            "masterGainDb": 0.0
        }))
        .unwrap();
        p.metadata.artist = Some("Thornhill".to_owned());
        let l = fetch(&p).unwrap().unwrap();
        assert!(l.synced.len() > 20, "{} synced lines", l.synced.len());
        println!("{:?}", &l.synced[..3]);
    }

    #[test]
    fn titles_lose_their_version_notes() {
        assert_eq!(core_title("Paparazzi - triple j Like A Version"), "Paparazzi");
        assert_eq!(core_title("Song (Live) [Remastered]"), "Song");
        assert_eq!(core_title("Plain"), "Plain");
    }
}
