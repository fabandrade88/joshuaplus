//! User folders ("Band", "Church", "Practice"…) that organise songs.
//!
//! Folders are stored in `<workspace>/.joshua/folders.json`; a song belongs
//! to at most one folder (`SongProject::folder`). Every folder also gets a
//! drop directory `inbox/<folder-id>/`: files copied there are imported
//! straight into that folder.

use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, RejectReason, Result};
use crate::security::{is_valid_song_id, sanitize_title, slugify};

pub const FOLDERS_FILE: &str = "folders.json";
pub const MAX_FOLDERS: usize = 200;
const MAX_NAME_CHARS: usize = 60;
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
/// Most songs a folder playlist can order.
pub const MAX_PLAYLIST: usize = 5_000;

/// Palette keys understood by the UI.
pub const COLORS: &[&str] = &["amber", "rose", "violet", "green", "cyan", "slate"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Folder {
    /// Stable, filesystem-safe identifier (also the inbox sub-directory).
    pub id: String,
    pub name: String,
    pub color: String,
    pub created_unix: u64,
    /// Playlist order (song ids). Songs missing here play after, newest first.
    #[serde(default)]
    pub order: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FolderFile {
    folders: Vec<Folder>,
}

fn valid(f: &Folder) -> bool {
    is_valid_song_id(&f.id)
        && !f.name.is_empty()
        && f.name.chars().count() <= MAX_NAME_CHARS
        && !f.name.chars().any(char::is_control)
        && COLORS.contains(&f.color.as_str())
        && f.order.len() <= MAX_PLAYLIST
        && f.order.iter().all(|id| is_valid_song_id(id))
}

/// Cleans a user supplied folder name.
///
/// # Errors
/// [`RejectReason::InvalidName`] when nothing printable remains.
pub fn clean_name(raw: &str) -> Result<String> {
    let name = sanitize_title(raw);
    // `sanitize_title` falls back to "Untitled" when nothing printable is left.
    if name == "Untitled" && raw.trim() != "Untitled" {
        return Err(RejectReason::InvalidName.into());
    }
    Ok(name.chars().take(MAX_NAME_CHARS).collect::<String>().trim().to_owned())
}

/// Loads the folder list; a missing file means "no folders".
///
/// # Errors
/// Oversized, malformed or invalid files.
pub fn load(dir: &Path) -> Result<Vec<Folder>> {
    let path = dir.join(FOLDERS_FILE);
    let file = match crate::security::open_regular(&path) {
        Ok(f) => f,
        Err(CoreError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut text = String::new();
    file.take(MAX_FILE_BYTES + 1).read_to_string(&mut text)?;
    if text.len() as u64 > MAX_FILE_BYTES {
        return Err(RejectReason::TooLarge.into());
    }
    let parsed: FolderFile = serde_json::from_str(&text).map_err(|e| CoreError::Project(e.to_string()))?;
    if parsed.folders.len() > MAX_FOLDERS || !parsed.folders.iter().all(valid) {
        return Err(RejectReason::InvalidProject.into());
    }
    Ok(parsed.folders)
}

/// Atomically saves the folder list.
///
/// # Errors
/// I/O errors or invalid content.
pub fn save(dir: &Path, folders: &[Folder]) -> Result<()> {
    if folders.len() > MAX_FOLDERS || !folders.iter().all(valid) {
        return Err(RejectReason::InvalidProject.into());
    }
    let json = serde_json::to_vec_pretty(&FolderFile { folders: folders.to_vec() })
        .map_err(|e| CoreError::Project(e.to_string()))?;
    crate::security::atomic_write(dir, FOLDERS_FILE, &json, false)
}

/// Unique id derived from the name (`"Louvor Domingo"` → `louvor-domingo`).
#[must_use]
pub fn unique_id(name: &str, existing: &[Folder]) -> String {
    let base = slugify(name);
    let taken = |id: &str| existing.iter().any(|f| f.id == id) || matches!(id, "processed" | "failed");
    if !taken(&base) {
        return base;
    }
    (2..).map(|n| format!("{base}-{n}")).find(|c| !taken(c)).unwrap_or(base)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(id: &str) -> Folder {
        Folder { id: id.into(), name: "Banda".into(), color: "amber".into(), created_unix: 0, order: Vec::new() }
    }

    #[test]
    fn round_trip_and_validation() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load(dir.path()).unwrap().is_empty());
        save(dir.path(), &[folder("banda")]).unwrap();
        assert_eq!(load(dir.path()).unwrap(), vec![folder("banda")]);

        for bad in [
            folder("../x"),
            Folder { color: "red;}".into(), ..folder("a") },
            Folder { name: String::new(), ..folder("a") },
            Folder { order: vec!["../x".into()], ..folder("a") },
        ] {
            assert!(save(dir.path(), &[bad]).is_err());
        }
        std::fs::write(
            dir.path().join(FOLDERS_FILE),
            r#"{"folders":[{"id":"../../x","name":"x","color":"amber","createdUnix":0}]}"#,
        )
        .unwrap();
        assert!(load(dir.path()).is_err());
    }

    #[test]
    fn ids_are_unique_slugs_avoiding_reserved_inbox_names() {
        let existing = vec![folder("banda"), folder("banda-2")];
        assert_eq!(unique_id("Banda", &existing), "banda-3");
        assert_eq!(unique_id("Louvor Domingo", &existing), "louvor-domingo");
        assert_eq!(unique_id("Processed", &[]), "processed-2");
        assert_eq!(unique_id("../..", &[]), "song");
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  Ensaio\u{202E} Banda ").unwrap(), "Ensaio Banda");
        assert!(clean_name("   ").is_err());
        assert!(clean_name("\u{200B}\u{0}").is_err());
        assert_eq!(clean_name(&"x".repeat(200)).unwrap().chars().count(), 60);
    }
}
