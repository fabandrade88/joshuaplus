//! One-off background job: reads tags and cover art for songs imported
//! before metadata support existed.

use tauri::{AppHandle, Emitter, Manager};

use crate::state::{AppState, lock};

pub fn start(app: AppHandle) {
    let spawned = std::thread::Builder::new().name("joshua-metadata".into()).spawn(move || {
        let state = app.state::<AppState>();
        let Ok(ws) = state.workspace() else { return };
        let Ok(songs) = ws.list_songs() else { return };
        let mut changed = false;
        for song in songs {
            match ws.backfill_metadata(&song.id) {
                Ok(true) => {
                    changed = true;
                    // Keep the open song's in-memory project in sync so an
                    // autosave cannot overwrite the new tags.
                    if let Ok(fresh) = ws.load_project(&song.id)
                        && let Some(cur) = lock(&state.current).as_mut().filter(|c| c.project.id == song.id)
                    {
                        cur.project.metadata = fresh.metadata;
                        cur.project.has_cover = fresh.has_cover;
                        cur.project.metadata_scanned = true;
                        cur.project.title = fresh.title;
                    }
                }
                Ok(false) => {}
                Err(e) => log::warn!("metadata backfill failed: {e}"),
            }
        }
        if changed {
            let _ = app.emit("library-changed", ());
        }
    });
    if let Err(e) = spawned {
        log::error!("could not start metadata backfill: {e}");
    }
}
