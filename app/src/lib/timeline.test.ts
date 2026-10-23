import { describe, expect, it } from "vitest";
import { barBeatAt, barsOf, rulerStep } from "./timeline";
import type { BeatAnalysis } from "./types";

const analysis = (beats: number[], downbeatPhase = 0): BeatAnalysis => ({
  bpm: 120,
  beats,
  beatsPerBar: 4,
  downbeatPhase,
  confidence: 1,
});

describe("bars", () => {
  it("lists downbeats as numbered bars", () => {
    const a = analysis([0, 0.5, 1, 1.5, 2, 2.5, 3, 3.5, 4]);
    expect(barsOf(a)).toEqual([
      { index: 1, time: 0 },
      { index: 2, time: 2 },
      { index: 3, time: 4 },
    ]);
  });
  it("honours the downbeat phase", () => {
    const a = analysis([0, 0.5, 1, 1.5, 2, 2.5], 1);
    expect(barsOf(a).map((b) => b.time)).toEqual([0.5, 2.5]);
  });
});

describe("barBeatAt", () => {
  const a = analysis([1, 1.5, 2, 2.5, 3, 3.5, 4, 4.5]);
  it("returns bar and beat", () => {
    expect(barBeatAt(a, 1)).toEqual({ bar: 1, beat: 1 });
    expect(barBeatAt(a, 2.7)).toEqual({ bar: 1, beat: 4 });
    expect(barBeatAt(a, 3.1)).toEqual({ bar: 2, beat: 1 });
  });
  it("is null before the first beat or without beats", () => {
    expect(barBeatAt(a, 0.5)).toBeNull();
    expect(barBeatAt(analysis([]), 3)).toBeNull();
  });
  it("labels pickup beats before the first downbeat as bar 0", () => {
    const pickup = analysis([0, 0.5, 1, 1.5, 2], 2);
    expect(barBeatAt(pickup, 0)).toEqual({ bar: 0, beat: 3 });
    expect(barBeatAt(pickup, 1)).toEqual({ bar: 1, beat: 1 });
  });
});

describe("rulerStep", () => {
  it("keeps labels readable", () => {
    expect(rulerStep(10, 1000)).toBe(1);
    expect(rulerStep(100, 1000)).toBe(8);
    expect(rulerStep(0, 1000)).toBe(1);
  });
});
