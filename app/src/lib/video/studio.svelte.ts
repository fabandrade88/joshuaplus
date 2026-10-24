// The video studio: films the musician with the webcam while the song plays
// (armed inputs are recorded as takes at the same time), then composes one
// clip per chosen format. The picture is the camera, cropped to the format;
// the sound is the real Joshua+ mix — song and takes, never the click —
// rendered by the backend and lined up with the picture.
import { api, events } from "../api";
import { app } from "../app.svelte";
import { i18n } from "../i18n/index.svelte";
import {
  DEFAULT_SYNC_MS,
  FORMATS,
  coverCrop,
  estimateSongAtVideoStart,
  pickContainer,
  videoBitrate,
  videoOnly,
  type Container,
  type FormatId,
  type SyncSample,
  type VideoFormat,
} from "./formats";

/** Longest take the studio films (the backend renders audio up to 30 min). */
const MAX_TAKE_SECS = 20 * 60;
const FPS = 30;

export type StudioPhase = "idle" | "starting" | "recording" | "saving" | "review" | "exporting";

export interface Camera {
  id: string;
  label: string;
}

interface Clip {
  blob: Blob;
  durationSecs: number;
  /** Song time at video time 0 (from transport samples). */
  songAtStart: number;
}

class VideoStudio {
  phase = $state<StudioPhase>("idle");
  cameras = $state<Camera[]>([]);
  cameraId = $state<string | null>(null);
  stream = $state<MediaStream | null>(null);
  cameraError = $state<string | null>(null);
  formats = $state<Record<FormatId, boolean>>({ youtube: true, vertical: true, square: false, portrait: false });
  showTitle = $state(true);
  /** Picture vs. sound adjustment in ms (negative = sound earlier). */
  syncMs = $state(DEFAULT_SYNC_MS);
  elapsed = $state(0);
  progress = $state(0);
  saved = $state<string[]>([]);
  clip = $state<Clip | null>(null);
  /** Export step shown in the sidebar. */
  exportStage = $state<"audio" | "encoding" | "saving" | null>(null);
  exportError = $state<string | null>(null);
  exportFormats = $state<FormatId[]>([]);
  /** The element being encoded; the sidebar card displays it. */
  composeVideo = $state<HTMLVideoElement | null>(null);
  clipUrl = $state<string | null>(null);
  readonly container: Container | null =
    typeof MediaRecorder === "undefined" ? null : pickContainer((m) => MediaRecorder.isTypeSupported(m));

  private recorder: MediaRecorder | null = null;
  private chunks: Blob[] = [];
  private t0 = 0;
  private samples: SyncSample[] = [];
  private unlisten: (() => void) | null = null;
  private timer: ReturnType<typeof setInterval> | undefined;
  private cancelExport = false;

  get supported(): boolean {
    return !!navigator.mediaDevices?.getUserMedia && this.container !== null;
  }

  get selectedFormats(): VideoFormat[] {
    return FORMATS.filter((f) => this.formats[f.id]);
  }

