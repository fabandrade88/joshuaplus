//! Song metadata (artist, album, year…) and cover art read from the file's
//! own tags (ID3, Vorbis comments, MP4 atoms). Nothing is fetched online.
//!
//! Embedded pictures are untrusted: they are decoded with strict size
//! limits and re-encoded as plain JPEG, which strips any trailing payload,
//! EXIF data or polyglot tricks before the image ever reaches the UI.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use image::imageops::FilterType;
use serde::{Deserialize, Serialize};
use symphonia::core::formats::FormatOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, MetadataRevision, StandardTag, StandardVisualKey};

use crate::error::{CoreError, Result};
use crate::security::sanitize_title;

/// Cover image (shown in the song view) and its list thumbnail.
pub const COVER_FILE: &str = "cover.jpg";
pub const THUMB_FILE: &str = "cover-thumb.jpg";
const COVER_SIZE: u32 = 512;
const THUMB_SIZE: u32 = 96;
const MAX_PICTURE_BYTES: usize = 20 * 1024 * 1024;
const MAX_PICTURE_DIM: u32 = 8_000;
const MAX_FIELD_CHARS: usize = 200;

/// Descriptive tags of a song. All optional; never used to build paths.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct SongMetadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub year: Option<String>,
    pub genre: Option<String>,
    pub track: Option<String>,
    pub composer: Option<String>,
    pub label: Option<String>,
}

impl SongMetadata {
    /// Every field is short, printable text.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        [
            &self.title,
            &self.artist,
            &self.album,
            &self.album_artist,
            &self.year,
            &self.genre,
            &self.track,
            &self.composer,
            &self.label,
        ]
        .into_iter()
        .flatten()
        .all(|v| v.chars().count() <= MAX_FIELD_CHARS && !v.chars().any(char::is_control))
    }
}

/// Tags plus the raw (still untrusted) front cover, if any.
#[derive(Debug, Default)]
pub struct ExtractedTags {
    pub metadata: SongMetadata,
    pub picture: Option<Vec<u8>>,
}

fn clean(value: &str) -> Option<String> {
    let v = sanitize_title(value);
    (v != "Untitled" || value.trim() == "Untitled").then(|| v.chars().take(MAX_FIELD_CHARS).collect())
}

fn absorb(revision: &MetadataRevision, out: &mut ExtractedTags, track: &mut (Option<u64>, Option<u64>)) {
    let m = &mut out.metadata;
    for tag in &revision.media.tags {
        let Some(std) = &tag.std else { continue };
        let set = |slot: &mut Option<String>, v: &str| {
            if slot.is_none() {
                *slot = clean(v);
            }
        };
        match std {
            StandardTag::TrackTitle(v) => set(&mut m.title, v),
            StandardTag::Artist(v) => set(&mut m.artist, v),
            StandardTag::Album(v) => set(&mut m.album, v),
            StandardTag::AlbumArtist(v) => set(&mut m.album_artist, v),
            StandardTag::Genre(v) => set(&mut m.genre, v),
            StandardTag::Composer(v) => set(&mut m.composer, v),
            StandardTag::Label(v) => set(&mut m.label, v),
            StandardTag::ReleaseDate(v) | StandardTag::RecordingDate(v) | StandardTag::OriginalReleaseDate(v) => {
                set(&mut m.year, v.get(..4).unwrap_or(v));
            }
            StandardTag::ReleaseYear(y) => set(&mut m.year, &y.to_string()),
            StandardTag::RecordingYear(y) => set(&mut m.year, &y.to_string()),
            StandardTag::TrackNumber(n) => track.0 = track.0.or(Some(*n)),
            StandardTag::TrackTotal(n) => track.1 = track.1.or(Some(*n)),
            _ => {}
        }
    }
    // Prefer the front cover; otherwise take the first picture.
    let pick = revision
        .media
        .visuals
        .iter()
        .find(|v| v.usage == Some(StandardVisualKey::FrontCover))
        .or_else(|| revision.media.visuals.first());
    if out.picture.is_none() {
        if let Some(v) = pick.filter(|v| v.data.len() <= MAX_PICTURE_BYTES) {
            out.picture = Some(v.data.to_vec());
        }
    }
}

/// Reads tags and the cover picture without decoding any audio.
///
/// # Errors
/// I/O or unrecognised container.
pub fn read_tags(path: &Path) -> Result<ExtractedTags> {
    let file = crate::security::open_regular(path)?;
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    catch_unwind(AssertUnwindSafe(|| -> Result<ExtractedTags> {
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(e) = ext.as_deref() {
            hint.with_extension(e);
        }
        let mut format = symphonia::default::get_probe()
            .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
            .map_err(|e| CoreError::Decode(e.to_string()))?;
        let mut out = ExtractedTags::default();
        let mut track = (None, None);
        let mut log = format.metadata();
        for _ in 0..16 {
            if let Some(rev) = log.current() {
                absorb(rev, &mut out, &mut track);
            }
            if log.is_latest() {
                break;
            }
            log.pop();
        }
        out.metadata.track = match track {
            (Some(n), Some(t)) if t > 1 => Some(format!("{n}/{t}")),
            (Some(n), _) => Some(n.to_string()),
            _ => None,
        };
        Ok(out)
    }))
    .unwrap_or_else(|_| Err(CoreError::Decode("metadata reader panicked".into())))
}

