//! Model files: pinned download location, size and SHA-256.
//!
//! Models are fetched once over HTTPS from a pinned repository revision and
//! verified against a hard-coded SHA-256 before they are ever loaded, so a
//! compromised mirror or a truncated download can never reach ONNX Runtime.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use sha2::{Digest, Sha256};

use crate::AiError;

/// A downloadable model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelSpec {
    pub id: &'static str,
    pub file_name: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size: u64,
    pub license: &'static str,
}

/// HT-Demucs v4, 6 sources (drums, bass, other, vocals, guitar, piano),
/// MIT-licensed weights by Meta AI Research, ONNX export with fp16 weights.
pub const HTDEMUCS_6S: ModelSpec = ModelSpec {
    id: "htdemucs_6s",
    file_name: "htdemucs_6s_fp16weights.onnx",
    url: "https://huggingface.co/adowu/htdemucs-6s-onnx/resolve/d83893853e32af79f4ddc8d97d7e513eb5058d14/htdemucs_6s_fp16weights.onnx",
    sha256: "7ce55792e2231c93fbf92de95f5fd5b3a5e6c89f7db690dfd693e8f1dce56869",
    size: 136_428_532,
    license: "MIT",
};

/// Directory holding verified models.
#[derive(Debug, Clone)]
pub struct ModelStore {
    dir: PathBuf,
}

impl ModelStore {
    #[must_use]
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    #[must_use]
    pub fn path(&self, spec: &ModelSpec) -> PathBuf {
        self.dir.join(spec.file_name)
    }

    /// Present with the expected size (the hash was checked at install).
    #[must_use]
    pub fn is_installed(&self, spec: &ModelSpec) -> bool {
        std::fs::symlink_metadata(self.path(spec)).is_ok_and(|m| m.is_file() && m.len() == spec.size)
    }

    /// Downloads and verifies `spec`. `progress` receives `0.0..=1.0`.
    ///
    /// # Errors
    /// Network, I/O, size or checksum failures, or cancellation.
    pub fn install(&self, spec: &ModelSpec, progress: &dyn Fn(f32), cancel: &AtomicBool) -> Result<PathBuf, AiError> {
        if self.is_installed(spec) {
            return Ok(self.path(spec));
        }
        if !spec.url.starts_with("https://huggingface.co/") {
            return Err(AiError::Download("untrusted model URL".into()));
        }
        std::fs::create_dir_all(&self.dir)?;
        let response = crate::http_agent(true).get(spec.url).call().map_err(|e| AiError::Download(e.to_string()))?;
        let reader = response.into_body().into_reader();
        let part = self.dir.join(format!("{}.part", spec.file_name));
        let result = save_verified(reader, &part, spec, progress, cancel);
        if result.is_err() {
            let _ = std::fs::remove_file(&part);
        }
        result?;
        std::fs::rename(&part, self.path(spec))?;
        Ok(self.path(spec))
    }
}

/// Streams `reader` into `dest`, enforcing the size cap and verifying SHA-256.
fn save_verified(
    reader: impl Read,
    dest: &Path,
    spec: &ModelSpec,
    progress: &dyn Fn(f32),
    cancel: &AtomicBool,
) -> Result<(), AiError> {
    let mut reader = reader.take(spec.size + 1);
    let mut file = File::create(dest)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut total = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(AiError::Cancelled);
        }
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > spec.size {
            return Err(AiError::Download("model larger than expected".into()));
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n])?;
        progress(total as f32 / spec.size as f32);
    }
    file.sync_all()?;
    if total != spec.size {
        return Err(AiError::Download("incomplete download".into()));
    }
    let digest: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if digest != spec.sha256 {
        return Err(AiError::Download("checksum mismatch".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec_for(data: &[u8]) -> ModelSpec {
        let digest: String = Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect();
        ModelSpec {
            id: "t",
            file_name: "t.onnx",
            url: "https://huggingface.co/x",
            sha256: Box::leak(digest.into_boxed_str()),
            size: data.len() as u64,
            license: "MIT",
        }
    }

    #[test]
    fn accepts_matching_payload_and_rejects_tampering() {
        let dir = tempfile::tempdir().unwrap();
        let good = b"model bytes".to_vec();
        let spec = spec_for(&good);
        let dest = dir.path().join("m");
        let no = AtomicBool::new(false);
        save_verified(good.as_slice(), &dest, &spec, &|_| {}, &no).unwrap();

        let mut evil = good.clone();
        evil[0] ^= 1;
        assert!(save_verified(evil.as_slice(), &dest, &spec, &|_| {}, &no).is_err());
        assert!(save_verified(&good[..5], &dest, &spec, &|_| {}, &no).is_err());
        let mut long = good.clone();
        long.extend_from_slice(b"extra");
        assert!(save_verified(long.as_slice(), &dest, &spec, &|_| {}, &no).is_err());
        assert!(matches!(
            save_verified(good.as_slice(), &dest, &spec, &|_| {}, &AtomicBool::new(true)),
            Err(AiError::Cancelled)
        ));
    }

    #[test]
    fn pinned_spec_is_https_huggingface_with_full_hash() {
        assert!(HTDEMUCS_6S.url.starts_with("https://huggingface.co/"));
        assert!(HTDEMUCS_6S.url.contains("/resolve/d83893853e32af79f4ddc8d97d7e513eb5058d14/"));
        assert_eq!(HTDEMUCS_6S.sha256.len(), 64);
    }
}
