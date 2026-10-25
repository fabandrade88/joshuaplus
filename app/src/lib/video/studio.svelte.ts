// The video studio: films the musician with the webcam while the song plays
// (armed inputs are recorded as takes at the same time), then composes one
// clip per chosen format. The picture is the camera, cropped to the format;
// the sound is the real Joshua+ mix — song and takes, never the click —
// rendered by the backend and lined up with the picture.
import { api, events, isApiError } from "../api";
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
  /** The song it was filmed on: its sound and overlay come from this song
   *  even if another one is open later. */
  song: ClipSong;
}

interface ClipSong {
  id: string;
  title: string;
  artist: string;
  cover: string | null;
}

/** The open song, as a clip remembers it. */
function currentClipSong(): ClipSong {
  const s = app.song;
  return {
    id: s?.project.id ?? "",
    title: s?.project.title ?? "",
    artist: s?.project.metadata?.artist ?? "",
    cover: s?.cover ?? null,
  };
}

/** Parts of the overlay burned into exported clips. */
export type Overlay = { cover: boolean; title: boolean; artist: boolean };

const OVERLAY_KEY = "joshua.videoOverlay";

function readOverlay(): Overlay {
  const all: Overlay = { cover: true, title: true, artist: true };
  try {
    const v: unknown = JSON.parse(localStorage.getItem(OVERLAY_KEY) ?? "null");
    if (v && typeof v === "object") {
      const o = v as Partial<Record<keyof Overlay, unknown>>;
      return { cover: o.cover !== false, title: o.title !== false, artist: o.artist !== false };
    }
  } catch {
    // Unreadable or blocked storage: everything on.
  }
  return all;
}

class VideoStudio {
  phase = $state<StudioPhase>("idle");
  cameras = $state<Camera[]>([]);
  cameraId = $state<string | null>(null);
  stream = $state<MediaStream | null>(null);
  cameraError = $state<string | null>(null);
  /** What the open camera actually delivers. */
  cameraMode = $state<{ width: number; height: number; fps: number } | null>(null);
  formats = $state<Record<FormatId, boolean>>({ youtube: true, vertical: true, square: false, portrait: false });
  /** What the export burns into the picture; each part can be turned off. */
  overlay = $state<Overlay>(readOverlay());
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
  /** The element being encoded (the clip playing, or the first format's
   *  frame); the sidebar card displays it. */
  composeVideo = $state<HTMLVideoElement | HTMLCanvasElement | null>(null);
  clipUrl = $state<string | null>(null);
  readonly container: Container | null =
    typeof MediaRecorder === "undefined" ? null : pickContainer((m) => MediaRecorder.isTypeSupported(m));

  private recorder: MediaRecorder | null = null;

  /** Turns one overlay part on or off and remembers the choice. */
  setOverlay(part: keyof Overlay, on: boolean): void {
    this.overlay[part] = on;
    try {
      localStorage.setItem(OVERLAY_KEY, JSON.stringify(this.overlay));
    } catch {
      // Storage unavailable: the choice lasts for this session only.
    }
  }

  private chunks: Blob[] = [];
  private filmedSong: ClipSong | null = null;
  private t0 = 0;
  private samples: SyncSample[] = [];
  private unlisten: (() => void) | null = null;
  private timer: ReturnType<typeof setInterval> | undefined;
  private cancelExport = false;
  /** Plain (non-reactive) flag: Svelte shows teardown code the state from
   *  before the change that closed the window, so `phase` can read stale
   *  "review" in `close()` right after an export started. */
  private exportRunning = false;

  get supported(): boolean {
    return !!navigator.mediaDevices?.getUserMedia && this.container !== null;
  }

  get selectedFormats(): VideoFormat[] {
    return FORMATS.filter((f) => this.formats[f.id]);
  }

