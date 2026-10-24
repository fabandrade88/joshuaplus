import { describe, expect, it } from "vitest";
import { FORMATS, coverCrop, estimateSongAtVideoStart, pickContainer, videoBitrate, videoOnly } from "./formats";

describe("coverCrop", () => {
  it("crops the sides of a 16:9 camera for vertical video", () => {
    const c = coverCrop(1920, 1080, 1080, 1920);
    expect(c.sh).toBe(1080);
    expect(c.sw).toBeCloseTo(607.5);
    expect(c.sx).toBeCloseTo((1920 - 607.5) / 2);
    expect(c.sy).toBe(0);
  });

  it("keeps the whole frame when aspect ratios match and crops top/bottom otherwise", () => {
    expect(coverCrop(1920, 1080, 1920, 1080)).toEqual({ sx: 0, sy: 0, sw: 1920, sh: 1080 });
    const tall = coverCrop(1080, 1920, 1920, 1080);
    expect(tall.sw).toBe(1080);
    expect(tall.sh).toBeCloseTo(607.5);
    expect(tall.sy).toBeCloseTo((1920 - 607.5) / 2);
  });

  it("never produces NaN for degenerate sizes", () => {
    expect(coverCrop(0, 0, 1080, 1080)).toEqual({ sx: 0, sy: 0, sw: 0, sh: 0 });
  });
});

describe("pickContainer", () => {
  it("prefers MP4 and falls back to WebM", () => {
    expect(pickContainer(() => true)?.ext).toBe("mp4");
    expect(pickContainer((m) => m.startsWith("video/webm"))?.mime).toBe("video/webm;codecs=vp9,opus");
    expect(pickContainer(() => false)).toBeNull();
  });

  it("strips the audio codec for the camera-only recording", () => {
    expect(videoOnly({ mime: "video/mp4;codecs=avc1,mp4a.40.2", ext: "mp4" })).toBe("video/mp4;codecs=avc1");
    expect(videoOnly({ mime: "video/webm;codecs=vp9,opus", ext: "webm" })).toBe("video/webm;codecs=vp9");
  });
});

describe("estimateSongAtVideoStart", () => {
  it("uses the median, ignoring a late tick", () => {
    const s = [0.5, 1, 1.5, 2, 2.5].map((v) => ({ video: v, song: 10 + v }));
    s.push({ video: 3, song: 10 + 3 - 0.4 }); // delivered late
    expect(estimateSongAtVideoStart(s)).toBeCloseTo(10, 5);
    expect(estimateSongAtVideoStart([])).toBeNull();
  });
});

describe("formats", () => {
  it("cover the main platforms with sane bitrates", () => {
    expect(FORMATS.map((f) => `${f.width}x${f.height}`)).toEqual(["1920x1080", "1080x1920", "1080x1080", "1080x1350"]);
    for (const f of FORMATS) {
      const b = videoBitrate(f);
      expect(b).toBeGreaterThanOrEqual(4_000_000);
      expect(b).toBeLessThanOrEqual(16_000_000);
    }
  });
});
