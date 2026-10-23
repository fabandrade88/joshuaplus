//! The user's workspace folder.
//!
//! ```text
//! <workspace>/
//!   inbox/            ← drop songs here (watched by the app)
//!     processed/      ← originals are moved here after import
//!     failed/         ← files that could not be imported
//!   songs/<id>/       ← one directory per song
//!     project.json
//!     source.<ext>
//!     stems/<stem>.wav
//!   .joshua/tmp/      ← private scratch space
//! ```

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{CoreError, RejectReason, Result};
use crate::folders::{self, COLORS, Folder};
use crate::project::SongProject;
use crate::security::{Limits, ensure_inside, is_valid_song_id};

pub const INBOX_DIR: &str = "inbox";
pub const PROCESSED_DIR: &str = "processed";
pub const FAILED_DIR: &str = "failed";
pub const SONGS_DIR: &str = "songs";
pub const PRIVATE_DIR: &str = ".joshua";

/// Lightweight listing entry.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SongSummary {
    pub id: String,
    pub title: String,
    pub bpm: f64,
    pub duration_secs: f64,
    pub separator: String,
    pub stems: usize,
    pub created_unix: u64,
    pub folder: Option<String>,
    pub artist: Option<String>,
    pub has_cover: bool,
}

impl From<&SongProject> for SongSummary {
    fn from(p: &SongProject) -> Self {
        Self {
            id: p.id.clone(),
            title: p.title.clone(),
            bpm: p.analysis.bpm,
            duration_secs: p.duration_secs,
            separator: p.separator.clone(),
            stems: p.tracks.len(),
            created_unix: p.created_unix,
            folder: p.folder.clone(),
            artist: p.metadata.artist.clone(),
            has_cover: p.has_cover,
        }
    }
}

/// `true` for a real directory (not a symlink to one).
fn real_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
}

/// An opened workspace with a canonical root.
#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
    limits: Limits,
}

