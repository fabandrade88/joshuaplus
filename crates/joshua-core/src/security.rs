//! Input validation. Every byte that enters Joshua+ from the outside world
//! (dropped files, project files, IDs coming from the UI) passes through here.
//!
//! Principles:
//! * **Allow-list, never deny-list** – extensions, signatures and name
//!   characters are explicitly enumerated.
//! * **Validate the handle you use** – [`open_audio_file`] returns the open
//!   `File` it validated so callers copy *that* handle (no TOCTOU swap).
//! * **Bounded resources** – size, duration and JSON length limits stop
//!   decompression/memory bombs.

use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use crate::error::{CoreError, RejectReason, Result};

/// Resource limits applied to untrusted input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Largest audio file accepted, in bytes.
    pub max_file_bytes: u64,
    /// Longest decoded audio accepted, in seconds.
    pub max_duration_secs: u32,
    /// Largest `project.json` accepted, in bytes.
    pub max_project_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_file_bytes: 1024 * 1024 * 1024, // 1 GiB
            max_duration_secs: 30 * 60,         // 30 minutes
            max_project_bytes: 8 * 1024 * 1024, // 8 MiB
        }
    }
}

/// Lower-case file extensions that may be imported.
pub const ALLOWED_EXTENSIONS: &[&str] =
    &["mp3", "wav", "wave", "flac", "ogg", "oga", "m4a", "mp4", "aac", "aif", "aiff"];

/// Container families recognised from magic bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    Wav,
    Flac,
    Ogg,
    Aiff,
    Mp4,
    /// MPEG audio frame (MP3) or ADTS (AAC) – both start with a frame sync.
    MpegFrames,
    /// ID3v2 tagged stream (MP3, occasionally AAC/FLAC).
    Id3Tagged,
}

/// Identifies a container from the first bytes of a file.
#[must_use]
pub fn sniff_container(header: &[u8]) -> Option<Container> {
    let at = |range: std::ops::Range<usize>| header.get(range);
    if matches!(at(0..4), Some(b"RIFF" | b"RF64")) && at(8..12) == Some(b"WAVE") {
        return Some(Container::Wav);
    }
    if at(0..4) == Some(b"FORM") && matches!(at(8..12), Some(b"AIFF" | b"AIFC")) {
        return Some(Container::Aiff);
    }
    match at(0..4) {
        Some(b"fLaC") => return Some(Container::Flac),
        Some(b"OggS") => return Some(Container::Ogg),
        _ => {}
    }
    if at(4..8) == Some(b"ftyp") {
        return Some(Container::Mp4);
    }
    if at(0..3) == Some(b"ID3") {
        return Some(Container::Id3Tagged);
    }
    if let (Some(&b0), Some(&b1)) = (header.first(), header.get(1)) {
        if b0 == 0xFF && b1 & 0xE0 == 0xE0 {
            return Some(Container::MpegFrames);
        }
    }
    None
}

/// Lower-cased, allow-listed extension of `path`.
///
/// # Errors
/// [`RejectReason::UnsupportedExtension`] if missing or not allow-listed.
pub fn allowed_extension(path: &Path) -> Result<String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or(RejectReason::UnsupportedExtension)?;
    if ALLOWED_EXTENSIONS.contains(&ext.as_str()) { Ok(ext) } else { Err(RejectReason::UnsupportedExtension.into()) }
}

/// A file that passed validation, together with the handle that was checked.
#[derive(Debug)]
pub struct ValidatedAudioFile {
    pub file: File,
    pub size: u64,
    pub extension: String,
    pub container: Container,
}

/// Opens and validates an audio file chosen by the user.
///
/// Rejects symlinks, non-regular files, empty/oversized files, unknown
/// extensions and files whose magic bytes are not a known audio container.
///
/// # Errors
/// [`CoreError::Rejected`] with the reason, or [`CoreError::Io`].
pub fn open_audio_file(path: &Path, limits: &Limits) -> Result<ValidatedAudioFile> {
    let extension = allowed_extension(path)?;
    let link_meta = std::fs::symlink_metadata(path)?;
    if link_meta.file_type().is_symlink() {
        return Err(RejectReason::Symlink.into());
    }
    if !link_meta.is_file() {
        return Err(RejectReason::NotARegularFile.into());
    }
    let mut file = File::open(path)?;
    // Re-check on the opened handle: the path may have been swapped meanwhile.
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Err(RejectReason::NotARegularFile.into());
    }
    let size = meta.len();
    if size == 0 {
        return Err(RejectReason::EmptyFile.into());
    }
    if size > limits.max_file_bytes {
        return Err(RejectReason::TooLarge.into());
    }
    let mut header = [0u8; 12];
    let read = read_up_to(&mut file, &mut header)?;
    let container = sniff_container(&header[..read]).ok_or(RejectReason::UnrecognizedSignature)?;
    // Rewind so the caller reads from the start.
    let file = reopen_at_start(file)?;
    Ok(ValidatedAudioFile { file, size, extension, container })
}

