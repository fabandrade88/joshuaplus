// Frame-by-frame composition of the exported clips with WebCodecs: the
// camera clip is decoded, cropped and titled once per frame, then encoded
// for every format, never in real time. Nothing can stall or drop frames
// (a slow machine just takes longer), and the encoder gets the time it
// needs for full quality. Engines without WebCodecs H.264/AAC use the
// real-time path in the studio instead.
import {
  ALL_FORMATS,
  AudioBufferSource,
  BlobSource,
  CanvasSource,
  Input,
  Mp4OutputFormat,
  Output,
  Quality,
  StreamTarget,
  VideoSampleSink,
  canEncodeAudio,
  canEncodeVideo,
  type StreamTargetChunk,
  type VideoSample,
} from "mediabunny";
import { FORMATS, coverCrop, videoBitrate, type VideoFormat } from "./formats";

/** Output frame rate (constant; the camera's own rate varies). */
export const COMPOSE_FPS = 30;
const AUDIO_BITRATE = 256_000;
/** H.264 High profile, level 4.0: what YouTube and Instagram recommend,
 *  and enough for 1080p30 in every format. */
const AVC_CODEC = "avc1.640028";
/** Audio is handed to the muxer in slices this long, alongside the frames. */
const AUDIO_SLICE_SECS = 1;
/** Bytes per write to the backend. */
const CHUNK_BYTES = 8 * 1024 * 1024;

/** Whether this engine can compose frame by frame (decided once). */
let support: Promise<boolean> | null = null;
export function canComposeOffline(): Promise<boolean> {
  support ??= (async () => {
    try {
      if (typeof VideoEncoder === "undefined" || typeof VideoDecoder === "undefined" || typeof AudioEncoder === "undefined") {
        return false;
      }
      const video = await Promise.all(
        FORMATS.map((f) =>
          canEncodeVideo("avc", {
            width: f.width,
            height: f.height,
            frameRate: COMPOSE_FPS,
            quality: new Quality({ bitrate: videoBitrate(f) }),
            fullCodecString: AVC_CODEC,
          }),
        ),
      );
      const audio = await canEncodeAudio("aac", {
        numberOfChannels: 2,
        sampleRate: 48_000,
        quality: new Quality({ bitrate: AUDIO_BITRATE }),
      });
      return video.every(Boolean) && audio;
    } catch {
      return false;
    }
  })();
  return support;
}

export interface ComposeSink {
  /** Writes encoded bytes at a file position. */
  write(data: Uint8Array, position: number): Promise<void>;
}

export interface ComposeJob {
  clip: Blob;
  formats: readonly VideoFormat[];
  /** The clip's sound, starting at video time 0. */
  audio: AudioBuffer;
  /** Draws the overlay on top of a finished frame. */
  overlay: (g: CanvasRenderingContext2D, f: VideoFormat) => void;
  /** Opens the file for one format. */
  open: (f: VideoFormat) => Promise<ComposeSink>;
  onProgress: (p: number) => void;
  /** Called with the first format's canvas, to show it while encoding. */
  onCanvas?: (canvas: HTMLCanvasElement) => void;
  /** Diagnostics (the camera's frame size). */
  log?: (message: string) => void;
  cancelled: () => boolean;
}

export class ComposeCancelled extends Error {}

/** Composes every format in one pass over the clip. Returns the duration
 *  written, in seconds. On error or cancel the outputs are abandoned (the
 *  caller discards their files). */
