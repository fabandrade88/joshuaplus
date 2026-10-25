import type { BeatAnalysis, ChordSpan, LyricLine } from "./types";

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

/** Index of the last item starting at or before `secs` (items sorted by start). */
function lastStarted<T>(items: T[], start: (x: T) => number, secs: number): number {
  let lo = 0;
  let hi = items.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (start(items[mid] as T) <= secs) lo = mid + 1;
    else hi = mid;
  }
  return lo - 1;
}

/** Chord sounding at `secs`, or `-1`. */
export function chordAt(chords: ChordSpan[], secs: number): number {
  const i = lastStarted(chords, (c) => c.start, secs);
  return i >= 0 && secs < (chords[i]?.end ?? 0) ? i : -1;
}

/** Line being sung at `secs` (lines moved by `offset`), or `-1` before the first. */
export function lineAt(lines: LyricLine[], secs: number, offset = 0): number {
  return lastStarted(lines, (l) => l.secs + offset, secs);
}

/** Chord names heard in `[from, to)`, consecutive repeats merged. */
export function chordsIn(chords: ChordSpan[], from: number, to: number): string[] {
  const out: string[] = [];
  for (const c of chords) {
    if (c.end <= from || c.start >= to) continue;
    // A chord that only grazes the bar (under a beat's worth) is noise here.
    if (Math.min(c.end, to) - Math.max(c.start, from) < (to - from) / 8) continue;
    if (out[out.length - 1] !== c.name) out.push(c.name);
  }
  return out;
}

/** `[mm:ss.xx] text` lines, for editing synced lyrics. */
export function toLrc(lines: LyricLine[]): string {
  return lines
    .map((l) => {
      const m = Math.floor(l.secs / 60);
      const s = (l.secs - m * 60).toFixed(2).padStart(5, "0");
      return `[${String(m).padStart(2, "0")}:${s}] ${l.text}`;
    })
    .join("\n");
}