  // ── Camera ───────────────────────────────────────────────────────────────
  /** Opens a camera: the given one, else the one used last time (matched by
   *  name: ids can change between runs), else any. A camera that is gone or
   *  busy falls back to whichever is available. */
  async openCamera(id: string | null = this.cameraId): Promise<void> {
    this.closeCamera();
    this.cameraError = null;
    await this.refreshCameras();
    const remembered = readCameraLabel();
    const wanted =
      (id && this.cameras.some((c) => c.id === id) ? id : null) ??
      this.cameras.find((c) => remembered !== null && c.label === remembered)?.id ??
      null;
    let stream: MediaStream | null = null;
    let error: unknown = null;
    for (const choice of wanted ? [wanted, null] : [null]) {
      try {
        stream = await navigator.mediaDevices.getUserMedia({
          audio: false,
          video: {
            ...(choice ? { deviceId: { exact: choice } } : {}),
            width: { ideal: 1920 },
            height: { ideal: 1080 },
            frameRate: { ideal: FPS },
          },
        });
        break;
      } catch (e) {
        error = e;
        if (e instanceof DOMException && e.name === "NotAllowedError") break;
      }
    }
    if (!stream) {
      this.cameraError =
        error instanceof DOMException && error.name === "NotAllowedError" ? i18n.t("video.denied") : i18n.t("video.noCamera");
      return;
    }
    this.stream = stream;
    const track = stream.getVideoTracks()[0];
    // "ideal" lets some webcams settle for 720p: insist on 1080p when the
    // camera can do it (if not, it keeps what it had).
    if (track && (track.getSettings().height ?? 0) < 1080) {
      await track
        .applyConstraints({ width: { exact: 1920 }, height: { exact: 1080 }, frameRate: { ideal: FPS } })
        .catch(() => undefined);
    }
    const settings = track?.getSettings();
    this.cameraMode = settings?.width && settings.height
      ? { width: settings.width, height: settings.height, fps: Math.round(settings.frameRate ?? 0) }
      : null;
    this.cameraId = settings?.deviceId ?? wanted;
    // Labels are only readable once a camera is open.
    await this.refreshCameras();
  }

  /** Lists the cameras connected to the computer. */
  async refreshCameras(): Promise<void> {
    try {
      const devices = await navigator.mediaDevices.enumerateDevices();
      this.cameras = devices
        .filter((d) => d.kind === "videoinput" && d.deviceId !== "")
        .map((d, i) => ({ id: d.deviceId, label: d.label || `${i18n.t("video.camera")} ${i + 1}` }));
    } catch {
      this.cameras = [];
    }
  }

  /** The user picked a camera: open it and remember it for next time. */
  async chooseCamera(id: string): Promise<void> {
    const label = this.cameras.find((c) => c.id === id)?.label;
    if (label) writeCameraLabel(label);
    await this.openCamera(id);
  }

  /** Follows cameras being plugged in or out while the studio is open.
   *  Returns a cleanup. */
  watchCameras(): () => void {
    const media = navigator.mediaDevices;
    if (!media?.addEventListener) return () => {};
    const onChange = async () => {
      const before = this.cameraId;
      await this.refreshCameras();
      if (this.phase !== "idle") return; // never swap cameras mid-take
      const gone = before !== null && !this.cameras.some((c) => c.id === before);
      const ended = this.stream?.getVideoTracks().every((t) => t.readyState === "ended") ?? true;
      if (gone || ended || !this.stream) await this.openCamera(null);
    };
    media.addEventListener("devicechange", onChange);
    return () => media.removeEventListener("devicechange", onChange);
  }

  closeCamera(): void {
    this.stream?.getTracks().forEach((t) => t.stop());
    this.stream = null;
    this.cameraMode = null;
  }

