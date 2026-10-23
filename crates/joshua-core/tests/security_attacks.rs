//! Adversarial integration tests. Each test is an attack an untrusted file,
//! folder or IPC argument could attempt; all must fail *safely*: no panic,
//! no write outside the workspace, no unbounded memory, no code execution.
//! See `docs/SECURITY.md` for the threat model.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Cursor;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use joshua_core::decode::{decode_file, decode_source};
use joshua_core::import::{ImportOptions, import_song};
use joshua_core::project::{PROJECT_FILE, SongProject};
use joshua_core::security::Limits;
use joshua_core::separation::spectral::SpectralSeparator;
use joshua_core::workspace::Workspace;
use joshua_core::{CoreError, RejectReason, StereoBuffer};

fn workspace() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let ws = Workspace::open(&dir.path().join("ws"), Limits::default()).unwrap();
    (dir, ws)
}

fn import(ws: &Workspace, path: &Path) -> joshua_core::Result<SongProject> {
    let sep = SpectralSeparator::default();
    let cancel = AtomicBool::new(false);
    import_song(ws, path, &ImportOptions { separator: &sep, folder: None, progress: &|_, _| {}, cancel: &cancel })
}

/// Minimal PCM WAV header with arbitrary fields.
fn wav_header(channels: u16, rate: u32, bits: u16, data_len: u32) -> Vec<u8> {
    let block = channels.wrapping_mul(bits / 8);
    let mut v = Vec::new();
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36u32.wrapping_add(data_len)).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&channels.to_le_bytes());
    v.extend_from_slice(&rate.to_le_bytes());
    v.extend_from_slice(&rate.wrapping_mul(u32::from(block)).to_le_bytes());
    v.extend_from_slice(&block.to_le_bytes());
    v.extend_from_slice(&bits.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    v
}

fn decode_bytes(bytes: Vec<u8>) -> joshua_core::Result<StereoBuffer> {
    decode_source(Box::new(Cursor::new(bytes)), Some("wav"), &Limits::default())
}

#[test]
fn lying_wav_headers_do_not_allocate_or_panic() {
    // Claims 4 GiB of data, ships 16 bytes.
    let mut liar = wav_header(2, 44_100, 16, u32::MAX);
    liar.extend_from_slice(&[0u8; 16]);
    let _ = decode_bytes(liar);

    // Absurd stream parameters: must never panic; invalid rates must fail.
    for (ch, rate, bits) in [(0, 44_100, 16), (65_535, 44_100, 16), (2, 44_100, 0), (2, 44_100, 7)] {
        let mut v = wav_header(ch, rate, bits, 64);
        v.extend_from_slice(&[0x7Fu8; 64]);
        let _ = decode_bytes(v);
    }
    for rate in [0, 1, 7_999, 384_001, u32::MAX] {
        let mut v = wav_header(2, rate, 16, 64);
        v.extend_from_slice(&[0x7Fu8; 64]);
        assert!(decode_bytes(v).is_err(), "sample rate {rate} must be refused");
    }
}

#[test]
fn random_bytes_never_panic_the_decoder() {
    let mut state = 0x1234_5678_u32;
    for len in [0usize, 1, 4, 12, 44, 100, 1_000, 10_000] {
        for prefix in [&b""[..], b"RIFF\0\0\0\0WAVE", b"fLaC", b"OggS", b"ID3\x04", b"\xFF\xFB", b"\0\0\0\x18ftypM4A "]
        {
            let mut v = prefix.to_vec();
            for _ in 0..len {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                v.push(state.to_le_bytes()[0]);
            }
            let _ = decode_source(Box::new(Cursor::new(v)), None, &Limits::default());
        }
    }
}

#[test]
fn polyglot_executable_with_audio_magic_is_contained() {
    // A file that *starts* like audio but carries an executable payload is
    // only ever decoded as audio; it is copied as `source.wav` (never run).
    let (dir, ws) = workspace();
    let mut poly = wav_header(1, 8_000, 16, 8_000 * 2);
    poly.extend(std::iter::repeat_n(0u8, 8_000 * 2));
    poly.extend_from_slice(b"MZ\x90\x00 this program cannot be run in DOS mode");
    let path = dir.path().join("track.wav");
    std::fs::write(&path, &poly).unwrap();
    if let Ok(p) = import(&ws, &path) {
        let song_dir = ws.song_dir(&p.id).unwrap();
        assert_eq!(p.source_file, "source.wav");
        for entry in walk(&song_dir) {
            let name = entry.file_name().unwrap().to_string_lossy().to_string();
            assert!(name.ends_with(".wav") || name == PROJECT_FILE, "unexpected file {name}");
        }
    }
}

