//! Downloads the model (once) and separates a file, printing timings.
//! `cargo run --release -p joshua-ai --example bench -- <model-dir> <audio> <out-dir> [cpu]`
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use joshua_ai::separator::Accelerator;
use joshua_ai::{HTDEMUCS_6S, ModelStore, NeuralSeparator};
use joshua_core::decode::decode_file;
use joshua_core::security::Limits;
use joshua_core::separation::{SeparationJob, Separator};

fn main() {
    let args: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let (models, audio, out) = (&args[0], &args[1], &args[2]);
    let accel = if args.get(3).is_some_and(|a| a.as_os_str() == "cpu") { Accelerator::Cpu } else { Accelerator::Auto };
    let no = AtomicBool::new(false);
    let store = ModelStore::new(models.clone());
    let t = Instant::now();
    let model = store.install(&HTDEMUCS_6S, &|p| eprint!("\rdownload {:>3.0}%", p * 100.0), &no).unwrap();
    eprintln!("\nmodel ready in {:.1}s", t.elapsed().as_secs_f64());
    let buf = decode_file(audio, &Limits::default()).unwrap();
    let sep = NeuralSeparator::new(model, accel);
    let t = Instant::now();
    let job = SeparationJob {
        source_path: audio,
        audio: &buf,
        stems_dir: out,
        scratch_dir: out,
        progress: &|p| eprint!("\rseparating {:>3.0}%", p * 100.0),
        cancel: &no,
    };
    sep.separate(&job).unwrap();
    let secs = t.elapsed().as_secs_f64();
    eprintln!(
        "\n{:.1}s audio separated in {secs:.1}s ({:.1}x realtime, {accel:?})",
        buf.duration_secs(),
        buf.duration_secs() / secs
    );
}