impl Workspace {
    /// Opens (creating if necessary) a workspace at `root`.
    ///
    /// # Errors
    /// I/O errors, or if `root` exists but is not a directory.
    pub fn open(root: &Path, limits: Limits) -> Result<Self> {
        if root.exists() && !root.is_dir() {
            return Err(RejectReason::NotARegularFile.into());
        }
        std::fs::create_dir_all(root)?;
        let root = root.canonicalize()?;
        for sub in [
            PathBuf::from(INBOX_DIR),
            Path::new(INBOX_DIR).join(PROCESSED_DIR),
            Path::new(INBOX_DIR).join(FAILED_DIR),
            PathBuf::from(SONGS_DIR),
            PathBuf::from(PRIVATE_DIR),
            Path::new(PRIVATE_DIR).join("tmp"),
        ] {
            let dir = root.join(sub);
            std::fs::create_dir_all(&dir)?;
            // A shared/tampered workspace could make these links to anywhere
            // (e.g. songs -> ~): refuse instead of writing/deleting through them.
            if !real_dir(&dir) {
                return Err(RejectReason::OutsideWorkspace.into());
            }
        }
        Ok(Self { root, limits })
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub const fn limits(&self) -> &Limits {
        &self.limits
    }

    #[must_use]
    pub fn inbox_dir(&self) -> PathBuf {
        self.root.join(INBOX_DIR)
    }

    #[must_use]
    pub fn songs_dir(&self) -> PathBuf {
        self.root.join(SONGS_DIR)
    }

    #[must_use]
    pub fn tmp_dir(&self) -> PathBuf {
        self.root.join(PRIVATE_DIR).join("tmp")
    }

    /// Directory of an existing or future song. Validates the ID.
    ///
    /// # Errors
    /// [`RejectReason::InvalidName`] for malformed IDs,
    /// [`RejectReason::OutsideWorkspace`] for symlink escapes.
    pub fn song_dir(&self, id: &str) -> Result<PathBuf> {
        if !is_valid_song_id(id) {
            return Err(RejectReason::InvalidName.into());
        }
        let dir = self.songs_dir().join(id);
        ensure_inside(&self.songs_dir(), &dir)?;
        Ok(dir)
    }

    /// Loads a song's project file.
    ///
    /// # Errors
    /// [`CoreError::NotFound`] or validation errors.
    pub fn load_project(&self, id: &str) -> Result<SongProject> {
        let dir = self.song_dir(id)?;
        if !dir.is_dir() {
            return Err(CoreError::NotFound);
        }
        let project = SongProject::load(&dir, &self.limits)?;
        if project.id != id {
            return Err(RejectReason::InvalidProject.into());
        }
        Ok(project)
    }

    /// Persists a project into its song directory.
    ///
    /// # Errors
    /// Validation or I/O errors.
    pub fn save_project(&self, project: &SongProject) -> Result<()> {
        let dir = self.song_dir(&project.id)?;
        if !dir.is_dir() {
            return Err(CoreError::NotFound);
        }
        project.save(&dir)
    }

    /// All valid songs, newest first. Invalid folders are skipped.
    ///
    /// # Errors
    /// I/O errors reading the songs directory.
    pub fn list_songs(&self) -> Result<Vec<SongSummary>> {
        let mut songs = Vec::new();
        for entry in std::fs::read_dir(self.songs_dir())? {
            let entry = entry?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else { continue };
            if !is_valid_song_id(&name) || !entry.file_type()?.is_dir() {
                continue; // staging dirs, symlinks, stray files
            }
            if let Ok(p) = self.load_project(&name) {
                songs.push(SongSummary::from(&p));
            }
        }
        songs.sort_by(|a, b| b.created_unix.cmp(&a.created_unix).then_with(|| a.title.cmp(&b.title)));
        Ok(songs)
    }

    /// Permanently deletes a song directory.
    ///
    /// # Errors
    /// Invalid ID, not found, or I/O errors.
    pub fn delete_song(&self, id: &str) -> Result<()> {
        let dir = self.song_dir(id)?;
        if !real_dir(&self.songs_dir()) || !real_dir(&dir) {
            return Err(RejectReason::NotARegularFile.into());
        }
        // Only delete what is verifiably one of our songs.
        self.load_project(id)?;
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    fn private_dir(&self) -> PathBuf {
        self.root.join(PRIVATE_DIR)
    }

    /// All user folders, in creation order.
    ///
    /// # Errors
    /// Corrupt folder file or I/O errors.
    pub fn list_folders(&self) -> Result<Vec<Folder>> {
        folders::load(&self.private_dir())
    }

    /// Drop directory of a folder (`inbox/<id>`).
    ///
    /// # Errors
    /// Invalid id.
    pub fn folder_inbox(&self, id: &str) -> Result<PathBuf> {
        if !is_valid_song_id(id) {
            return Err(RejectReason::InvalidName.into());
        }
        let dir = self.inbox_dir().join(id);
        if dir.exists() && !real_dir(&dir) {
            return Err(RejectReason::OutsideWorkspace.into());
        }
        Ok(dir)
    }

    /// Creates a folder (and its inbox drop directory).
    ///
    /// # Errors
    /// Invalid name/color, too many folders, I/O errors.
    pub fn create_folder(&self, name: &str, color: &str) -> Result<Folder> {
        let name = folders::clean_name(name)?;
        let mut list = self.list_folders()?;
        if list.len() >= folders::MAX_FOLDERS {
            return Err(RejectReason::TooLarge.into());
        }
        let color = if COLORS.contains(&color) { color } else { COLORS[list.len() % COLORS.len()] };
        let folder = Folder {
            order: Vec::new(),
            id: folders::unique_id(&name, &list),
            name,
            color: color.to_owned(),
            created_unix: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        };
        std::fs::create_dir_all(self.folder_inbox(&folder.id)?)?;
        list.push(folder.clone());
        folders::save(&self.private_dir(), &list)?;
        Ok(folder)
    }

    /// Renames and/or recolours a folder (its id never changes).
    ///
    /// # Errors
    /// Unknown folder or invalid values.
    pub fn update_folder(&self, id: &str, name: Option<&str>, color: Option<&str>) -> Result<Folder> {
        let mut list = self.list_folders()?;
        let folder = list.iter_mut().find(|f| f.id == id).ok_or(CoreError::NotFound)?;
        if let Some(n) = name {
            folder.name = folders::clean_name(n)?;
        }
        if let Some(c) = color {
            if !COLORS.contains(&c) {
                return Err(RejectReason::InvalidName.into());
            }
            c.clone_into(&mut folder.color);
        }
        let updated = folder.clone();
        folders::save(&self.private_dir(), &list)?;
        Ok(updated)
    }

    /// Deletes a folder. Its songs are kept and become unfiled; the inbox
    /// directory is removed only if empty.
    ///
    /// # Errors
    /// Unknown folder or I/O errors.
    pub fn delete_folder(&self, id: &str) -> Result<()> {
        let mut list = self.list_folders()?;
        let before = list.len();
        list.retain(|f| f.id != id);
        if list.len() == before {
            return Err(CoreError::NotFound);
        }
        for song in self.list_songs()?.into_iter().filter(|s| s.folder.as_deref() == Some(id)) {
            self.move_song(&song.id, None)?;
        }
        folders::save(&self.private_dir(), &list)?;
        let _ = std::fs::remove_dir(self.folder_inbox(id)?); // fails harmlessly if not empty
        Ok(())
    }

    /// Saves the playlist order of a folder. Unknown or foreign ids are
    /// dropped; songs of the folder missing from `order` keep playing after.
    ///
    /// # Errors
    /// Unknown folder or I/O errors.
    pub fn reorder_folder(&self, id: &str, order: &[String]) -> Result<Vec<String>> {
        let members: std::collections::HashSet<String> =
            self.list_songs()?.into_iter().filter(|s| s.folder.as_deref() == Some(id)).map(|s| s.id).collect();
        let mut seen = std::collections::HashSet::new();
        let cleaned: Vec<String> =
            order.iter().filter(|s| members.contains(*s) && seen.insert((*s).clone())).cloned().collect();
        let mut list = self.list_folders()?;
        let folder = list.iter_mut().find(|f| f.id == id).ok_or(CoreError::NotFound)?;
        folder.order.clone_from(&cleaned);
        folders::save(&self.private_dir(), &list)?;
        Ok(cleaned)
    }

    /// Renames a song (display title only; the folder name never changes).
    ///
    /// # Errors
    /// Empty/invalid title, unknown song or I/O errors.
    pub fn rename_song(&self, id: &str, title: &str) -> Result<SongProject> {
        let clean = crate::security::sanitize_title(title);
        if clean == "Untitled" && title.trim() != "Untitled" {
            return Err(RejectReason::InvalidName.into());
        }
        let mut project = self.load_project(id)?;
        project.title = clean;
        project.title_edited = true;
        self.save_project(&project)?;
        Ok(project)
    }

    /// Reads tags/cover for a song imported before metadata support.
    /// Returns `true` if the project changed.
    ///
    /// # Errors
    /// Unknown song or I/O errors.
    pub fn backfill_metadata(&self, id: &str) -> Result<bool> {
        let mut project = self.load_project(id)?;
        if project.metadata_scanned {
            return Ok(false);
        }
        let dir = self.song_dir(id)?;
        let source = crate::security::resolve_within(&dir, &project.source_file)?;
        let tags = crate::metadata::read_tags(&source).unwrap_or_default();
        project.has_cover =
            tags.picture.as_deref().is_some_and(|p| crate::metadata::write_cover(p, &dir).unwrap_or(false));
        if !project.title_edited {
            if let Some(t) = &tags.metadata.title {
                project.title.clone_from(t);
            }
        }
        project.metadata = tags.metadata;
        project.metadata_scanned = true;
        self.save_project(&project)?;
        Ok(true)
    }

    /// Path of a song's cover image or thumbnail, if it has one.
    ///
    /// # Errors
    /// Invalid song id.
    pub fn cover_path(&self, id: &str, thumbnail: bool) -> Result<Option<PathBuf>> {
        let name = if thumbnail { crate::metadata::THUMB_FILE } else { crate::metadata::COVER_FILE };
        let path = crate::security::resolve_within(&self.song_dir(id)?, name)?;
        Ok(std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_file()).then_some(path))
    }

    /// Files a song into a folder (or unfiles it with `None`).
    ///
    /// # Errors
    /// Unknown song/folder.
    pub fn move_song(&self, song_id: &str, folder: Option<&str>) -> Result<SongProject> {
        if let Some(f) = folder {
            if !self.list_folders()?.iter().any(|x| x.id == f) {
                return Err(CoreError::NotFound);
            }
        }
        let mut project = self.load_project(song_id)?;
        project.folder = folder.map(str::to_owned);
        self.save_project(&project)?;
        Ok(project)
    }

    /// Moves a file out of the inbox into `processed/` or `failed/`,
    /// never overwriting anything.
    ///
    /// # Errors
    /// I/O errors, or if `file` is not directly inside the inbox.
    pub fn archive_inbox_file(&self, file: &Path, succeeded: bool) -> Result<PathBuf> {
        let inbox = self.inbox_dir();
        let parent = file.parent().map(Path::canonicalize).transpose()?;
        let in_inbox = parent.as_deref() == Some(inbox.as_path())
            || parent.as_deref().and_then(Path::parent) == Some(inbox.as_path());
        if !in_inbox {
            return Err(RejectReason::OutsideWorkspace.into());
        }
        let name = file.file_name().ok_or(RejectReason::InvalidName)?;
        let target_dir = inbox.join(if succeeded { PROCESSED_DIR } else { FAILED_DIR });
        let mut target = target_dir.join(name);
        let mut n = 1;
        while target.exists() {
            let stem = Path::new(name).file_stem().and_then(|s| s.to_str()).unwrap_or("file");
            let ext = Path::new(name).extension().and_then(|s| s.to_str()).unwrap_or("");
            target = target_dir.join(format!("{stem} ({n}).{ext}"));
            n += 1;
        }
        std::fs::rename(file, &target)?;
        Ok(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::tests::sample_project;

    #[test]
    fn creates_layout_and_lists_songs() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(&dir.path().join("ws"), Limits::default()).unwrap();
        assert!(ws.inbox_dir().join(PROCESSED_DIR).is_dir());
        assert!(ws.tmp_dir().is_dir());

        let p = sample_project();
        std::fs::create_dir_all(ws.song_dir(&p.id).unwrap()).unwrap();
        ws.save_project(&p).unwrap();
        // Noise that must be ignored.
        std::fs::create_dir_all(ws.songs_dir().join(".staging-x")).unwrap();
        std::fs::create_dir_all(ws.songs_dir().join("broken")).unwrap();
        std::fs::write(ws.songs_dir().join("file.txt"), "x").unwrap();

        let songs = ws.list_songs().unwrap();
        assert_eq!(songs.len(), 1);
        assert_eq!(songs[0].id, p.id);
        assert_eq!(ws.load_project(&p.id).unwrap(), p);
    }

    #[test]
    fn project_id_must_match_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(dir.path(), Limits::default()).unwrap();
        let p = sample_project();
        let other = ws.song_dir("other-song").unwrap();
        std::fs::create_dir_all(&other).unwrap();
        p.save(&other).unwrap();
        assert!(ws.load_project("other-song").is_err());
    }

    #[test]
    fn invalid_ids_never_touch_the_filesystem() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(dir.path(), Limits::default()).unwrap();
        std::fs::write(dir.path().join("keep.txt"), "x").unwrap();
        for id in ["..", "../..", "/", "", "a/../../b", "C:\\"] {
            assert!(ws.delete_song(id).is_err());
            assert!(ws.load_project(id).is_err());
        }
        assert!(dir.path().join("keep.txt").exists());
    }