  // ── Filming ──────────────────────────────────────────────────────────────
  async start(): Promise<void> {
    if (!this.stream || !this.container || this.phase !== "idle") return;
    this.discardClip();
    this.phase = "starting";
    this.filmedSong = currentClipSong();
    this.chunks = [];
    this.samples = [];
    const recorder = new MediaRecorder(this.stream, {
      mimeType: videoOnly(this.container),
      // The source of every export: keep it well above the export bitrates.
      videoBitsPerSecond: 24_000_000,
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
    this.clip = { blob, durationSecs, songAtStart, song: this.filmedSong ?? currentClipSong() };
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
    if (this.exportRunning) return; // the export still needs the clip
    this.resetReviewAudio();
    if (this.clipUrl) URL.revokeObjectURL(this.clipUrl);
    this.clipUrl = null;
    this.clip = null;
    this.saved = [];
    if (this.phase === "review") this.phase = "idle";
    // Filming again: the camera may have been closed while the clip waited.
    if (app.videoOpen && this.phase === "idle" && !this.stream) void this.openCamera();
  }

  /** The clip under review was filmed on another song than the open one
   *  (its sound and overlay belong to that song). */
  get clipFromOtherSong(): boolean {
    return !!this.clip && this.clip.song.id !== (app.song?.project.id ?? "");
  }

  // ── Mix audio for the clip ───────────────────────────────────────────────
  /** The clip's sound for the current sync: reuses the one prepared for
   *  the review player when possible (rendering it is the slow part). */
  private async audioFor(ctx: AudioContext): Promise<AudioBuffer> {
    if (this.reviewFresh(this.syncMs)) return this.reviewBuffer!;
    return this.clipAudio(ctx);
  }

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
  /** Follows the review player's volume and mute buttons. */
  private reviewGain: GainNode | null = null;
  /** Bumped on every mix change made while reviewing: the prepared sound
   *  is then stale and gets rendered again (for preview and export). */
  private mixVersion = 0;
  private reviewMix = -1;
  private reviewVideo: HTMLVideoElement | null = null;
  private remixTimer: ReturnType<typeof setTimeout> | undefined;

  private reviewFresh(sync: number): boolean {
    return this.reviewBuffer !== null && this.reviewSync === sync && this.reviewMix === this.mixVersion;
  }

  /** The mix was edited in the review: re-render the clip's sound. If the
   *  clip is playing, it picks the new sound up where it is (debounced, so
   *  dragging a slider does not render on every step). */
  mixChanged(): void {
    this.mixVersion++;
    clearTimeout(this.remixTimer);
    this.remixTimer = setTimeout(() => {
      const v = this.reviewVideo;
      if (v && !v.paused) {
        v.pause();
        void v.play();
      }
    }, 350);
  }

  /** Must run inside a click/tap: WebKit only lets audio start from a gesture. */
  unlockAudio(): void {
    this.reviewCtx ??= new AudioContext();
    void this.reviewCtx.resume();
  }

  /** Renders (once per sync value) the mix that goes with the clip. */
  private async reviewBufferFor(sync: number): Promise<AudioBuffer> {
    if (this.reviewFresh(sync)) return this.reviewBuffer!;
    this.reviewAudio = "loading";
    const ctx = (this.reviewCtx ??= new AudioContext());
    const mix = this.mixVersion;
    const buffer = await this.clipAudio(ctx);
    this.reviewBuffer = buffer;
    this.reviewSync = sync;
    this.reviewMix = mix;
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
    // The camera clip has no sound track of its own: the player's volume and
    // mute buttons drive the mix played next to it.
    video.muted = false;
    this.reviewVideo = video;
    const follow = () => {
      if (this.reviewGain) this.reviewGain.gain.value = video.muted ? 0 : video.volume;
    };
    let starting = false;
    const onPlay = async () => {
      if (starting || !this.clip || this.clipFromOtherSong) return;
      this.stopReviewSound();
      const sync = this.syncMs;
      if (!this.reviewFresh(sync)) {
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
      const buffer = this.reviewBuffer!;
      src.buffer = buffer;
      if (!this.reviewGain) {
        this.reviewGain = ctx.createGain();
        this.reviewGain.connect(ctx.destination);
      }
      follow();
      src.connect(this.reviewGain);
      src.start(ctx.currentTime, Math.min(video.currentTime, buffer.duration));
      this.reviewSrc = src;
    };
    const onStop = () => {
      if (!starting) this.stopReviewSound();
    };
    video.addEventListener("playing", onPlay);
    video.addEventListener("pause", onStop);
    video.addEventListener("seeking", onStop);
    video.addEventListener("ended", onStop);
    video.addEventListener("volumechange", follow);
    return () => {
      video.removeEventListener("volumechange", follow);
      video.removeEventListener("playing", onPlay);
      video.removeEventListener("pause", onStop);
      video.removeEventListener("seeking", onStop);
      video.removeEventListener("ended", onStop);
      if (this.reviewVideo === video) this.reviewVideo = null;
      clearTimeout(this.remixTimer);
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
    this.reviewMix = -1;
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
    // The sound is rendered from the open song: it must be the clip's song.
    if (this.clipFromOtherSong) return;
    this.exportRunning = true;
    this.phase = "exporting";
    this.exportStage = "audio";
    this.exportError = null;
    this.progress = 0;
    this.cancelExport = false;
    this.saved = [];
    this.exportFormats = formats.map((f) => f.id);
    // Created inside the click, so the browser lets it play sound.
    const ctx = new AudioContext();
    const handles: number[] = [];
    const t0 = performance.now();
    const step = (what: string) => trace(`${what} (+${Math.round(performance.now() - t0)} ms)`);
    let video: HTMLVideoElement | null = null;
    // Loaded on first export: the media library is large.
    const compose = await import("./compose");
    try {
      const offline = await compose.canComposeOffline();
      if (!offline) {
        // Real time: start loading the picture while the sound renders.
        video = this.composeElement();
        await ensureInDocument(video);
        video.src = this.clipUrl!;
        video.load();
        step(`video loading, in page: ${video.isConnected}`);
      }
      await ctx.resume();
      const buffer = await this.audioFor(ctx);
      step(`sound ready: ${buffer.duration.toFixed(1)} s`);
      if (this.cancelExport) throw new Cancelled();
      const ov = { ...this.overlay };
      // Always the song the clip was filmed on.
      const title = ov.title ? clip.song.title : "";
      const artist = ov.artist ? clip.song.artist : "";
      const cover = ov.cover ? await loadImage(clip.song.cover) : null;
      const withOverlay = !!(title || artist || cover);
      const overlay = (g: CanvasRenderingContext2D, f: VideoFormat) => {
        if (withOverlay) drawTitle(g, f, title, artist, cover);
      };
      this.exportStage = "encoding";
      const keep = offline
        ? await this.composeFrames(compose, clip, formats, buffer, overlay, handles, step)
        : await this.composeRealtime(video!, ctx, clip, container, formats, buffer, overlay, handles, step);
      for (const h of handles) {
        const path = await api.videoSaveEnd(h, keep, offline);
        if (path) this.saved = [...this.saved, path];
      }
      handles.length = 0;
      step(`saved ${this.saved.length} file(s)`);
      if (this.cancelExport) app.notify(i18n.t("video.exportCancelled"));
      else if (!keep) throw new Error("save");
      else app.notify(i18n.t("video.exported", { n: this.saved.length }));
      void app.refreshExports();
    } catch (e) {
      if (!(e instanceof Cancelled) && !(e instanceof compose.ComposeCancelled)) {
        const why = e instanceof Error ? e.message : isApiError(e) ? e.code : String(e);
        trace(
          `failed: ${why}` +
            (video ? `; video readyState=${video.readyState} networkState=${video.networkState} error=${video.error?.code ?? "-"}` : ""),
        );
        this.exportError = `${i18n.t("video.exportFailed")} (${why})`;
        app.notify(this.exportError);
      } else {
        app.notify(i18n.t("video.exportCancelled"));
      }
    } finally {
      for (const h of handles) void api.videoSaveEnd(h, false).catch(() => undefined);
      if (video) {
        video.pause();
        video.removeAttribute("src");
        video.load();
      }
      this.composeVideo?.remove();
      this.composeVideo = null;
      this.exportStage = null;
      void ctx.close();
      this.exportRunning = false;
      this.phase = "review";
    }
  }

  /** The hidden player the real-time path encodes from (shown as the
   *  sidebar thumbnail). */
  private composeElement(): HTMLVideoElement {
    const video = document.createElement("video");
    video.muted = true;
    video.playsInline = true;
    video.preload = "auto";
    video.className = "compose-video";
    // WebKit pauses (and may not even load) muted videos that are not in
    // the page and visible: the sidebar card shows this element as a live
    // thumbnail. Put it in the page first, then load it.
    this.composeVideo = video;
    return video;
  }

  /** Frame-by-frame export (WebCodecs). Returns whether to keep the files. */
  private async composeFrames(
    compose: typeof import("./compose"),
    clip: Clip,
    formats: VideoFormat[],
    audio: AudioBuffer,
    overlay: (g: CanvasRenderingContext2D, f: VideoFormat) => void,
    handles: number[],
    step: (what: string) => void,
  ): Promise<boolean> {
    step(`composing ${formats.length} format(s) frame by frame`);
    const length = await compose.composeOffline({
      clip: clip.blob,
      formats,
      audio,
      overlay,
      open: async (f) => {
        const handle = await api.videoSaveBegin(f.id, "mp4");
        handles.push(handle);
        return { write: (data, position) => api.videoSaveChunk(handle, data, position) };
      },
      onProgress: (p) => (this.progress = p),
      log: step,
      onCanvas: (canvas) => {
        canvas.className = "compose-video";
        this.composeVideo = canvas;
      },
      cancelled: () => this.cancelExport,
    });
    this.exportStage = "saving";
    step(`picture done: ${length.toFixed(1)} s`);
    return true;
  }

  /** Real-time export (MediaRecorder on a canvas while the clip plays), for
   *  engines without WebCodecs. Returns whether to keep the files. */
  private async composeRealtime(
    video: HTMLVideoElement,
    ctx: AudioContext,
    clip: Clip,
    container: Container,
    formats: VideoFormat[],
    buffer: AudioBuffer,
    overlay: (g: CanvasRenderingContext2D, f: VideoFormat) => void,
    handles: number[],
    step: (what: string) => void,
  ): Promise<boolean> {
    await whenReady(video);
    step(`video ready ${video.videoWidth}x${video.videoHeight}`);
    const dest = ctx.createMediaStreamDestination();
    const src = ctx.createBufferSource();
    src.buffer = buffer;
    src.connect(dest);

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
        return { f, g, rec, finished, flush: () => queue, failed: () => failed };
      }),
    );

    const draw = () => {
      const vw = video.videoWidth;
      const vh = video.videoHeight;
      for (const o of outputs) {
        const { sx, sy, sw, sh } = coverCrop(vw, vh, o.f.width, o.f.height);
        o.g.drawImage(video, sx, sy, sw, sh, 0, 0, o.f.width, o.f.height);
        overlay(o.g, o.f);
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
    step(`encoding ${outputs.length} format(s) as ${container.mime}`);
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
    step(`picture done at ${video.currentTime.toFixed(1)} s`);
    for (const o of outputs) o.rec.stop();
    await Promise.all(outputs.map((o) => o.finished));
    await Promise.all(outputs.map((o) => o.flush()));
    return !this.cancelExport && outputs.every((o) => o.failed() === null);
  }

  /** Closes the studio window. While exporting, the export keeps running in
   *  the background (shown in the sidebar); otherwise the clip is dropped. */
  async close(): Promise<void> {
    if (this.phase === "recording") await this.stop();
    this.closeCamera();
    if (this.exportRunning) return;
    this.discardClip();
    this.phase = "idle";
  }

  /** Forgets a finished export (clears the sidebar card). */
  dismissExport(): void {
    if (this.exportRunning) return;
    this.saved = [];
    this.exportError = null;
    this.exportFormats = [];
    if (!app.videoOpen) this.discardClip();
  }
}

class Cancelled extends Error {}

/** No picture progress for this long means the export is stuck. */
const STALL_MS = 8_000;

/** Resolves once the video can show its first frame. Some engines only
 *  report `canplay`, or need a nudge (`play` then `pause`) to decode. */
function whenReady(video: HTMLVideoElement): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    if (video.readyState >= HTMLMediaElement.HAVE_CURRENT_DATA) return resolve();
    let nudged = false;
    const poll = setInterval(() => {
      if (video.readyState >= HTMLMediaElement.HAVE_CURRENT_DATA) return done();
      if (!nudged && performance.now() - started > 3_000) {
        nudged = true;
        void video
          .play()
          .then(() => video.pause())
          .catch(() => undefined);
      }
      if (performance.now() - started > 20_000) {
        cleanup();
        reject(new Error(`load timeout (readyState ${video.readyState}, network ${video.networkState})`));
      }
    }, 250);
    const started = performance.now();
    const cleanup = () => {
      clearInterval(poll);
      video.removeEventListener("loadeddata", done);
      video.removeEventListener("canplay", done);
      video.removeEventListener("error", fail);
    };
    const done = () => {
      cleanup();
      resolve();
    };
    const fail = () => {
      cleanup();
      reject(new Error(`video error ${video.error?.code ?? "?"}`));
    };
    video.addEventListener("loadeddata", done);
    video.addEventListener("canplay", done);
    video.addEventListener("error", fail);
  });
}