fn read_up_to(file: &mut File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match file.read(&mut buf[filled..])? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(filled)
}

fn reopen_at_start(mut file: File) -> std::io::Result<File> {
    use std::io::{Seek, SeekFrom};
    file.seek(SeekFrom::Start(0))?;
    Ok(file)
}

/// Opens an existing file for reading only if it is a *regular* file.
///
/// Symlinks, directories, FIFOs and device files are refused. On Unix the
/// open is non-blocking, so a FIFO planted under a trusted name can never
/// hang the caller even if it is swapped in between the check and the open.
///
/// # Errors
/// [`RejectReason::Symlink`], [`RejectReason::NotARegularFile`] or I/O errors.
pub fn open_regular(path: &Path) -> Result<File> {
    let meta = std::fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() {
        return Err(RejectReason::Symlink.into());
    }
    if !meta.is_file() {
        return Err(RejectReason::NotARegularFile.into());
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
    }
    let file = opts.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(RejectReason::NotARegularFile.into());
    }
    Ok(file)
}

/// Unpredictable suffix for temporary file names.
fn random_suffix() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    h.write_u32(std::process::id());
    format!("{:016x}", h.finish())
}

/// Creates a brand-new file that cannot already exist (not even as a
/// symlink): `O_CREAT | O_EXCL` never follows links. `private` restricts it
/// to the current user from the very first byte (secrets).
///
/// # Errors
/// I/O errors, including "already exists".
pub fn create_exclusive(path: &Path, private: bool) -> Result<File> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(if private { 0o600 } else { 0o644 });
    }
    #[cfg(not(unix))]
    let _ = private;
    Ok(opts.open(path)?)
}

/// Writes `dir/name` atomically and safely: data goes to a fresh, randomly
/// named temp file created exclusively (a planted symlink cannot redirect
/// it), is flushed to disk, then renamed over the target (the rename
/// replaces a symlink at `name` instead of writing through it).
///
/// # Errors
/// I/O errors (the temp file is removed).
pub fn atomic_write(dir: &Path, name: &str, bytes: &[u8], private: bool) -> Result<()> {
    use std::io::Write;
    let tmp = dir.join(format!(".{name}.{}.tmp", random_suffix()));
    let result = (|| -> Result<()> {
        let mut f = create_exclusive(&tmp, private)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, dir.join(name))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Characters that must never reach the UI or a file name: C0/C1 controls
/// and Unicode bidi / invisible formatting characters (Trojan-Source style
/// spoofing such as `song\u{202E}3pm.exe`).
fn is_dangerous_char(c: char) -> bool {
    c.is_control()
        || matches!(c,
            '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{FEFF}')
}

/// Human readable title derived from an untrusted file name.
/// Never used to build paths, only displayed.
#[must_use]
pub fn sanitize_title(raw: &str) -> String {
    let cleaned: String = raw.chars().filter(|c| !is_dangerous_char(*c)).collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let truncated: String = collapsed.chars().take(120).collect();
    if truncated.is_empty() { "Untitled".to_owned() } else { truncated }
}

/// Folds common Latin diacritics (pt-BR / de) to ASCII.
fn fold_char(c: char) -> Option<&'static str> {
    Some(match c {
        'á' | 'à' | 'â' | 'ã' | 'ä' | 'å' | 'Á' | 'À' | 'Â' | 'Ã' | 'Ä' | 'Å' => "a",
        'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => "e",
        'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => "i",
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => "o",
        'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => "u",
        'ç' | 'Ç' => "c",
        'ñ' | 'Ñ' => "n",
        'ß' => "ss",
        _ => return None,
    })
}

/// Maximum length of a slug produced by [`slugify`].
pub const MAX_SLUG_LEN: usize = 40;

/// Converts an untrusted string into a portable directory-name fragment
/// containing only `[a-z0-9-]`. Never empty.
#[must_use]
pub fn slugify(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len().min(MAX_SLUG_LEN));
    let mut last_dash = true;
    for c in raw.chars() {
        let piece: Option<String> = if c.is_ascii_alphanumeric() {
            Some(c.to_ascii_lowercase().to_string())
        } else {
            fold_char(c).map(str::to_owned)
        };
        match piece {
            Some(p) => {
                out.push_str(&p);
                last_dash = false;
            }
            None if !last_dash => {
                out.push('-');
                last_dash = true;
            }
            None => {}
        }
        if out.len() >= MAX_SLUG_LEN {
            break;
        }
    }
    out.truncate(MAX_SLUG_LEN);
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() { "song".to_owned() } else { trimmed.to_owned() }
}

