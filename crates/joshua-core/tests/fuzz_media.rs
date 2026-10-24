//! Mutation fuzzing of every untrusted parser: audio decoders (MP3, FLAC,
//! OGG/Vorbis, M4A/AAC, AIFF, WAV), tag readers and cover-art decoding.
//!
//! Each valid fixture is mutated thousands of ways (bit flips, truncation,
//! absurd size fields, duplicated/zeroed/garbage blocks). Every mutation must
//! be handled without a panic, a crash or a hang (time budget per file).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::Cursor;
use std::path::Path;
use std::time::{Duration, Instant};

use joshua_core::decode::decode_source;
use joshua_core::metadata::{read_tags, write_cover};
use joshua_core::security::{Limits, open_audio_file};

const MUTATIONS_PER_FORMAT: usize = 400;
const BUDGET_PER_INPUT: Duration = Duration::from_secs(3);

/// Small deterministic PRNG (xorshift64*), so failures are reproducible.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn mutate(rng: &mut Rng, input: &[u8]) -> Vec<u8> {
    let mut v = input.to_vec();
    let len = v.len().max(1);
    match rng.below(7) {
        0 => {
            for _ in 0..1 + rng.below(16) {
                let i = rng.below(len);
                if let Some(b) = v.get_mut(i) {
                    *b ^= 1 << rng.below(8);
                }
            }
        }
        1 => v.truncate(rng.below(len)),
        2 => {
            // Classic attack: oversize length/count fields.
            let i = rng.below(len.saturating_sub(4));
            let val: [u8; 4] = match rng.below(3) {
                0 => [0xFF; 4],
                1 => [0x7F, 0xFF, 0xFF, 0xFF],
                _ => [0x00, 0x00, 0x00, 0x00],
            };
            for (k, b) in val.iter().enumerate() {
                if let Some(x) = v.get_mut(i + k) {
                    *x = *b;
                }
            }
        }
        3 => {
            let a = rng.below(len);
            let b = (a + rng.below(512)).min(v.len());
            let chunk = v[a..b].to_vec();
            let at = rng.below(len);
            for _ in 0..1 + rng.below(4) {
                v.splice(at..at, chunk.iter().copied());
            }
        }
        4 => {
            let at = rng.below(len);
            let garbage: Vec<u8> = (0..1 + rng.below(256)).map(|_| rng.next() as u8).collect();
            v.splice(at..at, garbage);
        }
        5 => {
            let a = rng.below(len);
            let b = (a + rng.below(1024)).min(v.len());
            v[a..b].fill(0);
        }
        _ => v = (0..rng.below(4096)).map(|_| rng.next() as u8).collect(),
    }
    v
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).unwrap()
}

fn timed<T>(what: &str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = f();
    assert!(start.elapsed() < BUDGET_PER_INPUT, "{what} took {:?} (possible hang)", start.elapsed());
    out
}

#[test]
fn every_fixture_decodes_cleanly() {
    let limits = Limits::default();
    for (name, ext) in [
        ("tiny.mp3", "mp3"),
        ("tiny.flac", "flac"),
        ("tiny.ogg", "ogg"),
        ("tiny.m4a", "m4a"),
        ("tiny.aiff", "aiff"),
        ("tiny.wav", "wav"),
    ] {
        let audio = decode_source(Box::new(Cursor::new(fixture(name))), Some(ext), &limits)
            .unwrap_or_else(|e| panic!("{name} should decode: {e}"));
        assert!(audio.duration_secs() > 0.3, "{name} too short");
    }
}

#[test]
fn mutated_media_never_panics_or_hangs() {
    let limits = Limits::default();
    let dir = tempfile::tempdir().unwrap();
    let mut rng = Rng(0x00C0_FFEE_D00D_F00D);
    let mut decoded_ok = 0;
    for (name, ext) in [
        ("tiny.mp3", "mp3"),
        ("tiny.flac", "flac"),
        ("tiny.ogg", "ogg"),
        ("tiny.m4a", "m4a"),
        ("tiny.aiff", "aiff"),
        ("tiny.wav", "wav"),
    ] {
        let original = fixture(name);
        for i in 0..MUTATIONS_PER_FORMAT {
            let bytes = mutate(&mut rng, &original);
            let label = format!("{name} mutation #{i}");
            if timed(&label, || decode_source(Box::new(Cursor::new(bytes.clone())), Some(ext), &limits)).is_ok() {
                decoded_ok += 1;
            }
            // Same bytes through the file-based paths (signature check, tags).
            let path = dir.path().join(format!("m.{ext}"));
            std::fs::write(&path, &bytes).unwrap();
            let _ = timed(&label, || open_audio_file(&path, &limits));
            if let Ok(tags) = timed(&label, || read_tags(&path)) {
                if let Some(pic) = tags.picture {
                    let _ = timed(&label, || write_cover(&pic, dir.path()));
                }
            }
        }
    }
    // Sanity: the fuzzer must not only produce garbage — some mutations still decode.
    assert!(decoded_ok > 50, "only {decoded_ok} mutations decoded; fuzzer too destructive");
}

#[test]
fn mutated_cover_images_never_panic() {
    let dir = tempfile::tempdir().unwrap();
    let mut png = Cursor::new(Vec::new());
    image::RgbImage::from_fn(64, 64, |x, y| image::Rgb([x as u8 * 4, y as u8 * 4, 90]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let mut jpg = Cursor::new(Vec::new());
    image::RgbImage::from_fn(64, 64, |x, y| image::Rgb([x as u8 * 4, 30, y as u8 * 4]))
        .write_to(&mut jpg, image::ImageFormat::Jpeg)
        .unwrap();
    let mut rng = Rng(0xBADC_0FFE_E0DD_F00D);
    for original in [png.into_inner(), jpg.into_inner()] {
        for i in 0..600 {
            let bytes = mutate(&mut rng, &original);
            let _ = timed(&format!("cover mutation #{i}"), || write_cover(&bytes, dir.path()));
        }
    }
}
