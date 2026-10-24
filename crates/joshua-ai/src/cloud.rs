//! Optional cloud separation through [LALAL.AI](https://www.lalal.ai) (API v1).
//!
//! This is the *exception* path: the song is uploaded to a third party, so
//! the UI asks for explicit consent and the user's own license key. It adds
//! stems the offline engine cannot produce (acoustic vs electric guitar).
//!
//! Security and privacy:
//! * HTTPS only; result downloads are restricted to `*.lalal.ai` hosts.
//! * The upload is named `source.<ext>` — the user's file name never leaves
//!   the machine.
//! * Every download is size-capped, then decoded and re-encoded locally, so
//!   only valid audio ends up in the song folder.
//! * The source and results are deleted from the service afterwards.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use joshua_core::decode::decode_file;
use joshua_core::security::Limits;
use joshua_core::separation::{SeparationJob, Separator, StemKind, StemOutput};
use joshua_core::wav::write_stereo_wav;
use serde::Deserialize;
use serde_json::json;

use crate::AiError;

pub const LALAL_BASE: &str = "https://www.lalal.ai";
/// Page where users create an API license key.
pub const LALAL_SIGNUP_URL: &str = "https://www.lalal.ai/api/";
const POLL_INTERVAL: Duration = Duration::from_secs(3);
const TIMEOUT: Duration = Duration::from_secs(45 * 60);
const MAX_RESULT_BYTES: u64 = 600 * 1024 * 1024;

/// Stems requested from the service (its maximum of six per task).
const REQUESTED: [&str; 6] = ["vocals", "drum", "bass", "piano", "electric_guitar", "acoustic_guitar"];

/// Maps a LALAL track label to our stem kind.
#[must_use]
pub fn map_label(label: &str, kind: &str) -> Option<StemKind> {
    if kind == "back" {
        return Some(StemKind::Other);
    }
    Some(match label {
        "vocals" | "voice" => StemKind::Vocals,
        "drum" | "drums" => StemKind::Drums,
        "bass" => StemKind::Bass,
        "piano" => StemKind::Piano,
        "electric_guitar" | "guitar" => StemKind::Guitar,
        "acoustic_guitar" => StemKind::Acoustic,
        _ => return None,
    })
}

/// API keys are opaque tokens; accept a conservative character set only.
#[must_use]
pub fn is_plausible_key(key: &str) -> bool {
    (8..=256).contains(&key.len()) && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Only fetch results from the service's own hosts, always over HTTPS.
#[must_use]
pub fn safe_result_url(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next()?;
    if host.contains('@') || host.contains(':') {
        return None;
    }
    let ok = host == "lalal.ai" || host.ends_with(".lalal.ai");
    ok.then(|| format!("https://{rest}"))
}

#[derive(Debug, Deserialize)]
struct Upload {
    id: String,
}

#[derive(Debug, Deserialize)]
struct TaskStart {
    task_id: String,
}

#[derive(Debug, Deserialize)]
struct Track {
    #[serde(rename = "type")]
    kind: String,
    label: String,
    url: String,
}

#[derive(Debug, Deserialize)]
struct Tracks {
    tracks: Vec<Track>,
}

#[derive(Debug, Deserialize)]
struct TaskStatus {
    status: String,
    #[serde(default)]
    progress: Option<f32>,
    #[serde(default)]
    result: Option<Tracks>,
    #[serde(default)]
    error: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct CheckResponse {
    result: std::collections::HashMap<String, TaskStatus>,
}

#[derive(Debug, Deserialize)]
struct MinutesLeft {
    minutes_left: f64,
}

/// LALAL.AI client.
#[derive(Clone)]
pub struct LalalSeparator {
    key: String,
    base: String,
}

impl std::fmt::Debug for LalalSeparator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the key.
        f.debug_struct("LalalSeparator").field("base", &self.base).finish_non_exhaustive()
    }
}

fn http(e: ureq::Error) -> AiError {
    match e {
        ureq::Error::StatusCode(401 | 403) => AiError::Download("invalid or expired API key".into()),
        ureq::Error::StatusCode(402) => AiError::Download("no processing minutes left".into()),
        ureq::Error::StatusCode(429) => AiError::Download("service busy, try again later".into()),
        other => AiError::Download(format!("cloud service error: {other}")),
    }
}