    #[test]
    fn archives_inbox_files_without_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(dir.path(), Limits::default()).unwrap();
        for _ in 0..2 {
            let f = ws.inbox_dir().join("a.mp3");
            std::fs::write(&f, "x").unwrap();
            ws.archive_inbox_file(&f, true).unwrap();
        }
        assert!(ws.inbox_dir().join(PROCESSED_DIR).join("a.mp3").exists());
        assert!(ws.inbox_dir().join(PROCESSED_DIR).join("a (1).mp3").exists());
        let outside = dir.path().join("x.mp3");
        std::fs::write(&outside, "x").unwrap();
        assert!(ws.archive_inbox_file(&outside, true).is_err());
    }

    #[test]
    fn folders_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let ws = Workspace::open(dir.path(), Limits::default()).unwrap();
        let band = ws.create_folder("Banda do Domingo", "rose").unwrap();
        assert_eq!(band.id, "banda-do-domingo");
        assert!(ws.folder_inbox(&band.id).unwrap().is_dir());
        let again = ws.create_folder("Banda do Domingo", "nope").unwrap();
        assert_eq!(again.id, "banda-do-domingo-2");
        assert!(COLORS.contains(&again.color.as_str()));

        let p = sample_project();
        std::fs::create_dir_all(ws.song_dir(&p.id).unwrap()).unwrap();
        ws.save_project(&p).unwrap();
        assert_eq!(ws.move_song(&p.id, Some(&band.id)).unwrap().folder.as_deref(), Some("banda-do-domingo"));
        assert!(ws.move_song(&p.id, Some("missing")).is_err());

        let r = ws.rename_song(&p.id, "  Nova\u{202E} Versão ").unwrap();
        assert_eq!((r.title.as_str(), r.title_edited), ("Nova Versão", true));
        assert!(ws.rename_song(&p.id, "\u{200B}").is_err());

        let renamed = ws.update_folder(&band.id, Some("Igreja"), Some("green")).unwrap();
        assert_eq!((renamed.id.as_str(), renamed.name.as_str()), ("banda-do-domingo", "Igreja"));
        assert!(ws.update_folder(&band.id, None, Some("url(x)")).is_err());

        let saved = ws.reorder_folder(&band.id, &[p.id.clone(), "ghost".into(), p.id.clone()]).unwrap();
        assert_eq!(saved, vec![p.id.clone()], "unknown and duplicate ids are dropped");
        assert_eq!(ws.list_folders().unwrap()[0].order, vec![p.id.clone()]);

        ws.delete_folder(&band.id).unwrap();
        assert_eq!(ws.list_folders().unwrap().len(), 1);
        assert_eq!(ws.load_project(&p.id).unwrap().folder, None, "songs survive folder deletion");
        assert!(ws.delete_folder("../..").is_err());
    }

    #[test]
    fn rejects_file_as_root() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("file");
        std::fs::write(&f, "x").unwrap();
        assert!(Workspace::open(&f, Limits::default()).is_err());
    }
}