/** Diagnostics for exports (shown in the dev log). */
function trace(message: string): void {
  console.warn(`[video export] ${message}`);
}

/** The sidebar card adopts the element; if it has not (e.g. the sidebar is
 *  hidden), park it in a small visible corner so WebKit keeps playing it. */
async function ensureInDocument(video: HTMLVideoElement): Promise<void> {
  for (let i = 0; i < 10 && !video.isConnected; i++) await new Promise((r) => setTimeout(r, 50));
  if (video.isConnected) return;
  video.classList.add("compose-video-parked");
  document.body.append(video);
}

/** Loads the album cover (a `data:` URL from the backend); `null` if absent
 *  or broken, so the overlay simply goes without it. */
export function loadImage(src: string | null): Promise<HTMLImageElement | null> {
  if (!src) return Promise.resolve(null);
  return new Promise((resolve) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => resolve(null);
    img.src = src;
  });
}

const CAMERA_KEY = "joshua.video.camera";

function readCameraLabel(): string | null {
  try {
    return localStorage.getItem(CAMERA_KEY);
  } catch {
    return null;
  }
}

function writeCameraLabel(label: string): void {
  try {
    localStorage.setItem(CAMERA_KEY, label);
  } catch {
    // Remembering the camera is a convenience only.
  }
}

