//! Watches `<workspace>/inbox` by polling. Polling (instead of native file
//! notifications) behaves identically on every OS, works on network drives
//! and adds no dependencies. A file is imported once its size and
//! modification time are unchanged between two polls, i.e. the copy that
//! put it there has finished.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use joshua_core::security::allowed_extension;
use tauri::{AppHandle, Manager};

use crate::state::AppState;

const POLL_INTERVAL: Duration = Duration::from_secs(2);
const MAX_NEW_PER_POLL: usize = 50;

type Fingerprint = (u64, Option<SystemTime>);

pub fn start(app: AppHandle) {
    let spawned = std::thread::Builder::new().name("joshua-inbox".into()).spawn(move || {
        let mut seen: HashMap<PathBuf, Fingerprint> = HashMap::new();
        let mut queued: HashSet<(PathBuf, Fingerprint)> = HashSet::new();
        loop {
            std::thread::sleep(POLL_INTERVAL);
            poll(&app, &mut seen, &mut queued);
        }
    });
    if let Err(e) = spawned {
        log::error!("could not start inbox watcher: {e}");
    }
}

/// `queued` remembers every (file, version) already imported, so a file that
/// could not be archived afterwards is never imported again in a loop.
fn poll(app: &AppHandle, seen: &mut HashMap<PathBuf, Fingerprint>, queued: &mut HashSet<(PathBuf, Fingerprint)>) {
    let state = app.state::<AppState>();
    let Ok(ws) = state.workspace() else { return };
    // The inbox root files into "no folder"; `inbox/<folder-id>/` into that folder.
    let mut dirs = vec![(ws.inbox_dir(), None)];
    for folder in ws.list_folders().unwrap_or_default() {
        if let Ok(dir) = ws.folder_inbox(&folder.id) {
            let _ = std::fs::create_dir_all(&dir);
            // Never follow a folder inbox that is a symlink.
            if std::fs::symlink_metadata(&dir).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink()) {
                dirs.push((dir, Some(folder.id)));
            }
        }
    }
    let mut current = HashMap::new();
    let mut new_jobs = 0;
    for (dir, folder) in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let hidden = entry.file_name().to_string_lossy().starts_with('.');
            // `DirEntry::metadata` does not follow symlinks.
            let Ok(meta) = entry.metadata() else { continue };
            if hidden || !meta.is_file() || meta.len() == 0 || allowed_extension(&path).is_err() {
                continue;
            }
            let fp = (meta.len(), meta.modified().ok());
            let stable = seen.get(&path) == Some(&fp);
            current.insert(path.clone(), fp);
            if stable
                && new_jobs < MAX_NEW_PER_POLL
                && !state.jobs.is_pending(&path)
                && !queued.contains(&(path.clone(), fp))
                && state.jobs.enqueue(app, path.clone(), folder.clone(), true).is_some()
            {
                queued.insert((path, fp));
                new_jobs += 1;
            }
        }
    }
    *seen = current;
}
