import { describe, expect, it } from "vitest";
import { moveId, orderSongs } from "./playlist";
import type { SongSummary } from "./types";

const song = (id: string, createdUnix: number): SongSummary => ({
  id,
  title: id,
  bpm: 120,
  durationSecs: 60,
  separator: "neural:htdemucs_6s",
  stems: 6,
  createdUnix,
  folder: "f",
});

describe("orderSongs", () => {
  const pool = [song("a", 1), song("b", 2), song("c", 3), song("d", 4)];

  it("defaults to newest first", () => {
    expect(orderSongs(pool, []).map((s) => s.id)).toEqual(["d", "c", "b", "a"]);
  });

  it("follows the saved order, then appends new songs newest first", () => {
    expect(orderSongs(pool, ["b", "a"]).map((s) => s.id)).toEqual(["b", "a", "d", "c"]);
  });

  it("ignores ids that are not in the folder", () => {
    expect(orderSongs(pool, ["ghost", "c"]).map((s) => s.id)).toEqual(["c", "d", "b", "a"]);
  });
});

describe("moveId", () => {
  it("moves within bounds", () => {
    expect(moveId(["a", "b", "c"], "c", 0)).toEqual(["c", "a", "b"]);
    expect(moveId(["a", "b", "c"], "a", 99)).toEqual(["b", "c", "a"]);
    expect(moveId(["a", "b"], "zz", 0)).toEqual(["a", "b"]);
  });
});
