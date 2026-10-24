import { describe, expect, it } from "vitest";
import { en } from "./en";
import { catalogs, format, matchLocale } from "./index.svelte";

const placeholders = (s: string) => [...new Set([...s.matchAll(/\{(\w+)[:}]/g)].map((m) => m[1]))].sort();

describe("catalogs", () => {
  for (const [locale, messages] of Object.entries(catalogs)) {
    it(`${locale} is complete and consistent`, () => {
      expect(Object.keys(messages).sort()).toEqual(Object.keys(en).sort());
      for (const [key, value] of Object.entries(messages)) {
        expect(value.trim(), key).not.toBe("");
        expect(placeholders(value), key).toEqual(placeholders(en[key as keyof typeof en]));
        // Messages are rendered as text, but keep markup out of catalogs anyway.
        expect(value, key).not.toMatch(/<[a-z/]/i);
      }
    });
  }
});

describe("helpers", () => {
  it("matches OS locales", () => {
    expect(matchLocale("pt-BR")).toBe("pt-BR");
    expect(matchLocale("pt-PT")).toBe("pt-BR");
    expect(matchLocale("de-AT")).toBe("de");
    expect(matchLocale("fr-FR")).toBe("en");
    expect(matchLocale(undefined)).toBe("en");
  });
  it("interpolates without evaluating input", () => {
    expect(format("{n} stems", { n: 4 })).toBe("4 stems");
    expect(format("{missing}", {})).toBe("{missing}");
    expect(format("{t}", { t: "<img src=x onerror=alert(1)>" })).toBe("<img src=x onerror=alert(1)>");
    expect(format("{t}", { t: "$& $1 $$" })).toBe("$& $1 $$");
  });
});
