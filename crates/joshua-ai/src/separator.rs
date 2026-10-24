//! HT-Demucs inference with streaming overlap-add.
//!
//! The model takes fixed 7.8 s stereo windows at 44.1 kHz. Windows overlap
//! by 25 % and are cross-faded linearly, then finished samples are streamed
//! straight into the stem WAV writers, so memory stays constant no matter
//! how long the song is.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use joshua_core::StereoBuffer;
use joshua_core::resample::resample;
use joshua_core::separation::{SeparationJob, Separator, StemKind, StemOutput};
use joshua_core::wav::StereoWavWriter;
use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use ort::value::Tensor;

use crate::AiError;

pub const MODEL_RATE: u32 = 44_100;
/// Samples per model window (7.8 s).
pub const SEGMENT: usize = 343_980;
const OVERLAP: usize = SEGMENT / 4;
const STRIDE: usize = SEGMENT - OVERLAP;
/// Output order of the network.
pub const MODEL_STEMS: [StemKind; 6] =
    [StemKind::Drums, StemKind::Bass, StemKind::Other, StemKind::Vocals, StemKind::Guitar, StemKind::Piano];

/// Hardware used for inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accelerator {
    /// DirectML (Windows) / CUDA (feature `cuda`) when available, CPU otherwise.
    Auto,
    Cpu,
}

fn environment() -> Result<&'static ort::environment::Environment, AiError> {
    static ENV: std::sync::OnceLock<ort::environment::Environment> = std::sync::OnceLock::new();
    if let Some(env) = ENV.get() {
        return Ok(env);
    }
    let env = ort::init().with_name("joshua-plus").build()?;
    Ok(ENV.get_or_init(|| env))
}

fn build_session(model: &Path, accel: Accelerator) -> Result<Session, AiError> {
    let threads = std::thread::available_parallelism().map_or(4, std::num::NonZero::get);
    let mut builder = Session::builder(environment()?)?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| AiError::Inference(e.to_string()))?
        .with_intra_threads(threads)
        .map_err(|e| AiError::Inference(e.to_string()))?;
    // macOS: Core ML needs >10 minutes to compile this transformer graph,
    // while the CPU kernels run ~5x real time on Apple Silicon.
    if accel == Accelerator::Cpu || cfg!(target_os = "macos") {
        builder = builder
            .with_auto_device(ort::session::builder::AutoDevicePolicy::PreferCPU)
            .map_err(|e| AiError::Inference(e.to_string()))?;
    } else {
        #[allow(unused_mut)]
        let mut eps: Vec<ort::ep::ExecutionProviderDispatch> = Vec::new();
        #[cfg(target_os = "windows")]
        eps.push(ort::ep::DirectML::default().build());
        #[cfg(feature = "cuda")]
        eps.push(ort::ep::CUDA::default().build());
        if !eps.is_empty() {
            builder = builder.with_execution_providers(eps).map_err(|e| AiError::Inference(e.to_string()))?;
        }
    }
    Ok(builder.commit_from_file(model)?)
}

/// Receives finished samples: `(stem_index, left, right)`.
pub type WindowSink<'a> = dyn FnMut(usize, &[f32], &[f32]) -> Result<(), AiError> + 'a;
/// Maps one `(1, 2, SEGMENT)` model input to its `(1, stems, 2, SEGMENT)` output.
pub type Infer<'a> = dyn FnMut(Vec<f32>) -> Result<Vec<f32>, AiError> + 'a;

/// Built-in neural separator (6 stems).
pub struct NeuralSeparator {
    model: PathBuf,
    accel: Accelerator,
    /// Loading the 136 MB graph costs a few seconds; keep it between songs.
    session: Mutex<Option<Session>>,
}

impl std::fmt::Debug for NeuralSeparator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NeuralSeparator").field("model", &self.model).field("accel", &self.accel).finish()
    }
}

impl NeuralSeparator {
    #[must_use]
    pub fn new(model: PathBuf, accel: Accelerator) -> Self {
        Self { model, accel, session: Mutex::new(None) }
    }

    fn with_session<T>(&self, mut f: impl FnMut(&mut Session) -> Result<T, AiError>) -> Result<T, AiError> {
        let mut guard = self.session.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut accel = self.accel;
        loop {
            if guard.is_none() {
                *guard = Some(match build_session(&self.model, accel) {
                    Ok(s) => s,
                    Err(_) if accel == Accelerator::Auto => {
                        accel = Accelerator::Cpu;
                        continue;
                    }
                    Err(e) => return Err(e),
                });
            }
            let session = guard.as_mut().ok_or_else(|| AiError::Inference("no session".into()))?;
            match f(session) {
                // Accelerators compile lazily on the first run; if one rejects
                // the graph, retry the whole job on the CPU.
                Err(AiError::Inference(_)) if accel == Accelerator::Auto => {
                    accel = Accelerator::Cpu;
                    *guard = None;
                }
                other => return other,
            }
        }
    }
}

