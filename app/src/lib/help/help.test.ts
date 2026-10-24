import { describe, expect, it } from "vitest";
import { filterHelp, helpCatalogs } from "./index";

describe("help catalogs", () => {
  const en = helpCatalogs.en;
  for (const [locale, sections] of Object.entries(helpCatalogs)) {
    it(`${locale} mirrors the English structure`, () => {
      expect(sections.map((s) => s.id)).toEqual(en.map((s) => s.id));
      sections.forEach((s, i) => {
        expect(s.items.length, `${locale}/${s.id}`).toBe(en[i]?.items.length);
        for (const item of s.items) {
          expect(item.term.trim()).not.toBe("");
          expect(item.text.trim()).not.toBe("");
          expect(item.text).not.toMatch(/<[a-z/]/i);
        }
      });
    });
  }

  it("searches ignoring case and accents", () => {
    const pt = helpCatalogs["pt-BR"];
    const r = filterHelp(pt, "METRONOMO");
    expect(r.map((s) => s.id)).toContain("metronome");
    expect(filterHelp(pt, "contagem").flatMap((s) => s.items).length).toBeGreaterThan(0);
    expect(filterHelp(pt, "xyzzy")).toEqual([]);
    expect(filterHelp(pt, "  ")).toBe(pt);
  });
});
