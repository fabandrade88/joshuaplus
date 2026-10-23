//! Background import queue. Imports are CPU-heavy, so they run one at a
//! time on a dedicated worker thread and report progress through events.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use joshua_core::CoreError;
use joshua_core::import::{ImportOptions, ImportStage, import_song};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::error::ApiError;
use crate::state::{AppState, lock};

/// Most files accepted by a single drop.
pub const MAX_FILES_PER_REQUEST: usize = 100;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JobStatus {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

/// Event payload of `job-update`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobUpdate {
    pub id: u64,
    pub file_name: String,
    pub status: JobStatus,
    pub stage: Option<ImportStage>,
    pub progress: f32,
    pub error: Option<ApiError>,
    pub song_id: Option<String>,
    pub folder: Option<String>,
}

#[derive(Debug)]
struct Job {
    id: u64,
    path: PathBuf,
    folder: Option<String>,
    from_inbox: bool,
    cancel: Arc<AtomicBool>,
}

#[derive(Debug)]
pub struct JobQueue {
    tx: Mutex<Sender<Job>>,
    rx: Mutex<Option<Receiver<Job>>>,
    cancels: Mutex<HashMap<u64, Arc<AtomicBool>>>,
    pending: Mutex<HashSet<PathBuf>>,
    next_id: AtomicU64,
}

impl Default for JobQueue {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            tx: Mutex::new(tx),
            rx: Mutex::new(Some(rx)),
            cancels: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashSet::new()),
            next_id: AtomicU64::new(1),
        }
    }
}

fn file_name(path: &Path) -> String {
    joshua_core::security::sanitize_title(&path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default())
}

fn emit(app: &AppHandle, update: &JobUpdate) {
    let _ = app.emit("job-update", update);
}

impl JobQueue {
    /// Queues a file; returns `None` if it is already queued.
    pub fn enqueue(&self, app: &AppHandle, path: PathBuf, folder: Option<String>, from_inbox: bool) -> Option<u64> {
        if !lock(&self.pending).insert(path.clone()) {
            return None;
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let cancel = Arc::new(AtomicBool::new(false));
        lock(&self.cancels).insert(id, Arc::clone(&cancel));
        let update = JobUpdate {
            id,
            file_name: file_name(&path),
            status: JobStatus::Queued,
            stage: None,
            progress: 0.0,
            error: None,
            song_id: None,
            folder: folder.clone(),
        };
        if lock(&self.tx).send(Job { id, path: path.clone(), folder, from_inbox, cancel }).is_err() {
            lock(&self.pending).remove(&path);
            return None;
        }
        emit(app, &update);
        Some(id)
    }

    pub fn is_pending(&self, path: &Path) -> bool {
        lock(&self.pending).contains(path)
    }

    /// Requests cancellation; returns whether the job existed.
    pub fn cancel(&self, id: u64) -> bool {
        lock(&self.cancels).get(&id).is_some_and(|c| {
            c.store(true, Ordering::Relaxed);
            true
        })
    }

    /// Spawns the worker thread (once).
    pub fn start_worker(&self, app: AppHandle) {
        let Some(rx) = lock(&self.rx).take() else { return };
        let spawned = std::thread::Builder::new().name("joshua-import".into()).spawn(move || {
            for job in rx {
                run_job(&app, &job);
                let state = app.state::<AppState>();
                lock(&state.jobs.cancels).remove(&job.id);
                lock(&state.jobs.pending).remove(&job.path);
            }
        });
        if let Err(e) = spawned {
            log::error!("could not start import worker: {e}");
        }
    }
}

fn run_job(app: &AppHandle, job: &Job) {
    let state = app.state::<AppState>();
    let mut update = JobUpdate {
        id: job.id,
        file_name: file_name(&job.path),
        status: JobStatus::Running,
        stage: Some(ImportStage::Validating),
        progress: 0.0,
        error: None,
        song_id: None,
        folder: job.folder.clone(),
    };
    let fail = |mut update: JobUpdate, err: ApiError| {
        update.status = if err.code == "cancelled" { JobStatus::Cancelled } else { JobStatus::Failed };
        update.error = Some(err);
        emit(app, &update);
    };
    if job.cancel.load(Ordering::Relaxed) {
        return fail(update, CoreError::Cancelled.into());
    }
    let ws = match state.workspace() {
        Ok(ws) => ws,
        Err(e) => return fail(update, e),
    };

    let last = Mutex::new((Instant::now(), ImportStage::Validating));
    let base = update.clone();
    let progress = |stage: ImportStage, p: f32| {
        let mut last = lock(&last);
        if last.1 != stage || last.0.elapsed() >= PROGRESS_INTERVAL {
            *last = (Instant::now(), stage);
            emit(app, &JobUpdate { stage: Some(stage), progress: p.clamp(0.0, 1.0), ..base.clone() });
        }
    };

    // First studio import: fetch the engine model (once, verified).
    if state.needs_model() {
        progress(ImportStage::DownloadingModel, 0.0);
        if let Err(e) = state.ensure_model(&|p| progress(ImportStage::DownloadingModel, p), &job.cancel) {
            return fail(update, e);
        }
    }
    let separator = match state.separator() {
        Ok(s) => s,
        Err(e) => return fail(update, e),
    };
    emit(app, &update);

    let opts = ImportOptions {
        separator: separator.as_ref(),
        folder: job.folder.clone(),
        progress: &progress,
        cancel: &job.cancel,
    };
    let result = import_song(&ws, &job.path, &opts);

    if job.from_inbox
        && let Err(e) = ws.archive_inbox_file(&job.path, result.is_ok())
    {
        log::warn!("could not archive inbox file: {e}");
    }
    match result {
        Ok(project) => {
            update.status = JobStatus::Done;
            update.stage = Some(ImportStage::Finalizing);
            update.progress = 1.0;
            update.song_id = Some(project.id);
            emit(app, &update);
            let _ = app.emit("library-changed", ());
        }
        Err(e) => {
            log::warn!("import failed: {e}");
            fail(update, e.into());
        }
    }
}