/** Album cover, song title and artist in the lower left, over a soft shade.
 *  Any part may be missing (empty text / null cover); with nothing to show it
 *  draws nothing at all. */
export function drawTitle(
  g: CanvasRenderingContext2D,
  f: VideoFormat,
  title: string,
  artist: string,
  cover: HTMLImageElement | null,
): void {
  if (!title && !artist && !cover) return;
  // Bottom-left corner in every format, small enough to leave the picture
  // to the musician; a touch larger on tall frames, which phones show small.
  const tall = f.height > f.width * 1.2;
  const unit = (Math.min(f.width, f.height) / 1080) * (tall ? 1.1 : 1);
  const pad = 40 * unit;
  const shadeTop = f.height * (tall ? 0.84 : 0.76);
  const shade = g.createLinearGradient(0, shadeTop, 0, f.height);
  shade.addColorStop(0, "rgba(0,0,0,0)");
  shade.addColorStop(1, "rgba(0,0,0,0.55)");
  g.fillStyle = shade;
  g.fillRect(0, shadeTop, f.width, f.height - shadeTop);
  const base = f.height;

  // Cover: a rounded square whose bottom lines up with the last text line.
  const size = 92 * unit;
  let textX = pad;
  if (cover) {
    const x = pad;
    const y = base - pad - size + 6 * unit;
    const side = Math.min(cover.naturalWidth, cover.naturalHeight);
    const sx = (cover.naturalWidth - side) / 2;
    const sy = (cover.naturalHeight - side) / 2;
    g.save();
    g.shadowColor = "rgba(0,0,0,0.4)";
    g.shadowBlur = 16 * unit;
    g.beginPath();
    g.roundRect(x, y, size, size, 8 * unit);
    g.fillStyle = "#000";
    g.fill();
    g.shadowColor = "transparent";
    g.clip();
    g.drawImage(cover, sx, sy, side, side, x, y, size, size);
    g.restore();
    textX = pad + size + 18 * unit;
  }
  if (!title && !artist) return;

  const maxW = f.width - textX - pad;
  const font = "system-ui, -apple-system, Segoe UI, sans-serif";
  g.textBaseline = "alphabetic";
  // The last line sits on the baseline; the title goes above the artist.
  let top = base - pad;
  if (artist) {
    g.fillStyle = "rgba(255,255,255,0.75)";
    fitText(g, artist, textX, top, maxW, 22 * unit, `500 {size}px ${font}`);
  }
  if (title) {
    if (artist) top -= 30 * unit;
    g.fillStyle = "#ffffff";
    fitText(g, title, textX, top, maxW, 32 * unit, `700 {size}px ${font}`);
  }
  // Accent bar above the first line of text.
  g.fillStyle = "#ffb000";
  g.fillRect(textX, top - (title ? 44 : 34) * unit, 40 * unit, 4 * unit);
}

/** Draws `text` at up to `size` px: smaller (down to 75%) when it does not
 *  fit `maxW`, then cut with an ellipsis. Never squeezes the letters. */
function fitText(g: CanvasRenderingContext2D, text: string, x: number, y: number, maxW: number, size: number, font: string): void {
  let px = size;
  g.font = font.replace("{size}", String(Math.round(px)));
  const width = g.measureText(text).width;
  if (width > maxW) {
    px = Math.max(size * 0.75, (size * maxW) / width);
    g.font = font.replace("{size}", String(Math.round(px)));
  }
  let shown = text;
  while (shown.length > 1 && g.measureText(shown).width > maxW) shown = shown.slice(0, -2) + "…";
  g.fillText(shown, x, y);
}

export const studio = new VideoStudio();
