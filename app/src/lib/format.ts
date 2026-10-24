/** `m:ss.t` (or `h:mm:ss`) for a position in seconds. */
export function formatTime(secs: number, tenths = false): string {
  if (!Number.isFinite(secs) || secs < 0) secs = 0;
  const total = tenths ? Math.floor(secs * 10) / 10 : Math.floor(secs);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const ss = tenths ? s.toFixed(1).padStart(4, "0") : String(Math.floor(s)).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}

export const MIN_DB = -60;
export const MAX_DB = 6;

/**
 * Fader law: slider position `0..1` ↔ dB. The upper 75 % of travel covers
 * −30…+6 dB (where musicians actually mix); the rest fades to silence.
 */
export function faderToDb(pos: number): number {
  const p = Math.min(1, Math.max(0, pos));
  if (p <= 0) return -96;
  if (p >= 0.25) return -30 + ((p - 0.25) / 0.75) * (MAX_DB + 30);
  return MIN_DB + (p / 0.25) * (-30 - MIN_DB);
}

export function dbToFader(db: number): number {
  if (!Number.isFinite(db) || db <= MIN_DB) return 0;
  if (db >= MAX_DB) return 1;
  if (db >= -30) return 0.25 + ((db + 30) / (MAX_DB + 30)) * 0.75;
  return ((db - MIN_DB) / (-30 - MIN_DB)) * 0.25;
}

export function formatDb(db: number): string {
  if (!Number.isFinite(db) || db <= MIN_DB) return "−∞";
  const r = Math.round(db * 10) / 10;
  return `${r > 0 ? "+" : r < 0 ? "−" : ""}${Math.abs(r).toFixed(1)}`;
}

/** Linear peak (0..1) → meter fill (0..1) on a −48…0 dB scale. */
export function peakToMeter(peak: number): number {
  if (!(peak > 0)) return 0;
  const db = 20 * Math.log10(peak);
  return Math.min(1, Math.max(0, (db + 48) / 48));
}

export function formatBpm(bpm: number): string {
  return Number.isFinite(bpm) ? (Math.round(bpm * 10) / 10).toString() : "–";
}
