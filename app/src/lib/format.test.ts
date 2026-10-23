import { describe, expect, it } from "vitest";
import { dbToFader, faderToDb, formatBpm, formatDb, formatTime, peakToMeter } from "./format";

describe("formatTime", () => {
  it("formats minutes, hours and tenths", () => {
    expect(formatTime(0)).toBe("0:00");
    expect(formatTime(65.4)).toBe("1:05");
    expect(formatTime(65.47, true)).toBe("1:05.4");
    expect(formatTime(3725)).toBe("1:02:05");
  });
  it("never shows garbage for hostile values", () => {
    for (const v of [NaN, -5, Infinity, -Infinity]) expect(formatTime(v)).toBe("0:00");
  });
});

describe("fader law", () => {
  it("round-trips across the range", () => {
    for (const db of [-59, -45, -30, -12, -6, 0, 3, 6]) {
      expect(faderToDb(dbToFader(db))).toBeCloseTo(db, 6);
    }
  });
  it("is monotonic and clamped", () => {
    let prev = -Infinity;
    for (let p = 0; p <= 1.0001; p += 0.01) {
      const db = faderToDb(p);
      expect(db).toBeGreaterThanOrEqual(prev);
      prev = db;
    }
    expect(faderToDb(-1)).toBe(-96);
    expect(faderToDb(2)).toBe(6);
    expect(dbToFader(NaN)).toBe(0);
    expect(dbToFader(100)).toBe(1);
  });
  it("puts 0 dB at a comfortable position", () => {
    expect(dbToFader(0)).toBeGreaterThan(0.75);
    expect(dbToFader(0)).toBeLessThan(0.9);
  });
});

describe("display helpers", () => {
  it("formats dB values", () => {
    expect(formatDb(0)).toBe("0.0");
    expect(formatDb(3.04)).toBe("+3.0");
    expect(formatDb(-6.02)).toBe("−6.0");
    expect(formatDb(-100)).toBe("−∞");
  });
  it("maps peaks to meter positions", () => {
    expect(peakToMeter(0)).toBe(0);
    expect(peakToMeter(1)).toBe(1);
    expect(peakToMeter(NaN)).toBe(0);
    expect(peakToMeter(0.5)).toBeCloseTo(1 - 6.02 / 48, 2);
  });
  it("formats BPM", () => {
    expect(formatBpm(120)).toBe("120");
    expect(formatBpm(127.96)).toBe("128");
    expect(formatBpm(NaN)).toBe("–");
  });
});
