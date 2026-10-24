import { describe, expect, it } from "vitest";
import { FOLDER_GLYPHS, glyphFor, glyphsFor } from "./folderGlyph";

describe("glyphFor", () => {
  it("is stable per folder and spreads across the set", () => {
    expect(glyphFor("banda")).toBe(glyphFor("banda"));
    const seen = new Set(Array.from({ length: 200 }, (_, i) => glyphFor(`folder-${i}`)));
    expect(seen.size).toBeGreaterThan(Object.keys(FOLDER_GLYPHS).length / 2);
    for (const g of seen) expect(FOLDER_GLYPHS[g]).toBeTruthy();
  });

  it("gives existing folders distinct icons and keeps them when one is added", () => {
    const folders = Array.from({ length: 12 }, (_, i) => ({ id: `f${i}`, createdUnix: i }));
    const map = glyphsFor(folders);
    expect(new Set(map.values()).size).toBe(12);
    const more = glyphsFor([...folders, { id: "new", createdUnix: 99 }]);
    for (const f of folders) expect(more.get(f.id)).toBe(map.get(f.id));
  });
});