  // ── Camera ───────────────────────────────────────────────────────────────
  async openCamera(id: string | null = this.cameraId): Promise<void> {
    this.closeCamera();
    this.cameraError = null;
    try {
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: false,
        video: {
          ...(id ? { deviceId: { exact: id } } : {}),
          width: { ideal: 1920 },
          height: { ideal: 1080 },
          frameRate: { ideal: FPS },
        },
      });
      this.stream = stream;
      this.cameraId = stream.getVideoTracks()[0]?.getSettings().deviceId ?? id;
      const devices = await navigator.mediaDevices.enumerateDevices();
      this.cameras = devices
        .filter((d) => d.kind === "videoinput")
        .map((d, i) => ({ id: d.deviceId, label: d.label || `${i18n.t("video.camera")} ${i + 1}` }));
    } catch (e) {
      this.cameraError = e instanceof DOMException && e.name === "NotAllowedError" ? i18n.t("video.denied") : i18n.t("video.noCamera");
    }
  }

  closeCamera(): void {
    this.stream?.getTracks().forEach((t) => t.stop());
    this.stream = null;
  }

  // ── Filming ──────────────────────────────────────────────────────────────
  async start(): Promise<void> {
    if (!this.stream || !this.container || this.phase !== "idle") return;
    this.discardClip();
    this.phase = "starting";
    this.chunks = [];
    this.samples = [];
    const recorder = new MediaRecorder(this.stream, {
      mimeType: videoOnly(this.container),
      videoBitsPerSecond: 12_000_000,
    });
    recorder.ondataavailable = (e) => {
      if (e.data.size > 0) this.chunks.push(e.data);
    };
    this.recorder = recorder;
    await new Promise<void>((resolve) => {
      recorder.onstart = () => resolve();
      recorder.start(1000);
    });
    this.t0 = performance.now();
    this.unlisten = await events.onTransport((t) => {
      if (t.playing && !t.countingIn) this.samples.push({ video: (performance.now() - this.t0) / 1000, song: t.positionSecs });
    });
    if (!(await app.startRecording(true))) {
      this.abortFilming();
      return;
    }
    this.phase = "recording";
    this.timer = setInterval(() => {
      this.elapsed = (performance.now() - this.t0) / 1000;
      // The song ended on its own, or the take is too long.
      if ((!app.transport.playing && !app.transport.countingIn && this.elapsed > 2) || this.elapsed > MAX_TAKE_SECS) {
        void this.stop();
      }
    }, 250);
  }

  async stop(): Promise<void> {
    if (this.phase !== "recording" || !this.recorder) return;
    this.phase = "saving";
    clearInterval(this.timer);
    const recorder = this.recorder;
    const stopped = new Promise<void>((resolve) => (recorder.onstop = () => resolve()));
    recorder.stop();
    const durationSecs = (performance.now() - this.t0) / 1000;
    await stopped;
    this.unlisten?.();
    this.unlisten = null;
    await app.stopRecording(); // aligns and saves the takes recorded meanwhile
    const songAtStart = estimateSongAtVideoStart(this.samples);
    const blob = new Blob(this.chunks, { type: videoOnly(this.container!) });
    this.chunks = [];
    this.recorder = null;
    if (songAtStart === null || blob.size === 0) {
      app.notify(i18n.t("video.nothing"));
      this.phase = "idle";
      return;
    }
    this.clip = { blob, durationSecs, songAtStart };
    this.clipUrl = URL.createObjectURL(blob);
    this.phase = "review";
  }

  private abortFilming(): void {
    clearInterval(this.timer);
    this.unlisten?.();
    this.unlisten = null;
    if (this.recorder?.state === "recording") this.recorder.stop();
    this.recorder = null;
    this.chunks = [];
    this.phase = "idle";
  }

  discardClip(): void {
    this.resetReviewAudio();
    if (this.clipUrl) URL.revokeObjectURL(this.clipUrl);
    this.clipUrl = null;
    this.clip = null;
    this.saved = [];
    if (this.phase === "review") this.phase = "idle";
  }

  // ── Mix audio for the clip ───────────────────────────────────────────────
  private async clipAudio(ctx: AudioContext): Promise<AudioBuffer> {
    const clip = this.clip!;
    const start = clip.songAtStart + this.syncMs / 1000;
    const wav = await api.renderClipAudio(start, clip.durationSecs);
    return ctx.decodeAudioData(wav);
  }

  // ── Review playback with the final sound ───────────────────────────────
  /** State of the review sound (the camera clip itself is silent). */
  reviewAudio = $state<"idle" | "loading" | "ready" | "error">("idle");
  private reviewCtx: AudioContext | null = null;
  private reviewBuffer: AudioBuffer | null = null;
  private reviewSync: number | null = null;
  private reviewSrc: AudioBufferSourceNode | null = null;

  /** Must run inside a click/tap: WebKit only lets audio start from a gesture. */
  unlockAudio(): void {
    this.reviewCtx ??= new AudioContext();
    void this.reviewCtx.resume();
  }

  /** Renders (once per sync value) the mix that goes with the clip. */
  private async reviewBufferFor(sync: number): Promise<AudioBuffer> {
    if (this.reviewBuffer && this.reviewSync === sync) return this.reviewBuffer;
    this.reviewAudio = "loading";
    const ctx = (this.reviewCtx ??= new AudioContext());
    const buffer = await this.clipAudio(ctx);
    this.reviewBuffer = buffer;
    this.reviewSync = sync;
    this.reviewAudio = "ready";
    return buffer;
  }

  private stopReviewSound(): void {
    try {
      this.reviewSrc?.stop();
    } catch {
      /* already stopped */
    }
    this.reviewSrc = null;
  }

  /** Wires the review `<video>` so every play — native controls or the
   *  Preview button — sounds with the real mix, in sync. Returns a cleanup. */
  attachReview(video: HTMLVideoElement): () => void {
    video.muted = true;
    let starting = false;
    const onPlay = async () => {
      if (starting || !this.clip) return;
      this.stopReviewSound();
      const sync = this.syncMs;
      if (!this.reviewBuffer || this.reviewSync !== sync) {
        // Hold the picture until the sound is ready, then resume together.
        starting = true;
        video.pause();
        try {
          await this.reviewBufferFor(sync);
        } catch {
          this.reviewAudio = "error";
          starting = false;
          return;
        }
        starting = false;
        void video.play();
        return;
      }
      const ctx = this.reviewCtx!;
      await ctx.resume();
      const src = ctx.createBufferSource();
      src.buffer = this.reviewBuffer;
      src.connect(ctx.destination);
      src.start(ctx.currentTime, Math.min(video.currentTime, this.reviewBuffer.duration));
      this.reviewSrc = src;
    };
    const onStop = () => {
      if (!starting) this.stopReviewSound();
    };
    video.addEventListener("playing", onPlay);
    video.addEventListener("pause", onStop);
    video.addEventListener("seeking", onStop);
    video.addEventListener("ended", onStop);
    return () => {
      video.removeEventListener("playing", onPlay);
      video.removeEventListener("pause", onStop);
      video.removeEventListener("seeking", onStop);
      video.removeEventListener("ended", onStop);
      this.stopReviewSound();
    };
  }

  /** Plays the clip from the start with its final sound. */
  async preview(video: HTMLVideoElement): Promise<void> {
    if (!this.clip) return;
    this.unlockAudio();
    video.pause();
    video.currentTime = 0;
    await video.play();
  }

  /** Drops the review sound (new clip, or the studio closed). */
  private resetReviewAudio(): void {
    this.stopReviewSound();
    this.reviewBuffer = null;
    this.reviewSync = null;
    this.reviewAudio = "idle";
  }

  // ── Export ───────────────────────────────────────────────────────────────
  cancel(): void {
    this.cancelExport = true;
  }

  /** Composes and saves one file per selected format, in a single pass.
   *  Runs in the background: the studio can close while it works, and the
   *  sidebar shows its progress (and hosts the visible preview element). */
  async export(): Promise<void> {
    const clip = this.clip;
    const container = this.container;
    const formats = this.selectedFormats;
    if (!clip || !container || formats.length === 0 || this.phase !== "review") return;
    this.phase = "exporting";
    this.exportStage = "audio";
    this.exportError = null;
    this.progress = 0;
    this.cancelExport = false;
    this.saved = [];
    this.exportFormats = formats.map((f) => f.id);
    // Created inside the click, so the browser lets it play sound.
    const ctx = new AudioContext();
    const video = document.createElement("video");
    video.muted = true;
    video.playsInline = true;
    video.preload = "auto";
    video.className = "compose-video";
    video.src = this.clipUrl!;
    // WebKit pauses muted videos that are not visible: the sidebar card
    // shows this element as a live thumbnail while exporting.
    this.composeVideo = video;
    const ready = whenReady(video); // listen now, before any await
    const handles: number[] = [];
    try {
      await ctx.resume();
      const buffer = await this.clipAudio(ctx);
      if (this.cancelExport) throw new Cancelled();
      await ready;
      await ensureInDocument(video);
      this.exportStage = "encoding";
      const dest = ctx.createMediaStreamDestination();
      const src = ctx.createBufferSource();
      src.buffer = buffer;
      src.connect(dest);
      const title = app.song?.project.title ?? "";
      const artist = app.song?.project.metadata?.artist ?? "";

      const outputs = await Promise.all(
        formats.map(async (f) => {
          const canvas = document.createElement("canvas");
          canvas.width = f.width;
          canvas.height = f.height;
          const g = canvas.getContext("2d")!;
          const stream = canvas.captureStream(FPS);
          for (const t of dest.stream.getAudioTracks()) stream.addTrack(t.clone());
          const rec = new MediaRecorder(stream, {
            mimeType: container.mime,
            videoBitsPerSecond: videoBitrate(f),
            audioBitsPerSecond: 256_000,
          });
          const handle = await api.videoSaveBegin(f.id, container.ext);
          handles.push(handle);
          let queue = Promise.resolve();
          let failed: unknown = null;
          rec.ondataavailable = (e) => {
            if (e.data.size === 0) return;
            queue = queue
              .then(async () => api.videoSaveChunk(handle, new Uint8Array(await e.data.arrayBuffer())))
              .catch((err: unknown) => {
                failed = err;
              });
          };
          const finished = new Promise<void>((resolve) => (rec.onstop = () => resolve()));
          return { f, g, rec, handle, finished, flush: () => queue, failed: () => failed };
        }),
      );

      const draw = () => {
        const vw = video.videoWidth;
        const vh = video.videoHeight;
        for (const o of outputs) {
          const { sx, sy, sw, sh } = coverCrop(vw, vh, o.f.width, o.f.height);
          o.g.drawImage(video, sx, sy, sw, sh, 0, 0, o.f.width, o.f.height);
          if (this.showTitle && title) drawTitle(o.g, o.f, title, artist);
        }
        this.progress = Math.min(1, video.currentTime / Math.max(clip.durationSecs, 0.1));
      };
      let frameLoop: number | undefined;
      const tick = () => {
        draw();
        if (!video.ended) frameLoop = requestAnimationFrame(tick);
      };

      for (const o of outputs) o.rec.start(1000);
      draw();
      await new Promise<void>((resolve, reject) => {
        let lastTime = -1;
        let lastMove = performance.now();
        const watchdog = setInterval(() => {
          if (this.cancelExport) {
            clearInterval(watchdog);
            video.pause();
            resolve();
            return;
          }
          if (video.currentTime !== lastTime) {
            lastTime = video.currentTime;
            lastMove = performance.now();
          } else if (performance.now() - lastMove > STALL_MS) {
            clearInterval(watchdog);
            video.pause();
            reject(new Error("stalled"));
          } else if (video.paused && !video.ended) {
            void video.play().catch(() => undefined); // paused by the system: resume
          }
        }, 500);
        video.onplaying = () => {
          video.onplaying = null;
          src.start(ctx.currentTime, video.currentTime);
          tick();
        };
        video.onended = () => {
          clearInterval(watchdog);
          resolve();
        };
        video.play().catch(() => {
          clearInterval(watchdog);
          reject(new Error("play"));
        });
      });
      if (frameLoop !== undefined) cancelAnimationFrame(frameLoop);
      try {
        src.stop();
      } catch {
        /* already stopped */
      }
      this.exportStage = "saving";
      for (const o of outputs) o.rec.stop();
      await Promise.all(outputs.map((o) => o.finished));
      await Promise.all(outputs.map((o) => o.flush()));
      const keep = !this.cancelExport && outputs.every((o) => o.failed() === null);
      for (const o of outputs) {
        const path = await api.videoSaveEnd(o.handle, keep);
        if (path) this.saved = [...this.saved, path];
      }
      handles.length = 0;
      if (this.cancelExport) app.notify(i18n.t("video.exportCancelled"));
      else if (!keep) throw new Error("save");
      else app.notify(i18n.t("video.exported", { n: this.saved.length }));
    } catch (e) {
      if (!(e instanceof Cancelled)) {
        this.exportError = i18n.t("video.exportFailed");
        app.notify(this.exportError);
      }
    } finally {
      for (const h of handles) void api.videoSaveEnd(h, false).catch(() => undefined);
      video.pause();
      video.removeAttribute("src");
      video.load();
      video.remove();
      this.composeVideo = null;
      this.exportStage = null;
      void ctx.close();
      this.phase = "review";
    }
  }

  /** Closes the studio window. While exporting, the export keeps running in
   *  the background (shown in the sidebar); otherwise the clip is dropped. */
  async close(): Promise<void> {
    if (this.phase === "recording") await this.stop();
    this.closeCamera();
    if (this.phase === "exporting") return;
    this.discardClip();
    this.phase = "idle";
  }

  /** Forgets a finished export (clears the sidebar card). */
  dismissExport(): void {
    if (this.phase === "exporting") return;
    this.saved = [];
    this.exportError = null;
    this.exportFormats = [];
    if (!app.videoOpen) this.discardClip();
  }
}