/// Song identifiers: `[a-z0-9-]{1,64}`, not starting with `-`.
/// IDs travel through the IPC boundary, so they are re-validated on use.
#[must_use]
pub fn is_valid_song_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && !id.starts_with('-')
        && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// File names stored inside a project (`stems/vocals.wav`, …).
/// Only `[a-z0-9_.-]` segments separated by `/`, no `..`, no leading dot.
#[must_use]
pub fn is_safe_relative_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.split('/').all(|seg| {
            !seg.is_empty()
                && !seg.starts_with('.')
                && seg.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-' | b'.'))
        })
}

/// Joins a project-relative name to `root`, refusing anything that could
/// escape it (absolute paths, `..`, drive prefixes, unusual characters).
/// If the target exists, its canonical path must also live under the
/// canonical root, defeating symlink escapes.
///
/// # Errors
/// [`RejectReason::OutsideWorkspace`] or [`RejectReason::InvalidName`].
pub fn resolve_within(root: &Path, relative: &str) -> Result<PathBuf> {
    if !is_safe_relative_name(relative) {
        return Err(RejectReason::InvalidName.into());
    }
    let rel = Path::new(relative);
    if !rel.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(RejectReason::OutsideWorkspace.into());
    }
    let joined = root.join(rel);
    ensure_inside(root, &joined)?;
    Ok(joined)
}