export async function composeOffline(job: ComposeJob): Promise<number> {
  const input = new Input({ source: new BlobSource(job.clip), formats: ALL_FORMATS });
  try {
    const track = await input.getPrimaryVideoTrack();
    if (!track) throw new Error("no video track");
    if (!(await track.canDecode())) throw new Error(`cannot decode ${track.codec ?? "?"}`);
    const duration = await track.computeDuration();
    if (!(duration > 0)) throw new Error("empty clip");
    const frames = Math.max(1, Math.round(duration * COMPOSE_FPS));
    const length = frames / COMPOSE_FPS;

    const delay = (await aacPrimingSamples()) / job.audio.sampleRate;
    const outputs = await Promise.all(job.formats.map((f) => openOutput(job, f, delay)));
    job.onCanvas?.(outputs[0]!.canvas);
    try {
      for (const o of outputs) await o.output.start();
      const sink = new VideoSampleSink(track);
      // Before the camera's first frame there is nothing to show: hold it.
      const first = await sink.getSample(await track.getFirstTimestamp());
      if (first) {
        job.log?.(
          `camera frames ${first.displayWidth}x${first.displayHeight} (coded ${first.codedWidth}x${first.codedHeight}, rotation ${first.rotation})`,
        );
      }
      const stage = new Stage();
      let audioUntil = 0;
      let i = 0;
      try {
        for await (const found of sink.samplesAtTimestamps(timestamps(frames))) {
          if (job.cancelled()) throw new ComposeCancelled();
          const sample = found ?? first;
          if (sample) {
            const picture = stage.show(sample);
            for (const o of outputs) drawFrame(o.g, o.f, picture, job.overlay);
          }
          if (found) found.close();
          const t = i / COMPOSE_FPS;
          await Promise.all(outputs.map((o) => o.video.add(t, 1 / COMPOSE_FPS)));
          i++;
          // Keep the sound just ahead of the picture, so the muxer interleaves.
          while (audioUntil < Math.min(length, t + AUDIO_SLICE_SECS)) {
            const end = Math.min(length, audioUntil + AUDIO_SLICE_SECS);
            const slice = sliceAudio(job.audio, audioUntil, end);
            if (slice) await Promise.all(outputs.map((o) => o.audio.add(slice)));
            audioUntil = end;
          }
          job.onProgress(i / frames);
        }
      } finally {
        first?.close();
      }
      for (const o of outputs) {
        o.video.close();
        o.audio.close();
      }
      await Promise.all(outputs.map((o) => o.output.finalize()));
      await Promise.all(outputs.map((o) => o.flushed()));
      return length;
    } catch (e) {
      await Promise.all(outputs.map((o) => o.output.cancel().catch(() => undefined)));
      throw e;
    }
  } finally {
    input.dispose();
  }
}

async function openOutput(job: ComposeJob, f: VideoFormat, aacDelay: number) {
  const file = await job.open(f);
  const canvas = document.createElement("canvas");
  canvas.width = f.width;
  canvas.height = f.height;
  const g = canvas.getContext("2d", { alpha: false })!;
  g.imageSmoothingQuality = "high";
  // Writes go out in order; the first failure fails the export.
  let pending = Promise.resolve();
  const writable = new WritableStream<StreamTargetChunk>({
    write(chunk) {
      pending = pending.then(() => file.write(chunk.data, chunk.position));
      return pending;
    },
  });
  const output = new Output({
    format: new Mp4OutputFormat({ fastStart: false }),
    target: new StreamTarget(writable, { chunked: true, chunkSize: CHUNK_BYTES }),
  });
  const video = new CanvasSource(canvas, {
    codec: "avc",
    quality: new Quality({ bitrate: videoBitrate(f) }),
    fullCodecString: AVC_CODEC,
    keyFrameInterval: 2,
    // WebKit's encoder holds frames in "quality" mode and the pipeline never
    // gets them back; "realtime" only skips lookahead. Frames are fed with
    // backpressure, so none are dropped (checked frame by frame in tests).
    latencyMode: "realtime",
  });
  // The AAC encoder starts with silence of its own (priming): placing the
  // sound that much earlier makes the muxer write an edit list that tells
  // players to skip it, so the sound lands in sync.
  const audio = new AudioBufferSource(
    { codec: "aac", quality: new Quality({ bitrate: AUDIO_BITRATE }) },
    { startTimestamp: -aacDelay },
  );
  output.addVideoTrack(video, { frameRate: COMPOSE_FPS });
  output.addAudioTrack(audio);
  return { f, canvas, g, output, video, audio, flushed: () => pending };
}

/** Samples of silence the AAC encoder puts before the sound (its priming),
 *  measured once by encoding and decoding a click, since engines differ.
 *  WebKit cannot decode AAC with WebCodecs: its encoder is Apple's, whose
 *  priming is a fixed 2112 samples. */
let aacDelay: Promise<number> | null = null;
export function aacPrimingSamples(): Promise<number> {
  aacDelay ??= measureAacPriming().catch(() => (isAppleWebKit() ? APPLE_AAC_PRIMING : 0));
  return aacDelay;
}