class Cancelled extends Error {}

/** No picture progress for this long means the export is stuck. */
const STALL_MS = 8_000;

/** Resolves once the video can show its first frame (listens immediately). */
function whenReady(video: HTMLVideoElement): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    if (video.readyState >= HTMLMediaElement.HAVE_CURRENT_DATA) return resolve();
    const timer = setTimeout(() => reject(new Error("load timeout")), 15_000);
    video.addEventListener("loadeddata", () => (clearTimeout(timer), resolve()), { once: true });
    video.addEventListener("error", () => (clearTimeout(timer), reject(new Error("video"))), { once: true });
  });
}

/** The sidebar card adopts the element; if it has not (e.g. the sidebar is
 *  hidden), park it in a small visible corner so WebKit keeps playing it. */
async function ensureInDocument(video: HTMLVideoElement): Promise<void> {
  for (let i = 0; i < 10 && !video.isConnected; i++) await new Promise((r) => setTimeout(r, 50));
  if (video.isConnected) return;
  video.classList.add("compose-video-parked");
  document.body.append(video);
}

/** Song title (and artist) in the lower left, over a soft shade. */
function drawTitle(g: CanvasRenderingContext2D, f: VideoFormat, title: string, artist: string): void {
  const unit = Math.min(f.width, f.height) / 1080;
  const pad = 48 * unit;
  const shade = g.createLinearGradient(0, f.height * 0.72, 0, f.height);
  shade.addColorStop(0, "rgba(0,0,0,0)");
  shade.addColorStop(1, "rgba(0,0,0,0.55)");
  g.fillStyle = shade;
  g.fillRect(0, f.height * 0.72, f.width, f.height * 0.28);
  g.textBaseline = "alphabetic";
  g.fillStyle = "#ffffff";
  g.font = `800 ${Math.round(46 * unit)}px system-ui, -apple-system, Segoe UI, sans-serif`;
  const y = f.height - pad - (artist ? 40 * unit : 0);
  g.fillText(title, pad, y, f.width - pad * 2);
  if (artist) {
    g.fillStyle = "rgba(255,255,255,0.75)";
    g.font = `600 ${Math.round(30 * unit)}px system-ui, -apple-system, Segoe UI, sans-serif`;
    g.fillText(artist, pad, f.height - pad, f.width - pad * 2);
  }
  g.fillStyle = "#ffb000";
  g.fillRect(pad, y - 62 * unit, 56 * unit, 6 * unit);
}

export const studio = new VideoStudio();
