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
mockIPC(
  (cmd) => {
    switch (cmd) {
      case "get_app_info":
        return {
          version: "0.0.1",
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
          { index: 0, deviceId: "coreaudio:ag06", name: "Yamaha AG06MK2", channels: 2, sampleRate: 48000, firstChannel: 0, stereo: false, monitor: true, gainDb: -3, monitorOk: true, error: null },
          { index: 1, deviceId: "coreaudio:spd", name: "SPD-SX PRO", channels: 8, sampleRate: 48000, firstChannel: 0, stereo: true, monitor: false, gainDb: 0, monitorOk: true, error: null },
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
if (state === "folder") app.openFolder("igreja");
if (state === "settings") app.settingsOpen = true;
if (state === "song" || state === "away") {
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
        meters: [0.62, 0.81, 0.55, 0, 0.4, 0.18, 0.33],
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