#[test]
fn hostile_file_names_cannot_escape_or_spoof() {
    let (dir, ws) = workspace();
    let names = [
        "..%2F..%2Fescape.wav",
        "song\u{202E}vaw.exe.wav",
        "CON.wav",
        "a\u{0}b.wav",
        "     .wav",
        "$(reboot).wav",
        "`id`.wav",
        "<script>alert(1)</script>.wav",
    ];
    for name in names {
        let Ok(path) = std::panic::catch_unwind(|| dir.path().join(name)) else { continue };
        let mut audio = wav_header(1, 8_000, 16, 16_000);
        audio.extend(std::iter::repeat_n(0u8, 16_000));
        if std::fs::write(&path, &audio).is_err() {
            continue; // OS refused the name (NUL etc.) — fine.
        }
        let Ok(p) = import(&ws, &path) else { continue };
        assert!(joshua_core::security::is_valid_song_id(&p.id), "{name}: id {}", p.id);
        assert!(!p.title.chars().any(|c| c.is_control() || ('\u{202A}'..='\u{202E}').contains(&c)));
        assert!(ws.song_dir(&p.id).unwrap().starts_with(ws.songs_dir()));
    }
    // Nothing was written next to the workspace.
    let siblings: Vec<_> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert!(siblings.iter().all(|n| n == "ws" || n.to_string_lossy().ends_with(".wav")));
}

#[test]
fn tampered_project_cannot_read_files_outside_the_song() {
    let (dir, ws) = workspace();
    let secret = dir.path().join("secret.wav");
    let mut audio = wav_header(1, 8_000, 16, 16);
    audio.extend_from_slice(&[0u8; 16]);
    std::fs::write(&secret, &audio).unwrap();

    let mut song = wav_header(1, 8_000, 16, 16_000);
    song.extend(std::iter::repeat_n(0u8, 16_000));
    let src = dir.path().join("song.wav");
    std::fs::write(&src, &song).unwrap();
    let p = import(&ws, &src).unwrap();

    // Attacker edits project.json to point a track at ../../secret.wav.
    let song_dir = ws.song_dir(&p.id).unwrap();
    let text = std::fs::read_to_string(song_dir.join(PROJECT_FILE)).unwrap();
    let evil = text.replacen("stems/vocals.wav", "../../../secret.wav", 1);
    std::fs::write(song_dir.join(PROJECT_FILE), evil).unwrap();
    assert!(matches!(ws.load_project(&p.id), Err(CoreError::Rejected(RejectReason::InvalidProject))));
}

#[cfg(unix)]
#[test]
fn symlinked_stems_directory_is_not_followed() {
    let (dir, ws) = workspace();
    let mut song = wav_header(1, 8_000, 16, 16_000);
    song.extend(std::iter::repeat_n(0u8, 16_000));
    let src = dir.path().join("song.wav");
    std::fs::write(&src, &song).unwrap();
    let p = import(&ws, &src).unwrap();
    let song_dir = ws.song_dir(&p.id).unwrap();

    let outside = dir.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::rename(song_dir.join("stems"), outside.join("stems")).unwrap();
    std::os::unix::fs::symlink(outside.join("stems"), song_dir.join("stems")).unwrap();
    let err = joshua_core::security::resolve_within(&song_dir, "stems/vocals.wav").unwrap_err();
    assert!(matches!(err, CoreError::Rejected(RejectReason::OutsideWorkspace)));
}

#[cfg(unix)]
#[test]
fn fifo_and_device_files_are_rejected_without_blocking() {
    let (dir, ws) = workspace();
    // A FIFO would block forever on open/read; it must be refused up front.
    let fifo = dir.path().join("pipe.wav");
    let made = std::process::Command::new("mkfifo").arg(&fifo).status().is_ok_and(|s| s.success());
    if made {
        assert!(matches!(import(&ws, &fifo), Err(CoreError::Rejected(RejectReason::NotARegularFile))));
    }
    // Character devices disguised by a symlink with an audio extension.
    let link = dir.path().join("zero.wav");
    std::os::unix::fs::symlink("/dev/zero", &link).unwrap();
    assert!(matches!(import(&ws, &link), Err(CoreError::Rejected(RejectReason::Symlink))));
    assert!(decode_file(Path::new("/dev/null"), &Limits::default()).is_err());
}

#[test]
fn oversized_inputs_are_refused_before_processing() {
    let (dir, _) = workspace();
    let ws = Workspace::open(&dir.path().join("small"), Limits { max_file_bytes: 1_000, ..Limits::default() }).unwrap();
    let mut big = wav_header(1, 8_000, 16, 4_000);
    big.extend(std::iter::repeat_n(0u8, 4_000));
    let path = dir.path().join("big.wav");
    std::fs::write(&path, &big).unwrap();
    assert!(matches!(import(&ws, &path), Err(CoreError::Rejected(RejectReason::TooLarge))));
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() { out.extend(walk(&p)) } else { out.push(p) }
    }
    out
}
