//! Application state shared by commands and background threads.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};

use joshua_ai::cloud::LalalSeparator;
use joshua_ai::separator::Accelerator;
use joshua_ai::{HTDEMUCS_6S, ModelStore, NeuralSeparator};
use joshua_core::mixer::SharedState;
use joshua_core::project::SongProject;
use joshua_core::separation::Separator;
use joshua_core::separation::spectral::SpectralSeparator;
use joshua_core::workspace::Workspace;
use joshua_engine::Engine;

use crate::error::{ApiError, ApiResult};
use crate::jobs::JobQueue;
use crate::settings::{SeparatorChoice, Settings};

/// The song currently open in the mixer.
#[derive(Debug)]
pub struct LoadedSong {
    pub project: SongProject,
    /// Mixer changes not yet written to disk.
    pub dirty: bool,
    /// Sample rate of the engine session (frames ↔ seconds).
    pub session_rate: u32,
}

#[derive(Debug)]
pub struct AppState {
    pub settings: Mutex<Settings>,
    pub settings_path: PathBuf,
    pub models: ModelStore,
    pub workspace: RwLock<Option<Workspace>>,
    pub engine: Mutex<Option<Engine>>,
    pub engine_error: Mutex<Option<String>>,
    /// Transport/mixer atomics of the running engine (read by the ticker).
    pub shared: RwLock<Option<Arc<SharedState>>>,
    pub current: Mutex<Option<LoadedSong>>,
    pub jobs: JobQueue,
    /// Kept alive between imports so the model loads only once.
    neural: Mutex<Option<Arc<NeuralSeparator>>>,
    /// Serialises model downloads (import worker vs. settings button).
    model_lock: Mutex<()>,
    /// Audio inputs and MIDI e-drums.
    pub devices: crate::devices::Devices,
    /// Serialises song loads so engine and `current` always match.
    pub load_lock: Mutex<()>,
    /// The take recording in progress, if any.
    pub recording: Mutex<Option<crate::recorder::ActiveRecording>>,
    /// Video clips being saved by the video studio.
    pub videos: crate::video::VideoSaves,
}

/// Locks ignoring poisoning: a panicked holder never leaves our data in an
/// inconsistent state (all updates are single assignments).
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl AppState {
    pub fn new(settings_path: PathBuf, models_dir: PathBuf, jobs: JobQueue) -> Self {
        let settings = Settings::load(&settings_path);
        let workspace = settings
            .workspace
            .as_deref()
            .and_then(|p| Workspace::open(p, joshua_core::security::Limits::default()).ok());
        Self {
            settings: Mutex::new(settings),
            settings_path,
            models: ModelStore::new(models_dir),
            workspace: RwLock::new(workspace),
            engine: Mutex::new(None),
            engine_error: Mutex::new(None),
            shared: RwLock::new(None),
            current: Mutex::new(None),
            jobs,
            neural: Mutex::new(None),
            model_lock: Mutex::new(()),
            devices: crate::devices::Devices::default(),
            load_lock: Mutex::new(()),
            recording: Mutex::new(None),
            videos: crate::video::VideoSaves::default(),
        }
    }

    pub fn workspace(&self) -> ApiResult<Workspace> {
        self.workspace.read().unwrap_or_else(PoisonError::into_inner).clone().ok_or_else(ApiError::no_workspace)
    }

    pub fn shared(&self) -> ApiResult<Arc<SharedState>> {
        self.shared.read().unwrap_or_else(PoisonError::into_inner).clone().ok_or_else(ApiError::no_audio)
    }

    pub fn save_settings(&self) -> ApiResult<()> {
        lock(&self.settings).save(&self.settings_path).map_err(|e| ApiError::new("io", e.kind().to_string()))
    }

    pub fn model_installed(&self) -> bool {
        self.models.is_installed(&HTDEMUCS_6S)
    }

    /// Downloads and verifies the studio model unless already present.
    pub fn ensure_model(&self, progress: &dyn Fn(f32), cancel: &AtomicBool) -> ApiResult<()> {
        let _guard = lock(&self.model_lock);
        self.models.install(&HTDEMUCS_6S, progress, cancel).map(|_| ()).map_err(|e| match e {
            joshua_ai::AiError::Cancelled => ApiError::new("cancelled", "cancelled"),
            other => ApiError::new("model_download", other.to_string()),
        })
    }

    /// Whether imports need the studio model first.
    pub fn needs_model(&self) -> bool {
        lock(&self.settings).separator == SeparatorChoice::Studio && !self.model_installed()
    }

    /// Starts (or restarts) the audio engine with the configured device.
    pub fn start_engine(&self) {
        let device = lock(&self.settings).output_device.clone();
        let mut engine = lock(&self.engine);
        *engine = None; // stop the old stream first
        let result = Engine::start(device.as_deref()).or_else(|e| {
            log::warn!("output device unavailable ({e}), falling back to default");
            Engine::start(None)
        });
        match result {
            Ok(e) => {
                log::info!("audio: {:?}", e.info());
                *self.shared.write().unwrap_or_else(PoisonError::into_inner) = Some(Arc::clone(e.shared()));
                *lock(&self.engine_error) = None;
                *engine = Some(e);
            }
            Err(e) => {
                log::error!("audio engine failed: {e}");
                *self.shared.write().unwrap_or_else(PoisonError::into_inner) = None;
                *lock(&self.engine_error) = Some(e.to_string());
            }
        }
    }

    /// The separator chosen in the settings. The studio engine requires the
    /// model to be installed (the import worker downloads it first).
    pub fn separator(&self) -> ApiResult<Arc<dyn Separator>> {
        let (choice, key) = {
            let s = lock(&self.settings);
            (s.separator, s.cloud_key.clone())
        };
        match choice {
            SeparatorChoice::Quick => Ok(Arc::new(SpectralSeparator::default())),
            SeparatorChoice::Cloud => {
                let key = key.ok_or_else(|| ApiError::new("cloud_key_missing", "no cloud API key"))?;
                Ok(Arc::new(LalalSeparator::new(&key).map_err(|e| ApiError::new("cloud", e.to_string()))?))
            }
            SeparatorChoice::Studio => {
                if !self.model_installed() {
                    return Err(ApiError::new("model_missing", "studio engine not installed"));
                }
                let mut cached = lock(&self.neural);
                let sep = cached
                    .get_or_insert_with(|| {
                        Arc::new(NeuralSeparator::new(self.models.path(&HTDEMUCS_6S), Accelerator::Auto))
                    })
                    .clone();
                Ok(sep)
            }
        }
    }
}
