import type { BeatAnalysis } from "./types";

export interface Bar {
  index: number; // 1-based bar number
  time: number; // seconds
}

/** Bars (downbeats) derived from the beat analysis. */
export function barsOf(a: BeatAnalysis): Bar[] {
  const bpb = Math.max(1, a.beatsPerBar);
  const bars: Bar[] = [];
  a.beats.forEach((t, i) => {
    if (i % bpb === a.downbeatPhase % bpb) bars.push({ index: bars.length + 1, time: t });
  });
  return bars;
}

/** Bar/beat position (1-based) for a time, e.g. `{ bar: 12, beat: 3 }`. */
export function barBeatAt(a: BeatAnalysis, secs: number): { bar: number; beat: number } | null {
  if (a.beats.length === 0 || secs < (a.beats[0] ?? 0)) return null;
  let lo = 0;
  let hi = a.beats.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if ((a.beats[mid] ?? Infinity) <= secs) lo = mid;
    else hi = mid - 1;
  }
  const bpb = Math.max(1, a.beatsPerBar);
  const rel = lo - (a.downbeatPhase % bpb);
  const bar = Math.floor(rel / bpb) + 1;
  const beat = (((rel % bpb) + bpb) % bpb) + 1;
  return { bar: Math.max(bar, 0), beat };
}

/** Picks a ruler label step so labels are at least `minPx` apart. */
export function rulerStep(barCount: number, widthPx: number, minPx = 48): number {
  if (barCount <= 0 || widthPx <= 0) return 1;
  const pxPerBar = widthPx / barCount;
  for (const step of [1, 2, 4, 8, 16, 32, 64]) if (pxPerBar * step >= minPx) return step;
  return 128;
}

export function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}
