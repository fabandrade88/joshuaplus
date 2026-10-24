//! Attacks found during the security review, reproduced as tests.
//! Each one must fail safely.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use joshua_core::project::SongProject;
use joshua_core::security::Limits;
use joshua_core::workspace::Workspace;

fn workspace() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let ws = Workspace::open(&dir.path().join("ws"), Limits::default()).unwrap();
    (dir, ws)
}

fn project(id: &str) -> SongProject {
    let json = serde_json::json!({
        "schemaVersion": 1, "id": id, "title": "x", "sourceFile": "source.wav", "sampleRate": 44100,
        "durationSecs": 10.0, "separator": "spectral", "createdUnix": 0,
        "analysis": {"bpm": 120.0, "beats": [0.5, 1.0], "beatsPerBar": 4, "downbeatPhase": 0, "confidence": 1.0},
        "tracks": [{"stem": "vocals", "file": "stems/vocals.wav", "gainDb": 0.0, "pan": 0.0, "muted": false, "solo": false}],
        "metronome": {"gainDb": -6.0, "muted": false, "countIn": true},
        "masterGainDb": 0.0
    });
    serde_json::from_value(json).unwrap()
}

/// Review #1: a planted symlink at the temp-file name must not redirect the
/// write outside the workspace (e.g. onto ~/.zshrc).
#[cfg(unix)]
#[test]
fn saving_never_follows_planted_symlinks() {
    let (dir, ws) = workspace();
    let victim = dir.path().join("victim.txt");
    std::fs::write(&victim, "precious").unwrap();
    let p = project("song-1");
    let song = ws.song_dir(&p.id).unwrap();
    std::fs::create_dir_all(&song).unwrap();
    for name in [".project.json.tmp", "project.json"] {
        let _ = std::fs::remove_file(song.join(name));
        std::os::unix::fs::symlink(&victim, song.join(name)).unwrap();
        let _ = ws.save_project(&p);
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "precious", "write followed {name}");
    }
    // Folder list and cover art use the same atomic-write path.
    let private = dir.path().join("ws/.joshua");
    std::os::unix::fs::symlink(&victim, private.join(".folders.json.tmp")).unwrap();
    let _ = ws.create_folder("Banda", "amber");
    std::os::unix::fs::symlink(&victim, song.join(".cover.jpg.tmp")).unwrap();
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbImage::new(8, 8).write_to(&mut png, image::ImageFormat::Png).unwrap();
    let _ = joshua_core::metadata::write_cover(&png.into_inner(), &song);
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "precious");
}

/// Review #2: workspace sub-directories that are symlinks must be refused,
/// so deleting a "song" can never delete something outside.
#[cfg(unix)]
#[test]
fn symlinked_workspace_folders_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let outside = dir.path().join("home");
    std::fs::create_dir_all(outside.join("projects")).unwrap();
    std::fs::write(outside.join("projects/thesis.txt"), "years of work").unwrap();
    let root = dir.path().join("shared-ws");
    std::fs::create_dir_all(&root).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("songs")).unwrap();
    match Workspace::open(&root, Limits::default()) {
        Err(_) => {}
        Ok(ws) => {
            let _ = ws.delete_song("projects");
        }
    }
    assert!(outside.join("projects/thesis.txt").exists(), "deleted data outside the workspace");
}

/// Review #2b: delete only removes real songs (a valid project inside).
#[test]
fn delete_requires_a_real_song() {
    let (_dir, ws) = workspace();
    let not_a_song = ws.songs_dir().join("notes");
    std::fs::create_dir_all(&not_a_song).unwrap();
    std::fs::write(not_a_song.join("keep.txt"), "x").unwrap();
    assert!(ws.delete_song("notes").is_err());
    assert!(not_a_song.join("keep.txt").exists());
}

/// Review #5: absurd beat times in a tampered project are rejected, and
/// tempo edits can never allocate an unbounded grid.
#[test]
fn absurd_beat_times_are_rejected() {
    let mut p = project("song-2");
    p.analysis.beats = vec![0.0, 1e300];
    assert!(p.validate().is_err());
    let a = joshua_core::tempo::BeatAnalysis { beats: vec![0.0, 1e300], ..p.analysis.clone() };
    use joshua_core::tempo::TempoEdit;
    assert!(a.edited(TempoEdit::SetBpm { bpm: 120.0, anchor: 0.0 }).is_none());
    assert!(a.edited(TempoEdit::Tap(vec![0.0, 0.5, 1.0, 1.5])).is_none());
}

/// Review #6: a FIFO planted as project.json / audio must not hang the app.
#[cfg(unix)]
#[test]
fn fifos_in_the_workspace_do_not_block() {
    let (_dir, ws) = workspace();
    let song = ws.song_dir("song-3").unwrap();
    std::fs::create_dir_all(&song).unwrap();
    let fifo = song.join("project.json");
    if !std::process::Command::new("mkfifo").arg(&fifo).status().is_ok_and(|s| s.success()) {
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let ws2 = ws.clone();
    std::thread::spawn(move || {
        let _ = ws2.list_songs();
        let _ = ws2.load_project("song-3");
        let _ = joshua_core::decode::decode_file(&fifo, &Limits::default());
        let _ = joshua_core::metadata::read_tags(&fifo);
        tx.send(()).unwrap();
    });
    assert!(rx.recv_timeout(std::time::Duration::from_secs(5)).is_ok(), "FIFO blocked the app");
}

/// Review #4: decoded size is capped regardless of the sample rate, so a few
/// MB of compressed silence at 384 kHz cannot allocate gigabytes.
#[test]
fn decoded_frames_are_capped_independent_of_rate() {
    let limits = Limits { max_duration_secs: 2, ..Limits::default() };
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 384_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cur = std::io::Cursor::new(Vec::new());
    {
        let mut w = hound::WavWriter::new(&mut cur, spec).unwrap();
        for _ in 0..(384_000 * 2) {
            w.write_sample(0i16).unwrap();
        }
        w.finalize().unwrap();
    }
    // 2 s at 384 kHz exceeds the rate-independent cap of 2 s × 48 kHz.
    let r = joshua_core::decode::decode_source(Box::new(std::io::Cursor::new(cur.into_inner())), Some("wav"), &limits);
    assert!(r.is_err(), "high-rate input must hit the frame cap");
}
