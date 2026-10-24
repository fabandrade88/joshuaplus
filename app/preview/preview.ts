// UI preview harness: runs the real frontend in a normal browser with the
// Tauri IPC mocked, so the interface can be designed and screenshotted
// without the Rust backend.  `pnpm preview:ui` → http://127.0.0.1:1421/
//
// URL parameters: ?state=welcome|home|folder|song|away|settings  &lang=en|pt-BR|de
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { Folder, SongProject, SongSummary, StemKind } from "../src/lib/types";

const params = new URLSearchParams(location.search);
const state = params.get("state") ?? "home";
const lang = params.get("lang") ?? "en";

const DURATION = 214;
const BPM = 96;
const beats = Array.from({ length: Math.floor(((DURATION - 2) * BPM) / 60) }, (_, i) => 1.2 + (i * 60) / BPM);
const stems: StemKind[] = ["vocals", "drums", "bass", "guitar", "acoustic", "piano", "other"];

function peaks(seed: number): number[] {
  return Array.from({ length: 2000 }, (_, i) => {
    const t = i / 2000;
    const section = t < 0.08 || t > 0.94 ? 0.25 : t > 0.45 && t < 0.55 ? 0.55 : 1;
    const beat = seed === 1 ? Math.abs(Math.sin(i * 0.9)) ** 6 : 0.6 + 0.4 * Math.sin(i * 0.05 * (seed + 1));
    const noise = Math.abs(Math.sin(i * 12.9898 * (seed + 1)) * 43758.5453) % 1;
    const vocalGaps = seed === 0 && Math.sin(i * 0.021) < -0.4 ? 0.08 : 1;
    return Math.round(Math.min(1, section * vocalGaps * (0.35 + 0.65 * beat) * (0.75 + 0.25 * noise)) * 230);
  });
}

const project: SongProject = {
  schemaVersion: 1,
  id: "sunday-groove-1a2b3c4d",
  title: "Sunday Groove (Live Session)",
  sourceFile: "source.mp3",
  sampleRate: 44100,
  durationSecs: DURATION,
  separator: "neural:htdemucs_6s",
  createdUnix: 9,
  folder: "banda",
  analysis: { bpm: BPM, beats, beatsPerBar: 4, downbeatPhase: 0, confidence: 0.8 },
  tracks: stems.map((stem, i) => ({
    stem,
    file: `stems/${stem}.wav`,
    gainDb: [0, -2.5, 0, -6, -3, -12, -4][i] ?? 0,
    pan: 0,
    muted: stem === "guitar",
    solo: false,
  })),
  metronome: { gainDb: -6, muted: false, countIn: true, countInBars: 1, sound: "woodblock" },
  takes: [
    { id: "take-1", name: "SPD-SX PRO", file: "takes/take-1.wav", offsetSecs: 12.5, createdUnix: 0, gainDb: -2, pan: 0, muted: false, solo: false, room: 0.3 },
    { id: "take-2", name: "Yamaha AG06MK2", file: "takes/take-2.wav", offsetSecs: 12.5, createdUnix: 0, gainDb: 0, pan: 0, muted: false, solo: false, room: 0.55 },
  ],
  masterGainDb: 0,
  metadata: { artist: "The Session Band", album: "Live at Home EP", year: "2026", genre: "Funk", track: "3/5" },
  hasCover: true,
  songGainDb: 0,
  songMuted: false,
};

