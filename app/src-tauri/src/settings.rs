//! Persistent user preferences (`settings.json` in the OS config dir).

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const LOCALES: &[&str] = &["en", "pt-BR", "de"];
const MAX_SETTINGS_BYTES: u64 = 64 * 1024;

/// Which separation engine new imports use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SeparatorChoice {
    /// Built-in neural engine (offline, 6 stems). Default.
    #[default]
    #[serde(alias = "auto", alias = "demucs")]
    Studio,
    /// Built-in DSP (instant preview quality, 4 stems).
    #[serde(alias = "spectral")]
    Quick,
    /// External service (LALAL.AI) — requires the user's key and consent.
    Cloud,
}

/// One configured audio input (interface, e-drum USB audio, USB mic…).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputConfig {
    pub device_id: String,
    pub name: String,
    /// First hardware channel (0-based) routed to monitor/recording.
    pub first_channel: u16,
    pub stereo: bool,
    pub monitor: bool,
    pub gain_db: f32,
}

/// MIDI e-drum setup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct DrumSettings {
    pub port_id: Option<String>,
    pub brand: String,
    /// Note map (preset of the brand, refined by MIDI learn).
    pub map: joshua_midi::NoteMap,
}

impl Default for DrumSettings {
    fn default() -> Self {
        Self { port_id: None, brand: "generic".into(), map: joshua_midi::NoteMap::default() }
    }
}

/// Most audio inputs kept in the settings.
pub const MAX_INPUTS: usize = joshua_engine::input::MAX_INPUTS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Settings {
    pub workspace: Option<PathBuf>,
    pub locale: Option<String>,
    pub separator: SeparatorChoice,
    pub output_device: Option<String>,
    /// LALAL.AI license key. Never sent to the UI.
    pub cloud_key: Option<String>,
    pub inputs: Vec<InputConfig>,
    pub drums: DrumSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            workspace: None,
            locale: None,
            separator: SeparatorChoice::Studio,
            output_device: None,
            cloud_key: None,
            inputs: Vec::new(),
            drums: DrumSettings::default(),
        }
    }
}

impl Settings {
    /// Loads settings; any problem (missing, oversized, corrupt) yields defaults.
    pub fn load(path: &Path) -> Self {
        let Ok(file) = joshua_core::security::open_regular(path) else { return Self::default() };
        let mut text = String::new();
        if file.take(MAX_SETTINGS_BYTES + 1).read_to_string(&mut text).is_err()
            || text.len() as u64 > MAX_SETTINGS_BYTES
        {
            return Self::default();
        }
        let mut s: Self = serde_json::from_str(&text).unwrap_or_default();
        s.sanitize();
        s
    }

    /// Atomically writes the settings file. It is created readable only by
    /// the user from the first byte (it may hold an API key).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let dir = path.parent().ok_or_else(|| std::io::Error::other("no parent directory"))?;
        std::fs::create_dir_all(dir)?;
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("settings.json");
        joshua_core::security::atomic_write(dir, name, &json, true).map_err(|e| std::io::Error::other(e.to_string()))
    }

    fn sanitize(&mut self) {
        if self.locale.as_deref().is_some_and(|l| !LOCALES.contains(&l)) {
            self.locale = None;
        }
        if self.workspace.as_deref().is_some_and(|w| !w.is_absolute()) {
            self.workspace = None;
        }
        if self.output_device.as_deref().is_some_and(|d| d.len() > 512) {
            self.output_device = None;
        }
        if self.cloud_key.as_deref().is_some_and(|k| !joshua_ai::cloud::is_plausible_key(k)) {
            self.cloud_key = None;
        }
        self.inputs.truncate(MAX_INPUTS);
        self.inputs.retain(|i| i.device_id.len() <= 512 && i.name.len() <= 512);
        for i in &mut self.inputs {
            i.first_channel = i.first_channel.min(63);
            if !i.gain_db.is_finite() {
                i.gain_db = 0.0;
            }
            i.gain_db = i.gain_db.clamp(-60.0, 12.0);
        }
        if self.drums.port_id.as_deref().is_some_and(|p| p.len() > 512) {
            self.drums.port_id = None;
        }
        self.drums.map.notes.retain(|n, _| *n <= 127);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_sanitize() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let s = Settings { locale: Some("de".into()), workspace: Some(dir.path().to_path_buf()), ..Default::default() };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);

        std::fs::write(&path, r#"{"locale":"xx","workspace":"relative/path","cloudKey":"bad key!"}"#).unwrap();
        let s = Settings::load(&path);
        assert_eq!((s.locale, s.workspace, s.cloud_key), (None, None, None));

        std::fs::write(&path, r#"{"separator":"auto"}"#).unwrap();
        assert_eq!(Settings::load(&path).separator, SeparatorChoice::Studio, "old settings migrate");

        std::fs::write(&path, "not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        std::fs::write(&path, vec![b' '; 100_000]).unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
    }

    #[cfg(unix)]
    #[test]
    fn settings_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        Settings::default().save(&path).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }
}
