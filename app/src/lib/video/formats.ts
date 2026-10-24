// Export formats of the video studio and the pure maths behind them
// (cropping, codec choice, audio/video sync), kept free of DOM APIs so they
// can be unit-tested.

export type FormatId = "youtube" | "vertical" | "square" | "portrait";

export interface VideoFormat {
  id: FormatId;
  width: number;
  height: number;
}

/** YouTube 16:9, Shorts/Reels/TikTok 9:16, Instagram feed 1:1 and 4:5. */
export const FORMATS: readonly VideoFormat[] = [
  { id: "youtube", width: 1920, height: 1080 },
  { id: "vertical", width: 1080, height: 1920 },
  { id: "square", width: 1080, height: 1080 },
  { id: "portrait", width: 1080, height: 1350 },
];

export interface Crop {
  sx: number;
  sy: number;
  sw: number;
  sh: number;
}

/** Centre crop of a `srcW×srcH` frame that fills `dstW×dstH` without bars. */
export function coverCrop(srcW: number, srcH: number, dstW: number, dstH: number): Crop {
  if (srcW <= 0 || srcH <= 0 || dstW <= 0 || dstH <= 0) return { sx: 0, sy: 0, sw: 0, sh: 0 };
  const src = srcW / srcH;
  const dst = dstW / dstH;
  if (src > dst) {
    const sw = srcH * dst;
    return { sx: (srcW - sw) / 2, sy: 0, sw, sh: srcH };
  }
  const sh = srcW / dst;
  return { sx: 0, sy: (srcH - sh) / 2, sw: srcW, sh };
}

export interface Container {
  mime: string;
  ext: "mp4" | "webm";
}

/** Containers in order of preference: MP4 (H.264 + AAC) is what YouTube,
 *  Instagram and TikTok all accept; WebM is the fallback. */
const CANDIDATES: readonly Container[] = [
  { mime: "video/mp4;codecs=avc1.640028,mp4a.40.2", ext: "mp4" },
  { mime: "video/mp4;codecs=avc1,mp4a.40.2", ext: "mp4" },
  { mime: "video/mp4", ext: "mp4" },
  { mime: "video/webm;codecs=vp9,opus", ext: "webm" },
  { mime: "video/webm;codecs=vp8,opus", ext: "webm" },
  { mime: "video/webm", ext: "webm" },
];

export function pickContainer(isSupported: (mime: string) => boolean): Container | null {
  return CANDIDATES.find((c) => isSupported(c.mime)) ?? null;
}

/** Video-only variant of a container (for the raw camera recording). */
export function videoOnly(c: Container): string {
  return c.mime.replace(/,mp4a\.40\.2|,opus/, "");
}

export interface SyncSample {
  /** Seconds since the camera recording started. */
  video: number;
  /** Song position reported at that moment. */
  song: number;
}

/** Song time at video time 0: the median of `song − video` over the samples
 *  (robust to the odd late transport tick). `null` without samples. */
export function estimateSongAtVideoStart(samples: readonly SyncSample[]): number | null {
  if (samples.length === 0) return null;
  const d = samples.map((s) => s.song - s.video).sort((a, b) => a - b);
  const mid = Math.floor(d.length / 2);
  return d.length % 2 ? d[mid]! : (d[mid - 1]! + d[mid]!) / 2;
}

/** Default extra delay of the picture vs. the sound: webcam capture latency
 *  plus the audio output latency (the musician reacts to what they hear). */
export const DEFAULT_SYNC_MS = -100;

/** Bitrate giving good quality for social platforms at each size. */
export function videoBitrate(f: VideoFormat): number {
  const pixels = f.width * f.height;
  return Math.round(Math.min(16_000_000, Math.max(4_000_000, pixels * 4.5)));
}
