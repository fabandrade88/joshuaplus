// Guards against regressions that would open XSS / code-injection holes in
// the webview, and pins the Tauri security configuration (CSP, permissions).
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const SRC = join(__dirname);
const TAURI = join(__dirname, "..", "src-tauri");

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const p = join(dir, name);
    return statSync(p).isDirectory() ? files(p) : /\.(svelte|ts)$/.test(name) && !name.endsWith(".test.ts") ? [p] : [];
  });
}

describe("frontend source", () => {
  const sources = files(SRC).map((p) => ({ p, text: readFileSync(p, "utf8") }));

  it.each([
    ["{@html", /\{@html/],
    ["innerHTML", /\.innerHTML\s*=/],
    ["outerHTML", /\.outerHTML\s*=/],
    ["insertAdjacentHTML", /insertAdjacentHTML/],
    ["document.write", /document\.write/],
    ["eval()", /\beval\s*\(/],
    ["new Function()", /new\s+Function\s*\(/],
    ["string setTimeout", /setTimeout\(\s*["'`]/],
    ["javascript: URLs", /javascript:/i],
  ])("never uses %s", (_name, pattern) => {
    const offenders = sources.filter((f) => pattern.test(f.text)).map((f) => f.p);
    expect(offenders).toEqual([]);
  });

  it("opens external links only through the allow-listed opener", () => {
    const offenders = sources.filter((f) => /window\.open\(|location\.href\s*=/.test(f.text)).map((f) => f.p);
    expect(offenders).toEqual([]);
  });
});

describe("tauri security configuration", () => {
  const conf = JSON.parse(readFileSync(join(TAURI, "tauri.conf.json"), "utf8"));
  const csp = conf.app.security.csp as Record<string, string>;

  it("has a strict content security policy", () => {
    expect(csp["default-src"]).toBe("'self'");
    expect(csp["script-src"]).toBe("'self'");
    expect(csp["object-src"]).toBe("'none'");
    expect(csp["base-uri"]).toBe("'none'");
    expect(csp["frame-ancestors"]).toBe("'none'");
    expect(csp["form-action"]).toBe("'none'");
    for (const [k, v] of Object.entries(csp)) {
      expect(v, k).not.toMatch(/unsafe-eval|\*/);
      // No remote origins anywhere; connect-src is the Tauri IPC channel (checked exactly below).
      if (k !== "connect-src") expect(v, k).not.toMatch(/https?:/);
    }
    expect(csp["connect-src"]).toBe("ipc: http://ipc.localhost");
    // Video studio: local camera stream and in-memory recordings only.
    expect(csp["media-src"]).toBe("'self' blob: mediastream:");
  });

  it("freezes prototypes and keeps the Tauri API off window", () => {
    expect(conf.app.security.freezePrototype).toBe(true);
    expect(conf.app.withGlobalTauri).toBe(false);
  });

  it("grants only the minimal permissions", () => {
    const caps = JSON.parse(readFileSync(join(TAURI, "capabilities", "default.json"), "utf8"));
    const ids = caps.permissions.map((p: string | { identifier: string }) => (typeof p === "string" ? p : p.identifier));
    expect(ids.sort()).toEqual([
      "core:default",
      "dialog:allow-ask",
      "dialog:allow-confirm",
      "dialog:allow-message",
      "dialog:allow-open",
      "opener:allow-open-url",
    ]);
    const opener = caps.permissions.find((p: { identifier?: string }) => p.identifier === "opener:allow-open-url");
    expect(opener.allow).toEqual([{ url: "https://www.lalal.ai/*" }, { url: "https://github.com/fabandrade88/joshuaplus*" }]);
    expect(JSON.stringify(caps)).not.toMatch(/fs:|shell:|http:|process:/);
  });
});