/// Weighted overlap-add over model windows for every stem and channel.
struct WindowAccumulator {
    acc: Vec<[Vec<f32>; 2]>,
    weight: Vec<f32>,
}

impl WindowAccumulator {
    fn new(stems: usize) -> Self {
        Self { acc: (0..stems).map(|_| [vec![0.0; SEGMENT], vec![0.0; SEGMENT]]).collect(), weight: vec![0.0; SEGMENT] }
    }

    /// Adds a `(stems, 2, SEGMENT)` model output for the first `len` samples.
    fn add(&mut self, output: &[f32], len: usize, first: bool, last: bool) {
        for i in 0..len {
            let fade_in = if first || i >= OVERLAP { 1.0 } else { (i as f32 + 0.5) / OVERLAP as f32 };
            let from_end = SEGMENT - i;
            let fade_out = if last || from_end > OVERLAP { 1.0 } else { (from_end as f32 - 0.5) / OVERLAP as f32 };
            let w = fade_in * fade_out;
            self.weight[i] += w;
            for (s, stem) in self.acc.iter_mut().enumerate() {
                for (c, ch) in stem.iter_mut().enumerate() {
                    ch[i] += output[(s * 2 + c) * SEGMENT + i] * w;
                }
            }
        }
    }

    /// Hands the first `len` finished samples to `sink` and shifts by `len`.
    fn emit(&mut self, len: usize, scale: f32, sink: &mut WindowSink<'_>) -> Result<(), AiError> {
        let mut l = vec![0.0f32; len];
        let mut r = vec![0.0f32; len];
        for s in 0..self.acc.len() {
            for i in 0..len {
                let w = self.weight[i].max(1e-8);
                l[i] = self.acc[s][0][i] / w * scale;
                r[i] = self.acc[s][1][i] / w * scale;
            }
            sink(s, &l, &r)?;
            for ch in &mut self.acc[s] {
                ch.copy_within(len.., 0);
                let n = ch.len();
                ch[n - len..].fill(0.0);
            }
        }
        self.weight.copy_within(len.., 0);
        let n = self.weight.len();
        self.weight[n - len..].fill(0.0);
        Ok(())
    }
}