function art(a: string, b: string): string {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${a}"/><stop offset="1" stop-color="${b}"/></linearGradient></defs><rect width="100" height="100" fill="url(#g)"/><circle cx="50" cy="50" r="26" fill="none" stroke="#fff" stroke-opacity=".5" stroke-width="3"/></svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

const folders: Folder[] = [
  { id: "banda", name: "Banda do Domingo", color: "amber", createdUnix: 1, order: [] },
  { id: "igreja", name: "Louvor Igreja", color: "violet", createdUnix: 2, order: ["ousado-amor-2", "oceanos-1"] },
  { id: "estudo", name: "Estudo de Guitarra", color: "green", createdUnix: 3, order: [] },
];

const song = (id: string, title: string, bpm: number, secs: number, sep: string, n: number, t: number, folder: string | null): SongSummary => ({
  id, title, bpm, durationSecs: secs, separator: sep, stems: n, createdUnix: t, folder,
  artist: ["PRESIDENT", "The Beatles", "Tom Jobim", null][t % 4] ?? null,
  cover: t % 3 === 0 ? null : art(["#ff6b8a", "#8b6cff", "#ffb020", "#3ddc97"][t % 4] ?? "#888", "#11141b"),
});

const songs: SongSummary[] = [
  song(project.id, project.title, BPM, DURATION, project.separator, 7, 9, "banda"),
  song("blackbird-9f8e7d6c", "Blackbird", 93.4, 138, "neural:htdemucs_6s", 6, 8, "estudo"),
  song("garota-de-ipanema-55aa11bb", "Garota de Ipanema", 129, 312, "cloud:lalal", 7, 7, "banda"),
  song("oceanos-1", "Oceanos", 72, 534, "neural:htdemucs_6s", 6, 6, "igreja"),
  song("ousado-amor-2", "Ousado Amor", 68, 301, "neural:htdemucs_6s", 6, 5, "igreja"),
  song("sweet-child-3", "Sweet Child O' Mine", 125, 356, "spectral", 4, 4, "estudo"),
  song("demo-4", "Ideia nova (demo)", 110, 95, "neural:htdemucs_6s", 6, 3, null),
];

mockWindows("main");
const savedBytes: Record<number, number> = {};
let nextHandle = 1;
function sineWav(secs: number): ArrayBuffer {
  const rate = 48000;
  const n = Math.round(secs * rate);
  const buf = new ArrayBuffer(44 + n * 4);
  const v = new DataView(buf);
  const w = (o: number, t: string) => [...t].forEach((c, i) => v.setUint8(o + i, c.charCodeAt(0)));
  w(0, "RIFF"); v.setUint32(4, 36 + n * 4, true); w(8, "WAVE"); w(12, "fmt ");
  v.setUint32(16, 16, true); v.setUint16(20, 1, true); v.setUint16(22, 2, true); v.setUint32(24, rate, true);
  v.setUint32(28, rate * 4, true); v.setUint16(32, 4, true); v.setUint16(34, 16, true); w(36, "data"); v.setUint32(40, n * 4, true);
  for (let i = 0; i < n; i++) {
    const x = Math.round(Math.sin((2 * Math.PI * 440 * i) / rate) * 8000);
    v.setInt16(44 + i * 4, x, true);
    v.setInt16(46 + i * 4, x, true);
  }
  return buf;
}
(window as unknown as { savedBytes: Record<number, number> }).savedBytes = savedBytes;
/** How many times the clip sound was rendered (remix test). */
const renders = { n: 0 };

mockIPC(
  (cmd, args) => {
    switch (cmd) {
      case "check_for_update":
        return params.get("update") === "1"
          ? { version: "0.6.0", url: "https://github.com/fabandrade88/joshuaplus/releases/tag/v0.6.0" }
          : null;
      case "list_exports": {
        const dir = "/Users/musician/Music/Joshua+/Exports";
        const now = Math.floor(Date.now() / 1000);
        const f = (name: string, kind: string, mb: number, ago: number) => ({
          name,
          path: `${dir}/${name}`,
          kind,
          size: mb * 1024 * 1024,
          modifiedUnix: now - ago,
        });
        return {
          dir,
          files: [
            f("Mercy mix (2).wav", "audio", 63, 600),
            f("Left For Good youtube.mp4", "video", 18, 3600),
            f("Left For Good vertical.mp4", "video", 19, 3600),
            f("Mercy mix.wav", "audio", 63, 86400),
          ],
        };
      }
      case "render_clip_audio":
        renders.n++;
        return sineWav((args as { durationSecs: number }).durationSecs);
      case "video_save_begin":
        savedBytes[nextHandle] = 0;
        return nextHandle++;
      case "video_save_chunk": {
        const bytes = args as unknown as Uint8Array;
        const h = Number(Object.keys(savedBytes).pop());
        savedBytes[h] = (savedBytes[h] ?? 0) + (bytes?.byteLength ?? 0);
        return null;
      }
      case "video_save_end": {
        const { handle, keep } = args as { handle: number; keep: boolean };
        return keep ? `/Exports/clip-${handle}.${"mp4"} (${savedBytes[handle]} bytes)` : null;
      }
      case "get_app_info":
        return {
          version: "0.5.0",
          locale: lang,
          workspace: state === "welcome" ? null : "/Users/musician/Music/Joshua+",
          inbox: "/Users/musician/Music/Joshua+/inbox",
          separator: "studio",
          modelInstalled: state !== "settings",
          modelSizeMb: 136,
          cloudConfigured: false,
          audio: { device: "MacBook Pro Speakers", sampleRate: 48000, channels: 2 },
          audioError: null,
          outputDevice: null,
          exportDir: "/Users/musician/Music/Joshua+/Exports",
          exportDirCustom: false,
          updateCheck: true,
          suggestedWorkspace: "/Users/musician/Music/Joshua+",
          musicDir: "/Users/musician/Music",
        };
      case "list_songs":
        return songs;
      case "list_folders":
        return folders;
      case "load_song":
        return { project, peaks: stems.map((_, i) => peaks(i)), audioReady: true, cover: art("#ff6b8a", "#4c1d95") };
      case "list_audio_inputs":
        return [
          { id: "coreaudio:ag06", name: "Yamaha AG06MK2", channels: 2, sampleRate: 48000, isDefault: false },
          { id: "coreaudio:spd", name: "SPD-SX PRO", channels: 8, sampleRate: 48000, isDefault: false },
        ];
      case "get_inputs":
        return [
          { index: 0, deviceId: "coreaudio:ag06", name: "Yamaha AG06MK2", channels: 2, sampleRate: 48000, firstChannel: 0, stereo: false, monitor: true, gainDb: -3, monitorOk: false, directMonitor: true, armed: false, canRecord: true, error: null },
          { index: 1, deviceId: "coreaudio:spd", name: "SPD-SX PRO", channels: 8, sampleRate: 48000, firstChannel: 0, stereo: true, monitor: true, gainDb: 0, monitorOk: true, directMonitor: false, armed: true, canRecord: true, error: null },
        ];
      case "get_drums":
        return {
          ports: [{ id: "coremidi:strata", name: "Alesis Prime Drum Module" }, { id: "coremidi:spd", name: "SPD-SX PRO" }],
          portId: "coremidi:strata",
          connected: "Alesis Prime Drum Module",
          error: null,
          brand: "alesis",
          brands: [
            { id: "alesis", name: "Alesis", models: "Strata Prime, Strata Club, Strata Core, Strike Pro SE, Crimson II, Command, Surge, Turbo, Nitro / Nitro Max, Debut", preset: "generalMidi", usbAudio: true },
            { id: "roland", name: "Roland V-Drums", models: "TD-02, TD-07, TD-17, TD-27, TD-50X", preset: "rolandVDrums", usbAudio: true },
          ],
          map: { notes: { "36": "kick", "38": "snare", "40": "snareRim", "37": "snareCrossStick", "42": "hiHatClosed", "46": "hiHatOpen", "44": "hiHatPedal", "48": "tom1", "45": "tom2", "43": "tom3", "41": "tom4", "49": "crash1", "57": "crash2", "51": "ride", "53": "rideBell", "55": "splash", "52": "china" } },
          learning: null,
        };
      case "list_output_devices":
        return [{ id: "coreaudio:speakers", name: "MacBook Pro Speakers", isDefault: true }];
      default:
        return null;
    }
  },
  { shouldMockEvents: true },
);

await import("../src/main");
const { app } = await import("../src/lib/app.svelte");
await new Promise((r) => setTimeout(r, 150));

if (state !== "welcome") {
  await emit("job-update", { id: 1, fileName: "Wonderwall (Remastered).mp3", status: "running", stage: "separating", progress: 0.62, error: null, songId: null, folder: "banda" });
  await emit("job-update", { id: 2, fileName: "Asa Branca.flac", status: "queued", stage: null, progress: 0, error: null, songId: null, folder: null });
}
if (state === "exports") app.openExports();
if (state === "exportdialog") {
  await app.openSong(project.id);
  app.exportDialogOpen = true;
}
if (state === "folder") app.openFolder("igreja");
if (state === "settings") {
  app.settingsOpen = true;
  if (params.get("pending") === "1") app.pendingWorkspace = "/Volumes/SSD Externo/Joshua+";
}
if (state === "song" || state === "away" || state === "video") {
  app.browsing = "banda";
  await app.openSong(project.id);
  app.songCollapsed = params.get("collapsed") === "1";
  const bar = (n: number) => 1.2 + (n - 1) * 4 * (60 / BPM);
  app.loop = { start: bar(25), end: bar(29) };
  app.loopEnabled = true;
  setInterval(
    () =>
      emit("transport", {
        positionSecs: 71.6,
        playing: true,
        countingIn: false,
        meters: [0.62, 0.81, 0.55, 0, 0.4, 0.18, 0.33, 0.7],
        master: [0.72, 0.69],
        songMeter: 0.8,
        ended: false,
        deviceError: false,
      }),
    100,
  );
  setInterval(() => {
    void emit("input-levels", [0.5 + Math.random() * 0.3, 0.15 + Math.random() * 0.2]);
    void emit("drum-hit", { type: "strike", pad: "snare", note: 38, velocity: 100 + Math.round(Math.random() * 27) });
  }, 150);
}

if (state === "away") app.goHome();
if (state === "video") {
  app.videoOpen = true;
  if (params.get("phase") === "overlay") {
    const { drawTitle, loadImage } = await import("../src/lib/video/studio.svelte");
    const { FORMATS } = await import("../src/lib/video/formats");
    const cover = await loadImage(art("#ff6b8a", "#4c1d95").replace("viewBox", 'width="600" height="600" viewBox'));
    app.videoOpen = false;
    const wrap = document.createElement("div");
    wrap.style.cssText = "position:fixed;inset:0;z-index:99;background:#222;display:flex;gap:16px;padding:16px;align-items:flex-start";
    for (const f of FORMATS) {
      const c = document.createElement("canvas");
      c.width = f.width;
      c.height = f.height;
      const g = c.getContext("2d")!;
      const grad = g.createLinearGradient(0, 0, f.width, f.height);
      grad.addColorStop(0, "#3a4a5a");
      grad.addColorStop(1, "#1a1d24");
      g.fillStyle = grad;
      g.fillRect(0, 0, f.width, f.height);
      // ?combo=all|cover|text|artist|title|none picks the overlay parts.
      const combo = params.get("combo") ?? "all";
      const on = (p: string) => combo === "all" || combo === p || (combo === "text" && (p === "title" || p === "artist"));
      drawTitle(g, f, on("title") ? "Left For Good" : "", on("artist") ? "Bad Omens" : "", on("cover") ? cover : null);
      c.style.height = "460px";
      wrap.append(c);
    }
    document.body.append(wrap);
  }
  if (params.get("phase") === "e2e") {
    const { studio } = await import("../src/lib/video/studio.svelte");
    const log: string[] = [];
    const show = () => (document.title = "E2E " + log.join(" | "));
    const ow = console.warn;
    console.warn = (...a: unknown[]) => {
      log.push(a.map(String).join(" "));
      show();
      ow(...a);
    };
    window.addEventListener("unhandledrejection", (e) => (log.push("UNHANDLED " + String(e.reason)), show()));
    log.push("start");
    show();
    // A moving test picture instead of the camera (headless has no camera).
    const cv = document.createElement("canvas");
    cv.width = 1280;
    cv.height = 720;
    const g = cv.getContext("2d")!;
    let f = 0;
    const anim = setInterval(() => {
      g.fillStyle = `hsl(${(f * 7) % 360} 70% 40%)`;
      g.fillRect(0, 0, 1280, 720);
      g.fillStyle = "#fff";
      g.fillRect((f * 20) % 1280, 300, 120, 120);
      f++;
    }, 33);
    const stream = cv.captureStream(30);
    log.push(`container=${studio.container?.mime}`);
    show();
    const rec = new MediaRecorder(stream, { mimeType: studio.container!.mime.replace(/,mp4a\.40\.2|,opus/, "") });
    const chunks: Blob[] = [];
    rec.ondataavailable = (e) => chunks.push(e.data);
    rec.start(500);
    await new Promise((r) => setTimeout(r, 3000));
    await new Promise<void>((r) => ((rec.onstop = () => r()), rec.stop()));
    clearInterval(anim);
    const blob = new Blob(chunks, { type: rec.mimeType });
    studio.clip = { blob, durationSecs: 3, songAtStart: 10, song: { id: project.id, title: project.title, artist: project.metadata?.artist ?? "", cover: null } };
    studio.clipUrl = URL.createObjectURL(blob);
    studio.phase = "review";
    log.push(`clip ${blob.size} bytes ${rec.mimeType}`);
    void studio.export();
    app.videoOpen = false;
    for (let i = 0; i < 60 && (studio.phase === "exporting" || studio.saved.length === 0); i++) {
      await new Promise((r) => setTimeout(r, 500));
      if (studio.phase !== "exporting") break;
    }
    log.push(`phase=${studio.phase} saved=${JSON.stringify(studio.saved)} err=${studio.exportError}`);
    document.title = log.join(" | ");
  }
  if (params.get("phase") === "remix") {
    // Editing the mix while the review plays re-renders the clip sound, and
    // the export then reuses that fresh sound.
    const { studio } = await import("../src/lib/video/studio.svelte");
    const log: string[] = [];
    const cv = document.createElement("canvas");
    cv.width = 640;
    cv.height = 360;
    const g = cv.getContext("2d")!;
    let f = 0;
    const anim = setInterval(() => {
      g.fillStyle = `hsl(${(f++ * 9) % 360} 70% 40%)`;
      g.fillRect(0, 0, 640, 360);
    }, 33);
    const rec = new MediaRecorder(cv.captureStream(30), { mimeType: studio.container!.mime.replace(/,mp4a\.40\.2|,opus/, "") });
    const chunks: Blob[] = [];
    rec.ondataavailable = (e) => chunks.push(e.data);
    rec.start(500);
    await new Promise((r) => setTimeout(r, 6000));
    await new Promise<void>((r) => ((rec.onstop = () => r()), rec.stop()));
    clearInterval(anim);
    const blob = new Blob(chunks, { type: rec.mimeType });
    studio.clip = { blob, durationSecs: 6, songAtStart: 10, song: { id: project.id, title: project.title, artist: project.metadata?.artist ?? "", cover: null } };
    studio.clipUrl = URL.createObjectURL(blob);
    studio.phase = "review";
    await new Promise((r) => setTimeout(r, 800));
    const v = document.querySelector<HTMLVideoElement>(".stage > video")!;
    studio.unlockAudio();
    await v.play().catch((e: unknown) => log.push(`play failed ${String(e)}`));
    await new Promise((r) => setTimeout(r, 1500));
    log.push(`first renders=${renders.n} paused=${v.paused} t=${v.currentTime.toFixed(1)}`);
    const slider = document.querySelector<HTMLInputElement>(".mix input[type=range]")!;
    slider.value = "-10";
    slider.dispatchEvent(new Event("input", { bubbles: true }));
    await new Promise((r) => setTimeout(r, 1500));
    log.push(`after-edit renders=${renders.n} paused=${v.paused} t=${v.currentTime.toFixed(1)} songGain=${app.song?.project.songGainDb}`);
    const gainOf = () => (studio as unknown as { reviewGain: GainNode | null }).reviewGain?.gain.value;
    v.volume = 0.25;
    await new Promise((r) => setTimeout(r, 200));
    const atQuarter = gainOf();
    v.muted = true;
    await new Promise((r) => setTimeout(r, 200));
    log.push(`player volume 0.25 -> gain ${atQuarter}; muted -> gain ${gainOf()}`);
    v.muted = false;
    const s3 = studio as unknown as { audioFor: (c: AudioContext) => Promise<AudioBuffer> };
    await s3.audioFor(new AudioContext());
    log.push(`export-sound renders=${renders.n}`);
    document.title = "phase=remix | " + log.join(" | ");
  }
  if (params.get("phase") === "othersong") {
    // A clip filmed on another song: notice, no export; discarding films
    // this song with the camera on. Then: an exported clip of another song
    // is dropped when the studio reopens, and the camera starts.
    const { studio } = await import("../src/lib/video/studio.svelte");
    const log: string[] = [];
    const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
    const clipOf = (id: string, title: string) => {
      const blob = new Blob([new Uint8Array(1000)], { type: "video/mp4" });
      studio.clip = { blob, durationSecs: 5, songAtStart: 10, song: { id, title, artist: "Tom Jobim", cover: null } };
      studio.clipUrl = URL.createObjectURL(blob);
      studio.phase = "review";
    };
    await wait(1500);
    clipOf("other-song", "Garota de Ipanema");
    await wait(800);
    const notice = document.querySelector(".other-song")?.textContent?.trim().slice(0, 60);
    const exportBtn = document.querySelector<HTMLButtonElement>("aside .btn.primary");
    log.push(`notice="${notice}" exportDisabled=${exportBtn?.disabled}`);
    if (params.get("shot") === "1") {
      document.title = "phase=othersong";
    } else {
      [...document.querySelectorAll<HTMLButtonElement>(".other-song .btn")].pop()!.click();
      await wait(2500);
      log.push(`after discard: phase=${studio.phase} camera=${!!studio.stream} clip=${!!studio.clip}`);
      clipOf("other-song", "Garota de Ipanema");
      studio.saved = ["/x/a.mp4"];
      studio.closeCamera();
      app.videoOpen = false;
      await wait(500);
      app.videoOpen = true;
      await wait(2500);
      log.push(`reopen after export: phase=${studio.phase} camera=${!!studio.stream} clip=${!!studio.clip}`);
      document.title = "phase=othersong | " + log.join(" | ");
    }
  }
  if (params.get("phase") === "exportflow") {
    const { studio } = await import("../src/lib/video/studio.svelte");
    const blob = new Blob([new Uint8Array(1000)], { type: "video/mp4" });
    studio.clip = { blob, durationSecs: 5, songAtStart: 10, song: { id: project.id, title: project.title, artist: project.metadata?.artist ?? "", cover: null } };
    studio.clipUrl = URL.createObjectURL(blob);
    studio.phase = "review";
    await new Promise((r) => setTimeout(r, 800));
    const calls: string[] = [];
    const s2 = studio as unknown as Record<string, (...a: unknown[]) => unknown>;
    for (const name of ["discardClip", "close", "dismissExport", "cancel"]) {
      const orig = s2[name]!.bind(studio);
      s2[name] = (...a: unknown[]) => {
        calls.push(`${name}@${studio.phase}`);
        return orig(...a);
      };
    }
    (window as unknown as { calls: string[] }).calls = calls;
    const proto = Object.getPrototypeOf(studio) as object;
    const desc = Object.getOwnPropertyDescriptor(proto, "phase");
    if (desc?.set && desc.get) {
      Object.defineProperty(proto, "phase", {
        get: desc.get,
        set(v: unknown) {
          const where = (new Error().stack ?? "").split("\n").slice(2, 4).map((l) => l.trim().replace(/https?:\/\/[^/]+/, "")).join(" < ");
          calls.push(`SET phase=${String(v)} from ${where}`);
          desc.set!.call(this, v);
        },
        configurable: true,
      });
    } else calls.push("no phase accessor");
    const log: string[] = [`before phase=${studio.phase} container=${!!studio.container} formats=${studio.selectedFormats.length}`];
    const ow = console.warn;
    console.warn = (...a: unknown[]) => {
      calls.push("WARN " + a.map(String).join(" "));
      ow(...a);
    };
    void studio.export().then(
      () => calls.push(`export-resolved@${studio.phase} err=${studio.exportError}`),
      (e: unknown) => calls.push(`export-threw ${String(e)}`),
    );
    log.push(`after-export phase=${studio.phase}`);
    app.videoOpen = false; // same as the Export button
    log.push(`after-close phase=${studio.phase} clip=${studio.clip ? "yes" : "NULL"}`);
    for (const t of [0, 50, 200, 600]) {
      await new Promise((r) => setTimeout(r, t));
      log.push(`${t}ms phase=${studio.phase} clip=${studio.clip ? "yes" : "NULL"} card=${!!document.querySelector(".thumb")} inThumb=${!!document.querySelector(".thumb video")}`);
    }
    log.push(`calls=${calls.join(",")} err=${studio.exportError}`);
    document.title = log.join(" | ");
  }
  if (params.get("phase") === "review") {
    // Review screen with a dummy clip (layout checks only).
    const { studio } = await import("../src/lib/video/studio.svelte");
    const blob = new Blob([new Uint8Array(1000)], { type: "video/mp4" });
    studio.clip = { blob, durationSecs: 5, songAtStart: 10, song: { id: project.id, title: project.title, artist: project.metadata?.artist ?? "", cover: null } };
    studio.clipUrl = URL.createObjectURL(blob);
    studio.phase = "review";
  }
  if (params.get("phase") === "exporting" || params.get("phase") === "exported") {
    // Sidebar export cards (audio mixdown + video clips), studio closed.
    const { studio } = await import("../src/lib/video/studio.svelte");
    app.videoOpen = false;
    await new Promise((r) => setTimeout(r, 300)); // let the studio close first
    app.exportProgress = 0.37;
    studio.exportFormats = ["youtube", "vertical"];
    if (params.get("phase") === "exporting") {
      studio.phase = "exporting";
      studio.exportStage = "encoding";
      studio.progress = 0.42;
    } else {
      studio.saved = ["/Users/musician/Music/Joshua+/exports/a.mp4", "/Users/musician/Music/Joshua+/exports/b.mp4"];
    }
  }
  if (params.get("phase") === "recording") {
    const { studio } = await import("../src/lib/video/studio.svelte");
    studio.phase = "recording";
    studio.elapsed = 42;
  }
}

if (state === "gear") {
  app.openGear();
  setInterval(() => {
    void emit("input-levels", [0.55 + Math.random() * 0.3, 0.2 + Math.random() * 0.2]);
    void emit("drum-hit", { type: "strike", pad: "snare", note: 38, velocity: 112 });
    void emit("drum-hit", { type: "strike", pad: "crash1", note: 49, velocity: 90 });
    void emit("drum-hit", { type: "hiHatPedal", openness: 70 });
  }, 120);
}

if (state === "help") app.view = { kind: "help" };

if (state === "about") app.view = { kind: "about" };

// Any state: show a toast with a long message (layout checks).
if (params.get("toast") === "1") {
  const { i18n } = await import("../src/lib/i18n/index.svelte");
  setTimeout(() => app.notify(i18n.t("gear.monitorUnavailable", { out: 48000 }) + " " + i18n.t("error.unknown")), 300);
}
