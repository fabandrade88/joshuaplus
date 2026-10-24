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

  /** Plays the clip with its final sound, to check the sync. */
  async preview(video: HTMLVideoElement): Promise<void> {
    if (!this.clip) return;
    const ctx = new AudioContext();
    const buffer = await this.clipAudio(ctx);
    const src = ctx.createBufferSource();
    src.buffer = buffer;
    src.connect(ctx.destination);
    video.muted = true;
    video.currentTime = 0;
    const done = () => {
      try {
        src.stop();
      } catch {
        /* already stopped */
      }
      void ctx.close();
    };
    video.onpause = done;
    video.onended = done;
    video.onplaying = () => {
      video.onplaying = null;
      src.start(ctx.currentTime, video.currentTime);
    };
    await video.play();
  }

  // ── Export ───────────────────────────────────────────────────────────────
  cancel(): void {
    this.cancelExport = true;
  }

  /** Composes and saves one file per selected format, in a single pass. */
  async export(): Promise<void> {
    const clip = this.clip;
    const container = this.container;
    const formats = this.selectedFormats;
    if (!clip || !container || formats.length === 0 || this.phase !== "review") return;
    this.phase = "exporting";
    this.progress = 0;
    this.cancelExport = false;
    this.saved = [];
    const ctx = new AudioContext();
    const video = document.createElement("video");
    video.muted = true;
    video.playsInline = true;
    video.src = this.clipUrl!;
    const handles: number[] = [];
    try {
      const buffer = await this.clipAudio(ctx);
      await new Promise<void>((resolve, reject) => {
        video.onloadeddata = () => resolve();
        video.onerror = () => reject(new Error("video"));
      });
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
          rec.ondataavailable = (e) => {
            if (e.data.size === 0) return;
            queue = queue.then(async () => api.videoSaveChunk(handle, new Uint8Array(await e.data.arrayBuffer())));
          };
          const finished = new Promise<void>((resolve) => (rec.onstop = () => resolve()));
          return { f, g, rec, handle, finished, flush: () => queue };
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
        if (!video.ended && !video.paused) frameLoop = requestAnimationFrame(tick);
      };

      for (const o of outputs) o.rec.start(1000);
      draw();
      await new Promise<void>((resolve) => {
        video.onplaying = () => {
          video.onplaying = null;
          src.start(ctx.currentTime, video.currentTime);
          tick();
        };
        const guard = setInterval(() => {
          if (this.cancelExport) {
            clearInterval(guard);
            video.pause();
            resolve();
          }
        }, 200);
        video.onended = () => {
          clearInterval(guard);
          resolve();
        };
        void video.play();
      });
      if (frameLoop !== undefined) cancelAnimationFrame(frameLoop);
      try {
        src.stop();
      } catch {
        /* already stopped */
      }
      for (const o of outputs) o.rec.stop();
      await Promise.all(outputs.map((o) => o.finished));
      await Promise.all(outputs.map((o) => o.flush()));
      for (const o of outputs) {
        const path = await api.videoSaveEnd(o.handle, !this.cancelExport);
        if (path) this.saved = [...this.saved, path];
      }
      handles.length = 0;
      if (!this.cancelExport) app.notify(i18n.t("video.exported", { n: this.saved.length }));
    } catch {
      app.notify(i18n.t("video.exportFailed"));
    } finally {
      for (const h of handles) void api.videoSaveEnd(h, false).catch(() => undefined);
      video.removeAttribute("src");
      void ctx.close();
      this.phase = "review";
    }
  }

  /** Closes the studio: stops filming, frees the camera and the clip. */
  async close(): Promise<void> {
    if (this.phase === "recording") await this.stop();
    if (this.phase === "exporting") this.cancel();
    this.closeCamera();
    this.discardClip();
    this.phase = "idle";
  }
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