const APPLE_AAC_PRIMING = 2112;

function isAppleWebKit(): boolean {
  const ua = navigator.userAgent;
  return /AppleWebKit/.test(ua) && /Mac OS X/.test(ua) && !/Chrome|Edg\//.test(ua);
}

async function measureAacPriming(): Promise<number> {
  const rate = 48_000;
  const at = 9_600; // the click, 0.2 s in
  const frames = 24_000;
  const data = new Float32Array(frames * 2);
  for (let i = 0; i < 480; i++) data[at + i] = data[frames + at + i] = Math.sin(i * 0.3) * 0.8;
  const chunks: EncodedAudioChunk[] = [];
  let config: AudioDecoderConfig | undefined;
  const encoder = new AudioEncoder({
    output: (chunk, meta) => {
      chunks.push(chunk);
      if (meta?.decoderConfig) config = meta.decoderConfig;
    },
    error: () => undefined,
  });
  encoder.configure({ codec: "mp4a.40.2", sampleRate: rate, numberOfChannels: 2, bitrate: AUDIO_BITRATE });
  encoder.encode(new AudioData({ format: "f32-planar", sampleRate: rate, numberOfFrames: frames, numberOfChannels: 2, timestamp: 0, data }));
  await encoder.flush();
  encoder.close();
  if (!config) return 0;
  const decoded: number[] = [];
  const decoder = new AudioDecoder({
    output: (d) => {
      const plane = new Float32Array(d.numberOfFrames);
      d.copyTo(plane, { planeIndex: 0, format: "f32-planar" });
      d.close();
      decoded.push(...plane);
    },
    error: () => undefined,
  });
  decoder.configure(config);
  for (const c of chunks) decoder.decode(c);
  await decoder.flush();
  decoder.close();
  const heard = decoded.findIndex((x) => Math.abs(x) > 0.4);
  if (heard < 0) return 0;
  const priming = heard - at - 2; // the click passes 0.4 two samples in
  return priming > 0 && priming < rate / 5 ? priming : 0;
}

function* timestamps(frames: number): Generator<number> {
  for (let i = 0; i < frames; i++) yield i / COMPOSE_FPS;
}

/** The camera frame, whole, on a canvas of its own. WebKit ignores the
 *  source rectangle when it draws a camera frame (a decoded `VideoFrame`)
 *  and squeezes the whole picture into every format; cropping from a canvas
 *  is reliable everywhere. */
class Stage {
  private readonly canvas = document.createElement("canvas");
  private readonly g = this.canvas.getContext("2d", { alpha: false })!;

  show(sample: VideoSample): HTMLCanvasElement {
    const { displayWidth: w, displayHeight: h } = sample;
    if (this.canvas.width !== w || this.canvas.height !== h) {
      this.canvas.width = w;
      this.canvas.height = h;
    }
    sample.draw(this.g, 0, 0, w, h);
    return this.canvas;
  }
}

function drawFrame(
  g: CanvasRenderingContext2D,
  f: VideoFormat,
  picture: HTMLCanvasElement,
  overlay: (g: CanvasRenderingContext2D, f: VideoFormat) => void,
): void {
  const { sx, sy, sw, sh } = coverCrop(picture.width, picture.height, f.width, f.height);
  g.drawImage(picture, sx, sy, sw, sh, 0, 0, f.width, f.height);
  overlay(g, f);
}

/** Copy of `buffer` between two times; null when empty. */
function sliceAudio(buffer: AudioBuffer, from: number, to: number): AudioBuffer | null {
  const rate = buffer.sampleRate;
  const start = Math.min(buffer.length, Math.round(from * rate));
  const end = Math.min(buffer.length, Math.round(to * rate));
  // Past the end of the sound: silence keeps the tracks the same length.
  const length = Math.round(to * rate) - Math.round(from * rate);
  if (length <= 0) return null;
  const out = new AudioBuffer({ length, numberOfChannels: buffer.numberOfChannels, sampleRate: rate });
  for (let c = 0; c < buffer.numberOfChannels; c++) {
    if (end > start) out.copyToChannel(buffer.getChannelData(c).subarray(start, end), c);
  }
  return out;
}