/// Verifies that `candidate` (if it exists) resolves inside `root`.
///
/// # Errors
/// [`RejectReason::OutsideWorkspace`] when it escapes.
pub fn ensure_inside(root: &Path, candidate: &Path) -> Result<()> {
    let canonical_root = root.canonicalize()?;
    match candidate.canonicalize() {
        Ok(real) if real.starts_with(&canonical_root) => Ok(()),
        Ok(_) => Err(RejectReason::OutsideWorkspace.into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Not created yet: the lexical check in callers already applies;
            // verify the parent instead.
            match candidate.parent() {
                Some(parent) if parent != candidate && parent.exists() => {
                    if parent.canonicalize()?.starts_with(&canonical_root) {
                        Ok(())
                    } else {
                        Err(RejectReason::OutsideWorkspace.into())
                    }
                }
                _ => Ok(()),
            }
        }
        Err(e) => Err(CoreError::Io(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::io::Write;

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        File::create(&p).unwrap().write_all(bytes).unwrap();
        p
    }

    const WAV_HEADER: &[u8] = b"RIFF\x24\x00\x00\x00WAVEfmt ";

    #[test]
    fn sniffs_known_containers() {
        assert_eq!(sniff_container(WAV_HEADER), Some(Container::Wav));
        assert_eq!(sniff_container(b"fLaC\0\0\0\0"), Some(Container::Flac));
        assert_eq!(sniff_container(b"OggS\0\0"), Some(Container::Ogg));
        assert_eq!(sniff_container(b"FORM\0\0\0\0AIFF"), Some(Container::Aiff));
        assert_eq!(sniff_container(b"\0\0\0\x20ftypM4A "), Some(Container::Mp4));
        assert_eq!(sniff_container(b"ID3\x04"), Some(Container::Id3Tagged));
        assert_eq!(sniff_container(&[0xFF, 0xFB, 0x90]), Some(Container::MpegFrames));
    }

    #[test]
    fn rejects_executables_and_scripts_disguised_as_audio() {
        assert_eq!(sniff_container(b"MZ\x90\x00"), None); // Windows PE
        assert_eq!(sniff_container(b"\x7fELF"), None); // Linux ELF
        assert_eq!(sniff_container(&[0xCF, 0xFA, 0xED, 0xFE]), None); // Mach-O
        assert_eq!(sniff_container(b"#!/bin/sh"), None);
        assert_eq!(sniff_container(b"<html>"), None);
        assert_eq!(sniff_container(b"PK\x03\x04"), None); // zip
        assert_eq!(sniff_container(b""), None);
    }

    #[test]
    fn open_audio_file_accepts_valid_and_rejects_attacks() {
        let dir = tempfile::tempdir().unwrap();
        let limits = Limits::default();

        let ok = write(dir.path(), "song.WAV", WAV_HEADER);
        let v = open_audio_file(&ok, &limits).unwrap();
        assert_eq!(v.extension, "wav");
        assert_eq!(v.container, Container::Wav);

        let exe = write(dir.path(), "song.mp3", b"MZ\x90\x00\x03\x00\x00\x00");
        assert!(matches!(
            open_audio_file(&exe, &limits),
            Err(CoreError::Rejected(RejectReason::UnrecognizedSignature))
        ));

        let script = write(dir.path(), "song.sh", b"#!/bin/sh\nrm -rf ~");
        assert!(matches!(
            open_audio_file(&script, &limits),
            Err(CoreError::Rejected(RejectReason::UnsupportedExtension))
        ));

        let empty = write(dir.path(), "empty.flac", b"");
        assert!(matches!(open_audio_file(&empty, &limits), Err(CoreError::Rejected(RejectReason::EmptyFile))));

        let tiny = Limits { max_file_bytes: 4, ..limits };
        assert!(matches!(open_audio_file(&ok, &tiny), Err(CoreError::Rejected(RejectReason::TooLarge))));

        let dir_named_like_audio = dir.path().join("folder.mp3");
        std::fs::create_dir(&dir_named_like_audio).unwrap();
        assert!(matches!(
            open_audio_file(&dir_named_like_audio, &limits),
            Err(CoreError::Rejected(RejectReason::NotARegularFile))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn open_audio_file_rejects_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let target = write(dir.path(), "real.wav", WAV_HEADER);
        let link = dir.path().join("link.wav");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(matches!(open_audio_file(&link, &Limits::default()), Err(CoreError::Rejected(RejectReason::Symlink))));
    }

    #[test]
    fn title_strips_bidi_and_control_characters() {
        assert_eq!(sanitize_title("evil\u{202E}3pm.exe"), "evil3pm.exe");
        assert_eq!(sanitize_title("a\u{0}b\nc\td"), "abcd");
        assert_eq!(sanitize_title("  Canção   do  Mar "), "Canção do Mar");
        assert_eq!(sanitize_title("\u{200B}\u{FEFF}"), "Untitled");
        assert_eq!(sanitize_title(&"x".repeat(500)).chars().count(), 120);
    }

    #[test]
    fn slugify_produces_portable_names() {
        assert_eq!(slugify("Für Elise (Live) — 2024"), "fur-elise-live-2024");
        assert_eq!(slugify("Canção Açaí"), "cancao-acai");
        assert_eq!(slugify("Straße"), "strasse");
        assert_eq!(slugify("../../etc/passwd"), "etc-passwd");
        assert_eq!(slugify("..\\..\\Windows\\System32"), "windows-system32");
        assert_eq!(slugify("CON"), "con");
        assert_eq!(slugify("🎸🥁"), "song");
        assert_eq!(slugify(""), "song");
    }

    #[test]
    fn song_id_validation() {
        assert!(is_valid_song_id("my-song-1a2b3c4d"));
        for bad in ["", "-x", "../x", "a/b", "a\\b", "A", "a b", "a.b", "a\0b", &"a".repeat(65)] {
            assert!(!is_valid_song_id(bad), "{bad:?} must be invalid");
        }
    }

    #[test]
    fn resolve_within_blocks_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        assert!(resolve_within(root, "stems/vocals.wav").is_ok());
        for bad in [
            "../secret",
            "stems/../../x",
            "/etc/passwd",
            "C:\\Windows\\win.ini",
            "\\\\server\\share",
            "stems//x",
            ".hidden",
            "stems/./x",
            "",
            "a\0b",
            "STEMS/X",
        ] {
            assert!(resolve_within(root, bad).is_err(), "{bad:?} must be rejected");
        }
    }

    #[cfg(unix)]
    #[test]
    fn resolve_within_blocks_symlink_escape() {
        let outside = tempfile::tempdir().unwrap();
        let ws = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), ws.path().join("stems")).unwrap();
        write(outside.path(), "x.wav", b"data");
        assert!(matches!(
            resolve_within(ws.path(), "stems/x.wav"),
            Err(CoreError::Rejected(RejectReason::OutsideWorkspace))
        ));
    }

    proptest! {
        #[test]
        fn slug_is_always_safe(raw in any::<String>()) {
            let s = slugify(&raw);
            prop_assert!(!s.is_empty() && s.len() <= MAX_SLUG_LEN);
            prop_assert!(s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'));
            prop_assert!(!s.starts_with('-') && !s.ends_with('-'));
        }

        #[test]
        fn title_never_contains_dangerous_chars(raw in any::<String>()) {
            let t = sanitize_title(&raw);
            prop_assert!(!t.chars().any(is_dangerous_char));
            prop_assert!(t.chars().count() <= 120);
        }

        #[test]
        fn resolve_within_never_escapes(rel in "[a-z./\\\\:]{0,24}") {
            let dir = tempfile::tempdir().unwrap();
            if let Ok(p) = resolve_within(dir.path(), &rel) {
                prop_assert!(p.starts_with(dir.path()));
                prop_assert!(Path::new(&rel).components().all(|c| matches!(c, Component::Normal(_))));
            }
        }

        #[test]
        fn sniff_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..64)) {
            let _ = sniff_container(&bytes);
        }
    }
}