impl LalalSeparator {
    /// # Errors
    /// If the key is obviously malformed.
    pub fn new(key: &str) -> Result<Self, AiError> {
        let key = key.trim();
        if !is_plausible_key(key) {
            return Err(AiError::Download("invalid API key format".into()));
        }
        Ok(Self { key: key.to_owned(), base: LALAL_BASE.to_owned() })
    }

    fn post(&self, path: &str) -> ureq::RequestBuilder<ureq::typestate::WithBody> {
        crate::http_agent(false).post(format!("{}{path}", self.base)).header("X-License-Key", &self.key)
    }

    /// Remaining processing minutes — also used to validate a key.
    ///
    /// # Errors
    /// Network or authentication errors.
    pub fn minutes_left(&self) -> Result<f64, AiError> {
        let r: MinutesLeft = self
            .post("/api/v1/limits/minutes_left/")
            .send_empty()
            .map_err(http)?
            .body_mut()
            .read_json()
            .map_err(http)?;
        Ok(r.minutes_left)
    }

    fn upload(&self, source: &Path) -> Result<String, AiError> {
        let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("bin");
        let file = std::fs::File::open(source)?;
        let r: Upload = self
            .post("/api/v1/upload/")
            .header("Content-Disposition", format!("attachment; filename=source.{ext}"))
            .header("Content-Type", "application/octet-stream")
            .send(ureq::SendBody::from_owned_reader(file))
            .map_err(http)?
            .body_mut()
            .read_json()
            .map_err(http)?;
        Ok(r.id)
    }

    fn start(&self, source_id: &str) -> Result<String, AiError> {
        let body = json!({
            "source_id": source_id,
            "presets": { "stem_list": REQUESTED, "encoder_format": "wav", "splitter": "auto" },
        });
        let r: TaskStart = self
            .post("/api/v1/split/multistem/")
            .send_json(&body)
            .map_err(http)?
            .body_mut()
            .read_json()
            .map_err(http)?;
        Ok(r.task_id)
    }

    fn check(&self, task_id: &str) -> Result<TaskStatus, AiError> {
        let mut r: CheckResponse = self
            .post("/api/v1/check/")
            .send_json(json!({ "task_ids": [task_id] }))
            .map_err(http)?
            .body_mut()
            .read_json()
            .map_err(http)?;
        r.result.remove(task_id).ok_or_else(|| AiError::Download("task not found".into()))
    }

    fn delete(&self, source_id: &str) {
        let _ = self.post("/api/v1/delete/").send_json(json!({ "source_id": source_id }));
    }

    fn download(&self, url: &str, dest: &Path) -> Result<(), AiError> {
        let url = safe_result_url(url).ok_or_else(|| AiError::Download("unexpected result host".into()))?;
        let mut resp = crate::http_agent(true).get(&url).call().map_err(http)?;
        let mut reader = resp.body_mut().with_config().limit(MAX_RESULT_BYTES).reader();
        let mut file = std::fs::File::create(dest)?;
        std::io::copy(&mut reader, &mut file)?;
        Ok(())
    }

    fn run(&self, job: &SeparationJob<'_>, source_id: &str) -> Result<Vec<StemOutput>, AiError> {
        let task = self.start(source_id)?;
        let started = Instant::now();
        let tracks = loop {
            if job.cancel.load(Ordering::Relaxed) {
                let _ = self.post("/api/v1/cancel/").send_json(json!({ "task_ids": [task] }));
                return Err(AiError::Cancelled);
            }
            if started.elapsed() > TIMEOUT {
                return Err(AiError::Download("cloud separation timed out".into()));
            }
            let status = self.check(&task)?;
            match status.status.as_str() {
                "success" => break status.result.map(|r| r.tracks).unwrap_or_default(),
                "progress" => (job.progress)(0.1 + 0.7 * status.progress.unwrap_or(0.0) / 100.0),
                _ => {
                    let detail = status.error.map(|e| e.to_string()).unwrap_or_default();
                    return Err(AiError::Download(format!("cloud task {}: {detail}", status.status)));
                }
            }
            std::thread::sleep(POLL_INTERVAL);
        };

        std::fs::create_dir_all(job.stems_dir)?;
        let mut outputs: Vec<StemOutput> = Vec::new();
        for (i, track) in tracks.iter().enumerate() {
            let Some(kind) = map_label(&track.label, &track.kind) else { continue };
            if outputs.iter().any(|o| o.kind == kind) {
                continue;
            }
            (job.progress)(0.8 + 0.2 * i as f32 / tracks.len().max(1) as f32);
            let raw = job.scratch_dir.join(format!("cloud-{i}.wav"));
            self.download(&track.url, &raw)?;
            // Decode untrusted bytes and re-encode locally.
            let audio = decode_file(&raw, &Limits::default())?;
            write_stereo_wav(&job.stems_dir.join(format!("{}.wav", kind.as_str())), &audio)?;
            let _ = std::fs::remove_file(&raw);
            outputs.push(StemOutput { kind, relative_path: kind.file_name() });
        }
        if outputs.is_empty() {
            return Err(AiError::Download("the service returned no stems".into()));
        }
        Ok(outputs)
    }
}