/// Runs the network over `audio` (44.1 kHz) and streams stems to `sink`.
/// `infer` maps one `(1, 2, SEGMENT)` window to `(1, 6, 2, SEGMENT)`.
///
/// # Errors
/// Inference, sink errors or cancellation.
pub fn separate_windows(
    audio: &StereoBuffer,
    stems: usize,
    infer: &mut Infer<'_>,
    sink: &mut WindowSink<'_>,
    progress: &dyn Fn(f32),
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<(), AiError> {
    let total = audio.frames();
    if total == 0 {
        return Ok(());
    }
    // Demucs-style level normalisation (undone on output). Audio is
    // zero-mean in practice, so only the scale is normalised; that keeps the
    // stems summing exactly to the mix.
    let var = audio.to_mono().iter().map(|x| f64::from(*x).powi(2)).sum::<f64>() / total as f64;
    #[allow(clippy::cast_possible_truncation)]
    let std = (var.sqrt() as f32).max(1e-5);

    let chunks = total.saturating_sub(OVERLAP).div_ceil(STRIDE).max(1);
    let mut acc = WindowAccumulator::new(stems);
    for i in 0..chunks {
        if cancel.load(Ordering::Relaxed) {
            return Err(AiError::Cancelled);
        }
        progress(i as f32 / chunks as f32);
        let start = i * STRIDE;
        let len = SEGMENT.min(total - start);
        let mut input = vec![0.0f32; 2 * SEGMENT];
        for (c, src) in [audio.left(), audio.right()].into_iter().enumerate() {
            for (dst, s) in input[c * SEGMENT..c * SEGMENT + len].iter_mut().zip(&src[start..start + len]) {
                *dst = s / std;
            }
        }
        let output = infer(input)?;
        if output.len() != stems * 2 * SEGMENT {
            return Err(AiError::Inference(format!("unexpected output size {}", output.len())));
        }
        let last = i + 1 == chunks;
        acc.add(&output, len, i == 0, last);
        acc.emit(if last { len } else { STRIDE }, std, sink)?;
    }
    progress(1.0);
    Ok(())
}

impl Separator for NeuralSeparator {
    fn id(&self) -> String {
        "neural:htdemucs_6s".to_owned()
    }

    fn stems(&self) -> Vec<StemKind> {
        vec![StemKind::Vocals, StemKind::Drums, StemKind::Bass, StemKind::Guitar, StemKind::Piano, StemKind::Other]
    }

    fn separate(&self, job: &SeparationJob<'_>) -> joshua_core::Result<Vec<StemOutput>> {
        std::fs::create_dir_all(job.stems_dir)?;
        let audio = resample(job.audio.clone(), MODEL_RATE)?;
        let core = |e: joshua_core::CoreError| AiError::Inference(e.to_string());
        self.with_session(|session| {
            // Fresh writers per attempt, so a failed accelerator run leaves no partial stems.
            let mut writers = MODEL_STEMS
                .iter()
                .map(|k| StereoWavWriter::create(&job.stems_dir.join(format!("{}.wav", k.as_str())), MODEL_RATE))
                .collect::<joshua_core::Result<Vec<_>>>()
                .map_err(core)?;
            let mut infer = |input: Vec<f32>| -> Result<Vec<f32>, AiError> {
                let tensor = Tensor::from_array(([1usize, 2, SEGMENT], input))?;
                let outputs = session.run(ort::inputs![tensor])?;
                let (_, data) = outputs[0].try_extract_tensor::<f32>()?;
                Ok(data.to_vec())
            };
            let mut sink =
                |s: usize, l: &[f32], r: &[f32]| -> Result<(), AiError> { writers[s].write(l, r).map_err(core) };
            separate_windows(&audio, MODEL_STEMS.len(), &mut infer, &mut sink, job.progress, job.cancel)?;
            for w in writers {
                w.finish().map_err(core)?;
            }
            Ok(())
        })?;
        Ok(self.stems().into_iter().map(|k| StemOutput { kind: k, relative_path: k.file_name() }).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    /// Fake network: stem `s` = input × (s + 1) / 21, so the stems sum to the input.
    fn fake(input: Vec<f32>) -> Result<Vec<f32>, AiError> {
        let mut out = Vec::with_capacity(6 * input.len());
        for s in 0..6 {
            out.extend(input.iter().map(|x| x * (s as f32 + 1.0) / 21.0));
        }
        Ok(out)
    }

    type Stems = Vec<(Vec<f32>, Vec<f32>)>;

    fn run(frames: usize) -> (StereoBuffer, Stems) {
        let left: Vec<f32> = (0..frames).map(|i| ((i * 7919) % 1000) as f32 / 2000.0 - 0.2).collect();
        let right: Vec<f32> = left.iter().map(|x| -0.5 * x).collect();
        let audio = StereoBuffer::new(MODEL_RATE, left, right).unwrap();
        let mut stems = vec![(Vec::new(), Vec::new()); 6];
        separate_windows(
            &audio,
            6,
            &mut fake,
            &mut |s, l, r| {
                stems[s].0.extend_from_slice(l);
                stems[s].1.extend_from_slice(r);
                Ok(())
            },
            &|_| {},
            &AtomicBool::new(false),
        )
        .unwrap();
        (audio, stems)
    }

    #[test]
    fn overlap_add_reconstructs_exactly_across_window_boundaries() {
        for frames in [1, 1000, SEGMENT - 1, SEGMENT, SEGMENT + 1, STRIDE * 3 + 12_345] {
            let (audio, stems) = run(frames);
            for (s, (l, r)) in stems.iter().enumerate() {
                assert_eq!(l.len(), frames, "stem {s} length for {frames}");
                let k = (s as f32 + 1.0) / 21.0;
                let err = l
                    .iter()
                    .zip(audio.left())
                    .chain(r.iter().zip(audio.right()))
                    .map(|(y, x)| (y - x * k).abs())
                    .fold(0.0, f32::max);
                assert!(err < 1e-4, "frames {frames} stem {s} error {err}");
            }
        }
    }

    #[test]
    fn rejects_malformed_model_output_and_honours_cancel() {
        let audio = StereoBuffer::silent(MODEL_RATE, 10).unwrap();
        let no = AtomicBool::new(false);
        let r = separate_windows(&audio, 6, &mut |_| Ok(vec![0.0; 3]), &mut |_, _, _| Ok(()), &|_| {}, &no);
        assert!(matches!(r, Err(AiError::Inference(_))));
        let r = separate_windows(&audio, 6, &mut fake, &mut |_, _, _| Ok(()), &|_| {}, &AtomicBool::new(true));
        assert!(matches!(r, Err(AiError::Cancelled)));
    }
}