/// Decodes an untrusted picture and writes a square-fitted cover and thumbnail
/// as fresh JPEGs into `dir`. Returns whether a cover was written.
///
/// # Errors
/// I/O errors writing the files (undecodable pictures just return `false`).
/// Decodes an untrusted image with dimension and allocation limits.
fn decode_limited(bytes: &[u8]) -> Option<image::DynamicImage> {
    catch_unwind(AssertUnwindSafe(|| {
        let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(MAX_PICTURE_DIM);
        limits.max_image_height = Some(MAX_PICTURE_DIM);
        limits.max_alloc = Some(256 * 1024 * 1024);
        reader.limits(limits);
        reader.decode().ok()
    }))
    .ok()
    .flatten()
}

pub fn write_cover(picture: &[u8], dir: &Path) -> Result<bool> {
    let Some(img) = decode_limited(picture) else { return Ok(false) };
    for (name, size) in [(COVER_FILE, COVER_SIZE), (THUMB_FILE, THUMB_SIZE)] {
        let rgb = img.resize(size, size, FilterType::Triangle).to_rgb8();
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 85)
            .encode_image(&rgb)
            .map_err(|e| CoreError::InvalidAudio(e.to_string()))?;
        crate::security::atomic_write(dir, name, &bytes, false)?;
    }
    Ok(true)
}

/// Reads one of our own re-encoded JPEGs as a `data:` URL for the UI
/// (allowed by the app's CSP; no file access from the webview needed).
///
/// # Errors
/// I/O errors, or files larger than 2 MiB.
pub fn jpeg_data_url(path: &Path) -> Result<String> {
    use base64::Engine as _;
    use std::io::Read as _;
    let mut bytes = Vec::new();
    crate::security::open_regular(path)?.take(2 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 2 * 1024 * 1024 || bytes.get(..3) != Some(&[0xFF, 0xD8, 0xFF]) {
        return Err(crate::error::RejectReason::UnrecognizedSignature.into());
    }
    // Re-encode on every read: a cover swapped in a shared folder is never
    // handed raw to the webview's image decoder.
    let img = decode_limited(&bytes).ok_or(crate::error::RejectReason::UnrecognizedSignature)?;
    let mut clean = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut clean, 85)
        .encode_image(&img.to_rgb8())
        .map_err(|e| CoreError::InvalidAudio(e.to_string()))?;
    Ok(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(clean)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 128]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn covers_are_reencoded_and_resized() {
        let dir = tempfile::tempdir().unwrap();
        let mut payload = png(1200, 800);
        payload.extend_from_slice(b"MZ\x90\x00 trailing executable payload");
        assert!(write_cover(&payload, dir.path()).unwrap());
        let cover = std::fs::read(dir.path().join(COVER_FILE)).unwrap();
        assert_eq!(&cover[..3], &[0xFF, 0xD8, 0xFF], "fresh JPEG");
        assert!(!cover.windows(2).any(|w| w == b"MZ"), "payload stripped");
        let thumb = image::load_from_memory(&std::fs::read(dir.path().join(THUMB_FILE)).unwrap()).unwrap();
        assert!(thumb.width() <= THUMB_SIZE && thumb.height() <= THUMB_SIZE);
    }

    #[test]
    fn hostile_pictures_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!write_cover(b"not an image", dir.path()).unwrap());
        assert!(!write_cover(&[], dir.path()).unwrap());
        // PNG header declaring a gigantic canvas (decompression bomb).
        let mut bomb = png(1, 1);
        bomb[16..20].copy_from_slice(&100_000u32.to_be_bytes());
        bomb[20..24].copy_from_slice(&100_000u32.to_be_bytes());
        assert!(!write_cover(&bomb, dir.path()).unwrap());
        assert!(!dir.path().join(COVER_FILE).exists());
    }

    #[test]
    fn metadata_validation_and_cleaning() {
        assert_eq!(clean("  Bad\u{202E} Omens "), Some("Bad Omens".into()));
        assert_eq!(clean("\u{200B}"), None);
        let bad = SongMetadata { artist: Some("a\u{7}b".into()), ..Default::default() };
        assert!(!bad.is_valid());
        assert!(SongMetadata::default().is_valid());
    }

    #[test]
    fn files_without_tags_are_fine() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.wav");
        crate::wav::write_stereo_wav(&path, &crate::StereoBuffer::silent(8_000, 100).unwrap()).unwrap();
        let tags = read_tags(&path).unwrap();
        assert_eq!(tags.metadata, SongMetadata::default());
        assert!(tags.picture.is_none());
    }
}