impl From<joshua_core::CoreError> for AiError {
    fn from(e: joshua_core::CoreError) -> Self {
        Self::Inference(e.to_string())
    }
}

impl Separator for LalalSeparator {
    fn id(&self) -> String {
        "cloud:lalal".to_owned()
    }

    fn stems(&self) -> Vec<StemKind> {
        vec![
            StemKind::Vocals,
            StemKind::Drums,
            StemKind::Bass,
            StemKind::Guitar,
            StemKind::Acoustic,
            StemKind::Piano,
            StemKind::Other,
        ]
    }

    fn separate(&self, job: &SeparationJob<'_>) -> joshua_core::Result<Vec<StemOutput>> {
        std::fs::create_dir_all(job.scratch_dir)?;
        (job.progress)(0.0);
        let source_id = self.upload(job.source_path)?;
        (job.progress)(0.1);
        let result = self.run(job, &source_id);
        self.delete(&source_id); // privacy: remove our song from their storage
        Ok(result?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_service_labels() {
        assert_eq!(map_label("acoustic_guitar", "stem"), Some(StemKind::Acoustic));
        assert_eq!(map_label("electric_guitar", "stem"), Some(StemKind::Guitar));
        assert_eq!(map_label("drum", "stem"), Some(StemKind::Drums));
        assert_eq!(map_label("no_multistem", "back"), Some(StemKind::Other));
        assert_eq!(map_label("../../etc", "stem"), None);
    }

    #[test]
    fn only_lalal_hosts_over_https() {
        assert_eq!(safe_result_url("http://d.lalal.ai/x/vocals").as_deref(), Some("https://d.lalal.ai/x/vocals"));
        assert!(safe_result_url("https://www.lalal.ai/a").is_some());
        for bad in [
            "https://evil.com/x",
            "https://lalal.ai.evil.com/x",
            "https://evil.com@d.lalal.ai/x",
            "https://d.lalal.ai:8443/x",
            "file:///etc/passwd",
            "ftp://d.lalal.ai/x",
            "https://evillalal.ai/x",
        ] {
            assert!(safe_result_url(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn key_validation_and_redaction() {
        assert!(LalalSeparator::new("short").is_err());
        assert!(LalalSeparator::new("abc def ghi jkl").is_err());
        assert!(LalalSeparator::new("abc\r\nX-Injected: 1").is_err());
        let s = LalalSeparator::new("  a1b2c3d4e5f6g7h8  ").unwrap();
        assert!(!format!("{s:?}").contains("a1b2c3d4"));
    }

    #[test]
    fn parses_check_responses() {
        let ok: CheckResponse = serde_json::from_str(
            r#"{"result":{"t1":{"status":"success","source_id":"s","presets":{},"result":{"duration":16,
            "tracks":[{"label":"vocals","type":"stem","url":"https://d.lalal.ai/1/vocals"},
                      {"label":"no_multistem","type":"back","url":"https://d.lalal.ai/1/back"}]}}}}"#,
        )
        .unwrap();
        let t = &ok.result["t1"];
        assert_eq!(t.status, "success");
        assert_eq!(t.result.as_ref().unwrap().tracks.len(), 2);
        let progress: CheckResponse =
            serde_json::from_str(r#"{"result":{"t1":{"status":"progress","progress":42,"source_id":"s"}}}"#).unwrap();
        assert_eq!(progress.result["t1"].progress, Some(42.0));
    }
}
