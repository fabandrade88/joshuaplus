// Central UI state. Components read from it and call its actions; only this
// module talks to the backend.
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { api, events, isApiError } from "./api";
import { hasMatchingAudio } from "./devices";
import { i18n, matchLocale, type Locale } from "./i18n/index.svelte";
import { orderSongs } from "./playlist";
import { stemLabelKey } from "./stems";
import type {
  AppInfo,
  BeatAnalysis,
  DrumPad,
  DesktopEntryStatus,
  UpdateInfo,
  DrumState,
  ExportList,
  InputPatch,
  InputView,
  Folder,
  FolderColor,
  JobUpdate,
  MetronomePatch,
  MixView,
  RecordMode,
  SeparatorChoice,
  SongSummary,
  SongView,
  TempoEdit,
  TempoMode,
  TrackPatch,
  TransportTick,
} from "./types";

const DONE_JOB_TTL_MS = 6000;
const UPDATE_EVERY_MS = 6 * 60 * 60 * 1000;

function readFlag(key: string): boolean {
  try {
    return localStorage.getItem(key) === "1";
  } catch {
    return false;
  }
}

function writeFlag(key: string, value: boolean): void {
  try {
    localStorage.setItem(key, value ? "1" : "0");
  } catch {
    /* storage unavailable: preference just isn't remembered */
  }
}
const METER_DECAY = 0.82;

/** `all` = every song, `null` = songs without a folder, otherwise a folder id. */
export type FolderKey = string | null | "all";

export type View = { kind: "home" } | { kind: "folder"; folder: FolderKey } | { kind: "song" } | { kind: "gear" } | { kind: "help" } | { kind: "about" } | { kind: "exports" } | { kind: "settings" };

function idleTransport(): TransportTick {
  return {
    positionSecs: 0,
    playing: false,
    countingIn: false,
    meters: [],
    master: [0, 0],
    songMeter: 0,
    ended: false,
    deviceError: false,
  };
}

class AppStore {
  info = $state<AppInfo | null>(null);
  songs = $state<SongSummary[]>([]);
  folders = $state<Folder[]>([]);
  jobs = $state<JobUpdate[]>([]);
  view = $state<View>({ kind: "home" });
  /** Folder the user was browsing; new drops are filed there. */
  browsing = $state<FolderKey>("all");
  song = $state<SongView | null>(null);
  loadingId = $state<string | null>(null);
  transport = $state<TransportTick>(idleTransport());
  /** Smoothed meter levels (0..1 linear peak), indexed like the tracks. */
  meters = $state<number[]>([]);
  masterMeter = $state<[number, number]>([0, 0]);
  songMeter = $state(0);
  /** Song stems shown as one strip (per-viewer preference). */
  songCollapsed = $state(readFlag("joshua.songCollapsed"));
  /** Play the next song of the folder when one ends. Default on. */
  autoAdvance = $state(!readFlag("joshua.noAutoAdvance"));
  loop = $state<{ start: number; end: number } | null>(null);
  loopEnabled = $state(false);
  dragging = $state(false);
  modelProgress = $state<number | null>(null);
  // Live inputs and e-drums.
  inputs = $state<InputView[]>([]);
  inputLevels = $state<number[]>([]);
  drums = $state<DrumState | null>(null);
  /** Last strike per pad: velocity and time (ms) — drives the pad lights. */
  padHits = $state<Partial<Record<DrumPad, { velocity: number; at: number }>>>({});
  hiHatOpenness = $state(0);
  lastNote = $state<{ note: number; velocity: number; pad: DrumPad | null; at: number } | null>(null);
  /** Names of the audio input devices present, to spot MIDI-only kits. */
  audioDeviceNames = $state<string[]>([]);
  // Recording and export.
  recordMode = $state<RecordMode>("audio");
  /** Waiting for the backend to align and save takes. */
  savingTakes = $state(false);
  /** The automatic balance is analysing the song. */
  balancing = $state(false);
  /** Export progress 0..1, or null when idle. */
  exportProgress = $state<number | null>(null);
  /** Last exported file (for "show in folder"). */
  lastExport = $state<string | null>(null);
  /** Files in the export folder (Exports view), newest first. */
  exports = $state<ExportList | null>(null);
  /** The "Export audio" dialog is open. */
  exportDialogOpen = $state(false);
  /** Linux AppImage: application-menu entry. */
  desktopEntry = $state<DesktopEntryStatus | null>(null);
  /** A newer release is out (sidebar notice). */
  update = $state<UpdateInfo | null>(null);
  private updateTimer: ReturnType<typeof setInterval> | undefined;
  /** The video studio overlay is open. */
  videoOpen = $state(false);
  toast = $state<string | null>(null);
  private toastTimer: ReturnType<typeof setTimeout> | undefined;

  get ready(): boolean {
    return this.info !== null;
  }

  get hasWorkspace(): boolean {
    return !!this.info?.workspace;
  }

  get canPlay(): boolean {
    return !!this.song?.audioReady;
  }

  /** Folder that drops and imports go to right now. */
  get dropFolder(): string | null {
    if (this.view.kind === "folder") return this.view.folder === "all" ? null : this.view.folder;
    if (this.view.kind === "song") return this.song?.project.folder ?? null;
    return null;
  }

  folderById(id: string | null | undefined): Folder | undefined {
    return id ? this.folders.find((f) => f.id === id) : undefined;
  }

  /** Songs of a list in playlist order: the folder's saved order first,
   *  then any others newest first. */
  songsIn(key: FolderKey): SongSummary[] {
    const pool = key === "all" ? this.songs : this.songs.filter((s) => (s.folder ?? null) === key);
    const order = typeof key === "string" && key !== "all" ? (this.folderById(key)?.order ?? []) : [];
    return orderSongs(pool, order);
  }

  /** The list the open song is being played from. */
  get playlist(): SongSummary[] {
    return this.songsIn(this.browsing);
  }

  get playlistIndex(): number {
    const id = this.song?.project.id;
    return id ? this.playlist.findIndex((s) => s.id === id) : -1;
  }

  get nextSong(): SongSummary | undefined {
    const i = this.playlistIndex;
    return i >= 0 ? this.playlist[i + 1] : undefined;
  }

  get prevSong(): SongSummary | undefined {
    const i = this.playlistIndex;
    return i > 0 ? this.playlist[i - 1] : undefined;
  }

  async init(): Promise<void> {
    await Promise.all([
      events.onJob((u) => this.onJob(u)),
      events.onTransport((t) => this.onTransport(t)),
      events.onLibraryChanged(() => void this.refreshLibrary()),
      events.onDevicesChanged((c) => {
        if (c.restored && c.output) this.notify(i18n.t("devices.restored", { name: c.output }));
        else if (c.lostOutput)
          this.notify(
            c.output
              ? i18n.t("devices.outputLost", { lost: c.lostOutput, name: c.output })
              : i18n.t("devices.outputNone", { lost: c.lostOutput }),
          );
        void this.refreshInfo();
        void this.refreshInputs();
      }),
      events.onModelProgress((p) => (this.modelProgress = p)),
      events.onExportProgress((p) => (this.exportProgress = p)),
      events.onInputLevels((l) => {
        this.inputLevels = l.map((v, i) => Math.max(v, (this.inputLevels[i] ?? 0) * METER_DECAY));
      }),
      events.onDrumHit((hit) => {
        if (hit.type === "strike") {
          this.lastNote = { note: hit.note, velocity: hit.velocity, pad: hit.pad, at: performance.now() };
          if (hit.pad) this.padHits = { ...this.padHits, [hit.pad]: { velocity: hit.velocity, at: performance.now() } };
        } else if (hit.type === "hiHatPedal") this.hiHatOpenness = hit.openness;
      }),
      events.onDrumLearned(() => void this.refreshDrums()),
    ]);
    await getCurrentWebview().onDragDropEvent((e) => {
      const p = e.payload;
      if (p.type === "enter" || p.type === "over") this.dragging = true;
      else if (p.type === "drop") {
        this.dragging = false;
        void this.importPaths(p.paths);
      } else this.dragging = false;
    });
    await this.refreshInfo();
    void this.refreshInputs();
    void this.refreshDrums();
    i18n.locale = this.info?.locale ? matchLocale(this.info.locale) : i18n.locale;
    await this.refreshLibrary();
    void this.refreshExports();
    void this.offerDesktopEntry();
    // New versions: shortly after start, then a few times a day.
    setTimeout(() => void this.checkForUpdate(), 4000);
    clearInterval(this.updateTimer);
    this.updateTimer = setInterval(() => void this.checkForUpdate(), UPDATE_EVERY_MS);
  }

  async checkForUpdate(): Promise<void> {
    this.update = await api.checkForUpdate().catch(() => null);
  }

  /** "Not now": hide the notice until an even newer version is out. */
  async skipUpdate(): Promise<void> {
    const version = this.update?.version;
    this.update = null;
    if (version) await this.guard(() => api.skipUpdate(version));
  }

  async setUpdateCheck(enabled: boolean): Promise<void> {
    if ((await this.guard(() => api.setUpdateCheck(enabled))) === undefined) return;
    if (this.info) this.info.updateCheck = enabled;
    if (enabled) void this.checkForUpdate();
    else this.update = null;
  }

  /** Linux AppImage: ask once whether to add Joshua+ to the application
   *  menu (the AppImage cannot be found there otherwise). */
  private async offerDesktopEntry(): Promise<void> {
    const status = await api.desktopEntryStatus().catch(() => null);
    this.desktopEntry = status;
    if (!status?.available || status.asked || status.installed) return;
    const yes = window.confirm(i18n.t("desktopEntry.ask"));
    await this.setDesktopEntry(yes);
    if (yes) this.notify(i18n.t("desktopEntry.added"));
  }

  async setDesktopEntry(install: boolean): Promise<void> {
    const status = await this.guard(() => api.setDesktopEntry(install));
    if (status) this.desktopEntry = status;
  }

  async refreshInfo(): Promise<void> {
    this.info = await api.appInfo();
  }

  async refreshLibrary(): Promise<void> {
    if (!this.hasWorkspace) return;
    await this.guard(async () => {
      const [songs, folders] = await Promise.all([api.listSongs(), api.listFolders()]);
      this.songs = songs;
      this.folders = folders;
    });
  }

  // ── Notifications ────────────────────────────────────────────────────────
  notify(message: string): void {
    this.toast = message;
    clearTimeout(this.toastTimer);
    this.toastTimer = setTimeout(() => (this.toast = null), 4500);
  }

  private async guard<T>(fn: () => Promise<T>): Promise<T | undefined> {
    try {
      return await fn();
    } catch (e) {
      this.notify(isApiError(e) ? i18n.error(e.code) : i18n.t("error.unknown"));
      return undefined;
    }
  }

  // ── Navigation ───────────────────────────────────────────────────────────
  goHome(): void {
    this.view = { kind: "home" };
    this.browsing = "all"; // songs opened from Home play through the library
  }

  openFolder(folder: FolderKey): void {
    this.browsing = folder;
    this.view = { kind: "folder", folder };
  }

  backFromSong(): void {
    this.openFolder(this.browsing);
  }

  // ── Folders ──────────────────────────────────────────────────────────────
  async createFolder(name: string, color: FolderColor): Promise<Folder | undefined> {
    const folder = await this.guard(() => api.createFolder(name, color));
    if (folder) this.folders = [...this.folders, folder];
    return folder;
  }

  async updateFolder(id: string, name: string | null, color: FolderColor | null): Promise<void> {
    const f = await this.guard(() => api.updateFolder(id, name, color));
    if (f) this.folders = this.folders.map((x) => (x.id === f.id ? f : x));
  }

  async deleteFolder(id: string): Promise<void> {
    if ((await this.guard(() => api.deleteFolder(id))) === undefined) return;
    if (this.song?.project.folder === id) this.song.project.folder = null;
    if (this.view.kind === "folder" && this.view.folder === id) this.goHome();
    await this.refreshLibrary();
  }

  /** Saves a new playlist order for a folder (optimistic). */
  async reorderFolder(id: string, order: string[]): Promise<void> {
    const folder = this.folderById(id);
    if (!folder) return;
    folder.order = order;
    const saved = await this.guard(() => api.reorderFolder(id, order));
    if (saved) folder.order = saved;
  }

  async renameSong(id: string, title: string): Promise<void> {
    const t = title.trim();
    if (!t) return;
    const saved = await this.guard(() => api.renameSong(id, t));
    if (saved === undefined) return;
    this.songs = this.songs.map((s) => (s.id === id ? { ...s, title: saved } : s));
    if (this.song?.project.id === id) this.song.project.title = saved;
  }

  async moveSong(id: string, folder: string | null): Promise<void> {
    if ((await this.guard(() => api.moveSong(id, folder))) === undefined) return;
    this.songs = this.songs.map((s) => (s.id === id ? { ...s, folder } : s));
    if (this.song?.project.id === id) this.song.project.folder = folder;
  }

  openSettings(): void {
    this.view = { kind: "settings" };
  }

  // ── Inputs & e-drums ─────────────────────────────────────────────────────
  openGear(): void {
    this.view = { kind: "gear" };
    void this.refreshInputs();
    void this.refreshDrums();
  }

  async refreshInputs(): Promise<void> {
    const v = await this.guard(() => api.getInputs());
    if (v) this.inputs = v;
  }

  async addInput(deviceId: string): Promise<void> {
    const v = await this.guard(() => api.addAudioInput(deviceId));
    if (v) this.inputs = v;
  }

  async removeInput(index: number): Promise<void> {
    const v = await this.guard(() => api.removeAudioInput(index));
    if (v) this.inputs = v;
  }

  async updateInput(index: number, patch: InputPatch): Promise<void> {
    const local = this.inputs[index];
    if (local) Object.assign(local, patch);
    const v = await this.guard(() => api.updateAudioInput(index, patch));
    if (v) this.inputs = v;
  }

  async refreshDrums(): Promise<void> {
    const [d, audio] = await Promise.all([this.guard(() => api.getDrums()), this.guard(() => api.listAudioInputs())]);
    if (d) this.drums = d;
    if (audio) this.audioDeviceNames = audio.map((a) => a.name);
  }

  /** The connected kit sends MIDI over USB but no audio device of that name exists. */
  get drumsMidiOnly(): boolean {
    const name = this.drums?.connected;
    return !!name && !hasMatchingAudio(name, this.audioDeviceNames);
  }

  async setMidiPort(portId: string | null): Promise<void> {
    const d = await this.guard(() => api.setMidiPort(portId));
    if (d) this.drums = d;
  }

  async setDrumBrand(brandId: string): Promise<void> {
    const d = await this.guard(() => api.setDrumBrand(brandId));
    if (d) this.drums = d;
  }

  async drumLearn(pad: DrumPad | null): Promise<void> {
    const d = await this.guard(() => api.drumLearn(pad));
    if (d) this.drums = d;
  }

  // ── Settings ─────────────────────────────────────────────────────────────
  async setLocale(locale: Locale): Promise<void> {
    i18n.locale = locale;
    await this.guard(() => api.setLocale(locale));
  }

  async setSeparator(choice: SeparatorChoice): Promise<void> {
    await this.guard(() => api.setSeparator(choice));
    await this.refreshInfo();
  }

  async downloadModel(): Promise<void> {
    this.modelProgress = 0;
    await this.guard(() => api.downloadModel());
    this.modelProgress = null;
    await this.refreshInfo();
  }

  /** Returns the minutes left on the account, or undefined if the key was rejected. */
  async setCloudKey(key: string): Promise<number | undefined> {
    const minutes = await this.guard(() => api.setCloudKey(key));
    await this.refreshInfo();
    return minutes;
  }

  async clearCloudKey(): Promise<void> {
    await this.guard(() => api.clearCloudKey());
    await this.refreshInfo();
  }

  async setOutputDevice(id: string | null): Promise<void> {
    await this.guard(() => api.setOutputDevice(id));
    await this.refreshInfo();
    // The backend reloaded the song at the new rate (loop cleared, back to 0).
    if (this.song) await this.openSong(this.song.project.id, false, this.view.kind === "song", true);
  }

  /** New library folder picked in Settings, waiting for "move" or "empty". */
  pendingWorkspace = $state<string | null>(null);
  movingLibrary = $state(false);

  /** Folder picker that opens in the Music folder. */
  private async pickFolder(start: string | null | undefined): Promise<string | null> {
    const dir = await open({ directory: true, multiple: false, recursive: false, defaultPath: start ?? undefined });
    return typeof dir === "string" ? dir : null;
  }

  /** Welcome screen: use the recommended `<Music>/Joshua+` folder. */
  async useSuggestedWorkspace(): Promise<void> {
    const dir = this.info?.suggestedWorkspace;
    if (dir) await this.applyWorkspace(dir, false);
  }

  /** Picks a library folder. With songs in the current library, Settings
   *  then asks whether to move them there or start an empty library. */
  async chooseWorkspace(): Promise<void> {
    const dir = await this.pickFolder(this.info?.musicDir);
    if (!dir || dir === this.info?.workspace) return;
    if (this.hasWorkspace && this.songs.length > 0) this.pendingWorkspace = dir;
    else await this.applyWorkspace(dir, false);
  }

  async applyWorkspace(dir: string, move: boolean): Promise<void> {
    this.movingLibrary = move;
    const ok = await this.guard(() => api.setWorkspace(dir, move));
    this.movingLibrary = false;
    this.pendingWorkspace = null;
    if (ok === undefined) return;
    this.song = null;
    this.transport = idleTransport();
    this.goHome();
    await this.refreshInfo();
    await this.refreshLibrary();
    this.exports = null;
    void this.refreshExports();
    if (move) this.notify(i18n.t("workspace.moved"));
  }

  async chooseExportDir(): Promise<void> {
    const dir = await this.pickFolder(this.info?.exportDir ?? this.info?.musicDir);
    if (!dir) return;
    if ((await this.guard(() => api.setExportDir(dir))) !== undefined) await this.afterExportDirChange();
  }

  async resetExportDir(): Promise<void> {
    if ((await this.guard(() => api.setExportDir(null))) !== undefined) await this.afterExportDirChange();
  }

  private async afterExportDirChange(): Promise<void> {
    await this.refreshInfo();
    if (this.exports) await this.refreshExports();
  }

  // ── Import jobs ──────────────────────────────────────────────────────────
  async importPaths(paths: string[]): Promise<void> {
    if (paths.length === 0) return;
    if (!this.hasWorkspace) return this.notify(i18n.t("error.no_workspace"));
    const folder = this.dropFolder;
    await this.guard(() => api.importFiles(paths, folder));
  }

  async pickFiles(): Promise<void> {
    const files = await open({
      multiple: true,
      directory: false,
      filters: [{ name: "Audio", extensions: ["mp3", "wav", "flac", "ogg", "oga", "m4a", "mp4", "aac", "aif", "aiff"] }],
    });
    if (Array.isArray(files)) await this.importPaths(files);
    else if (typeof files === "string") await this.importPaths([files]);
  }

  private onJob(u: JobUpdate): void {
    const i = this.jobs.findIndex((j) => j.id === u.id);
    if (i >= 0) this.jobs[i] = u;
    else this.jobs.push(u);
    if (u.status === "done" || u.status === "cancelled") {
      setTimeout(() => this.dismissJob(u.id), DONE_JOB_TTL_MS);
    }
    if (u.status === "done" && this.info && !this.info.modelInstalled) void this.refreshInfo();
  }

  async cancelJob(id: number): Promise<void> {
    await this.guard(() => api.cancelJob(id));
  }

  dismissJob(id: number): void {
    this.jobs = this.jobs.filter((j) => j.id !== id);
  }

  // ── Song & transport ─────────────────────────────────────────────────────
  /** Starts the folder's playlist from its first song. */
  async playFolder(key: FolderKey): Promise<void> {
    this.browsing = key;
    const first = this.songsIn(key)[0];
    if (first) await this.openSong(first.id, true);
  }

  /** Next song of the playlist. Only switches screens when the user is
   *  already looking at the song (auto-advance never yanks them away). */
  async playNext(autoplay = true): Promise<void> {
    const next = this.nextSong;
    if (next) await this.openSong(next.id, autoplay, this.view.kind === "song");
  }

  async playPrev(): Promise<void> {
    const prev = this.prevSong;
    if (prev) await this.openSong(prev.id, this.transport.playing);
  }

  toggleAutoAdvance(): void {
    this.autoAdvance = !this.autoAdvance;
    writeFlag("joshua.noAutoAdvance", !this.autoAdvance);
  }

  /** Opens a song. If it is already loaded, just returns to its screen —
   *  playback keeps going from where it is. */
  /** Increases with every load; only the latest request may update the UI. */
  private loadSeq = 0;

  async openSong(id: string, autoplay = false, show = true, force = false): Promise<void> {
    if (!force && this.song?.project.id === id && this.loadingId === null) {
      if (show) this.view = { kind: "song" };
      if (autoplay && !this.transport.playing && !this.transport.countingIn) await this.guard(() => api.play());
      return;
    }
    const seq = ++this.loadSeq;
    this.loadingId = id;
    const view = await this.guard(() => api.loadSong(id));
    if (seq !== this.loadSeq) return; // a newer click won; ignore this result
    this.loadingId = null;
    if (!view) return;
    this.song = view;
    if (show) this.view = { kind: "song" };
    this.loop = null;
    this.loopEnabled = false;
    this.meters = [...view.project.tracks, ...(view.project.takes ?? [])].map(() => 0);
    this.transport = idleTransport();
    if (!view.audioReady) this.notify(i18n.t("song.noAudio"));
    else if (autoplay) await this.guard(() => api.play());
  }

  async deleteSong(id: string): Promise<void> {
    await this.guard(() => api.deleteSong(id));
    if (this.song?.project.id === id) {
      this.song = null;
      if (this.view.kind === "song") this.backFromSong();
    }
    await this.refreshLibrary();
  }

  private onTransport(t: TransportTick): void {
    this.transport = t;
    this.lastTickAt = performance.now();
    if (t.ended && this.autoAdvance && this.nextSong && this.loadingId === null) void this.playNext();
    this.meters = t.meters.map((m, i) => Math.max(m, (this.meters[i] ?? 0) * METER_DECAY));
    this.songMeter = Math.max(t.songMeter, this.songMeter * METER_DECAY);
    this.masterMeter = [
      Math.max(t.master[0], this.masterMeter[0] * METER_DECAY),
      Math.max(t.master[1], this.masterMeter[1] * METER_DECAY),
    ];
  }

  /** Back to the open song's screen (mini-player). */
  showSong(): void {
    if (this.song) this.view = { kind: "song" };
  }

  async togglePlay(): Promise<void> {
    if (!this.canPlay) return;
    await this.guard(() => (this.transport.playing || this.transport.countingIn ? api.pause() : api.play()));
  }

  async seek(seconds: number): Promise<void> {
    if (!this.canPlay) return;
    const dur = this.song?.project.durationSecs ?? 0;
    const s = Math.min(Math.max(0, seconds), dur);
    this.transport.positionSecs = s;
    await this.guard(() => api.seek(s));
  }

  async setLoopRegion(start: number, end: number): Promise<void> {
    const [a, b] = start < end ? [start, end] : [end, start];
    if (b - a < 0.1) return;
    this.loop = { start: a, end: b };
    this.loopEnabled = true;
    await this.guard(() => api.setLoop(a, b));
  }

  async clearLoop(): Promise<void> {
    this.loop = null;
    this.loopEnabled = false;
    await this.guard(() => api.setLoop(null, null));
  }

  async toggleLoop(): Promise<void> {
    if (!this.loop) return this.notify(i18n.t("transport.loopHint"));
    this.loopEnabled = !this.loopEnabled;
    const { start, end } = this.loop;
    await this.guard(() => (this.loopEnabled ? api.setLoop(start, end) : api.setLoop(null, null)));
  }

  // ── Mixer ────────────────────────────────────────────────────────────────
  async updateTrack(index: number, patch: TrackPatch): Promise<void> {
    const track = this.song?.project.tracks[index];
    if (!track) return;
    Object.assign(track, patch);
    await this.guard(() => api.updateTrack(index, patch));
  }

  // ── Recording ────────────────────────────────────────────────────────────
  get recording(): boolean {
    return !!this.transport.recording;
  }

  /** Inputs whose device is connected and running. Disconnected ones are
   *  only listed in Instruments & inputs (tagged "disconnected"); the song
   *  screen and recording ignore them. */
  get liveInputs(): InputView[] {
    return this.inputs.filter((i) => !i.error);
  }

  get armedCount(): number {
    return this.liveInputs.filter((i) => i.armed && i.canRecord).length;
  }

  /** Index of a take's meter (takes follow the stems). */
  takeMeter(j: number): number {
    return this.meters[(this.song?.project.tracks.length ?? 0) + j] ?? 0;
  }

  async toggleArm(index: number): Promise<void> {
    const input = this.inputs[index];
    if (input) await this.updateInput(index, { armed: !input.armed });
  }

  /** Starts recording every armed input; the song plays with its count-in.
   *  The video studio passes `video` (filming works without armed inputs).
   *  Returns whether recording started. */
  async startRecording(video = false): Promise<boolean> {
    if (!this.canPlay || this.recording) return false;
    if (!video && this.armedCount === 0) {
      this.notify(i18n.t("record.armFirst"));
      return false;
    }
    // A take is one straight pass through the song.
    if (this.loopEnabled) await this.clearLoop();
    if (this.transport.playing || this.transport.countingIn) await this.guard(() => api.pause());
    const ok = (await this.guard(() => api.startRecording(video))) !== undefined;
    if (ok) this.transport.recording = true;
    return ok;
  }

  async stopRecording(): Promise<void> {
    if (!this.recording || this.savingTakes) return;
    this.transport.recording = false;
    this.savingTakes = true;
    const before = new Set((this.song?.project.takes ?? []).map((t) => t.id));
    const result = await this.guard(() => api.stopRecording());
    this.savingTakes = false;
    if (!result) return;
    this.applySongView(result.song);
    if (result.overflowed) this.notify(i18n.t("record.overflow"));
    else this.notify(result.added > 0 ? i18n.t("record.saved", { n: result.added }) : i18n.t("record.nothing"));
    // Only the takes just recorded are balanced: everything else the user
    // set (levels, mutes, earlier takes) stays as it is.
    const fresh = (result.song.project.takes ?? []).filter((t) => !before.has(t.id)).map((t) => t.id);
    if (fresh.length > 0 && this.info?.autoBalance) await this.autoBalance(fresh);
  }

  /** Installs the levels of a whole-mix change. */
  private applyMix(mix: MixView): void {
    const p = this.song?.project;
    if (!p) return;
    p.tracks = mix.tracks;
    p.takes = mix.takes;
    p.songGainDb = mix.songGainDb;
    p.songMuted = mix.songMuted;
    p.masterGainDb = mix.masterGainDb;
  }

  /** Every volume back to the song's original balance (asks first).
   *  Returns whether anything changed. */
  async resetMix(): Promise<boolean> {
    if (!this.song || !window.confirm(i18n.t("mix.resetConfirm"))) return false;
    const mix = await this.guard(() => api.resetMix());
    if (!mix) return false;
    this.applyMix(mix);
    this.notify(i18n.t("mix.resetDone"));
    return true;
  }

  /** Balances the takes against the original instruments. Returns whether
   *  the mix changed. */
  async autoBalance(takes?: string[]): Promise<boolean> {
    if (!this.song?.project.takes?.length || this.balancing) return false;
    this.balancing = true;
    const result = await this.guard(() => api.autoBalance(takes));
    this.balancing = false;
    if (!result) return false;
    this.applyMix(result.mix);
    const names = new Map(result.mix.takes.map((t) => [t.id, t.name]));
    const replaced = result.takes
      .filter((t) => t.stem)
      .map((t) => i18n.t("mix.replaces", { take: names.get(t.id) ?? "", stem: i18n.t(stemLabelKey(t.stem!)) }));
    this.notify(replaced.length > 0 ? i18n.t("mix.balanced", { list: replaced.join(" · ") }) : i18n.t("mix.balancedNone"));
    return true;
  }

  /** Takes whose tone match is being computed. */
  matchingTone = $state<string[]>([]);

  /** Sets a take's tone; one recorded before tone matching gets its match
   *  computed first (level, pan and mutes are left alone). */
  async setTakeTone(id: string, tone: number): Promise<void> {
    const take = this.song?.project.takes?.find((t) => t.id === id);
    if (!take) return;
    if (!take.toneEq?.some((g) => g !== 0)) {
      if (this.matchingTone.includes(id)) return;
      this.matchingTone = [...this.matchingTone, id];
      const mix = await this.guard(() => api.matchTone(id));
      this.matchingTone = this.matchingTone.filter((t) => t !== id);
      if (!mix) return;
      this.applyMix(mix);
    }
    await this.updateTake(id, { tone });
  }

  async setAutoBalance(enabled: boolean): Promise<void> {
    if ((await this.guard(() => api.setAutoBalance(enabled))) === undefined) return;
    if (this.info) this.info.autoBalance = enabled;
  }

  async toggleRecording(): Promise<void> {
    await (this.recording ? this.stopRecording() : this.startRecording());
  }

  async updateTake(id: string, patch: TrackPatch): Promise<void> {
    const take = this.song?.project.takes?.find((t) => t.id === id);
    if (!take) return;
    Object.assign(take, patch);
    await this.guard(() => api.updateTake(id, patch));
  }

  async deleteTake(id: string): Promise<void> {
    const view = await this.guard(() => api.deleteTake(id));
    if (view) this.applySongView(view);
  }

  /** Installs a reloaded view of the open song (same song, new tracks). */
  private applySongView(view: SongView): void {
    this.song = view;
    this.meters = [...view.project.tracks, ...(view.project.takes ?? [])].map(() => 0);
  }

  async exportMix(includeClick: boolean): Promise<void> {
    if (!this.song || this.exportProgress !== null) return;
    this.exportDialogOpen = false;
    this.exportProgress = 0;
    const path = await this.guard(() => api.exportMix(includeClick));
    this.exportProgress = null;
    if (!path) return;
    this.lastExport = path;
    this.notify(i18n.t("export.done"));
    void this.refreshExports();
  }

  async revealExport(path = this.lastExport): Promise<void> {
    if (path) await this.guard(() => api.revealExport(path));
  }

  openExports(): void {
    this.view = { kind: "exports" };
    void this.refreshExports();
  }

  async refreshExports(): Promise<void> {
    if (!this.hasWorkspace) return;
    const list = await this.guard(() => api.listExports());
    if (list) this.exports = list;
  }

  async openExport(path: string): Promise<void> {
    await this.guard(() => api.openExport(path));
  }

  async openExportDir(): Promise<void> {
    await this.guard(() => api.openExportDir());
  }

  async updateMetronome(patch: MetronomePatch): Promise<void> {
    if (!this.song) return;
    Object.assign(this.song.project.metronome, patch);
    await this.guard(() => api.updateMetronome(patch));
  }

  analyzing = $state(false);
  /** Taps collected for tap-to-align (song seconds). */
  taps = $state<number[]>([]);
  private lastTickAt = 0;
  private tapTimer: ReturnType<typeof setTimeout> | undefined;

  /** Song time right now, interpolated between transport ticks. */
  private songNow(): number {
    const t = this.transport;
    return t.playing ? t.positionSecs + (performance.now() - this.lastTickAt) / 1000 : t.positionSecs;
  }

  /** Records a tap; ≥ 4 taps followed by a short pause align the grid. */
  tap(): void {
    if (!this.song || !this.transport.playing) return this.notify(i18n.t("tempo.tapHint"));
    const now = this.songNow();
    const last = this.taps[this.taps.length - 1];
    this.taps = last !== undefined && now - last < 2.5 && now > last ? [...this.taps, now] : [now];
    clearTimeout(this.tapTimer);
    this.tapTimer = setTimeout(() => {
      const taps = this.taps;
      this.taps = [];
      if (taps.length >= 4) void this.editTempo({ kind: "tap", value: taps });
    }, 1300);
  }

  async reanalyzeTempo(mode: TempoMode = "auto"): Promise<void> {
    if (!this.song) return;
    const id = this.song.project.id;
    this.analyzing = true;
    const analysis = await this.guard(() => api.reanalyzeTempo(mode));
    this.analyzing = false;
    // Only apply to the song that was analysed (the user may have switched).
    if (analysis && this.song?.project.id === id) this.applyAnalysis(analysis);
  }

  private applyAnalysis(analysis: BeatAnalysis): void {
    if (!this.song) return;
    this.song.project.analysis = analysis;
    const id = this.song.project.id;
    this.songs = this.songs.map((s) => (s.id === id ? { ...s, bpm: analysis.bpm } : s));
  }

  async editTempo(edit: TempoEdit): Promise<void> {
    if (!this.song) return;
    const analysis = await this.guard(() => api.editTempo(edit));
    if (!analysis) return;
    this.applyAnalysis(analysis);
    if (edit.kind === "setBpm") this.song.project.typedBpm = edit.value.bpm;
  }

  /** The tempo the user typed, when the current one differs (e.g. after
   *  detecting the tempo again): offered back under the BPM. */
  get typedBpmReminder(): number | null {
    const p = this.song?.project;
    const typed = p?.typedBpm;
    return typed && Math.abs(typed - p.analysis.bpm) >= 0.05 ? typed : null;
  }

  async useTypedBpm(): Promise<void> {
    const bpm = this.typedBpmReminder;
    if (bpm) await this.editTempo({ kind: "setBpm", value: { bpm, anchor: this.transport.positionSecs } });
  }

  async forgetTypedBpm(): Promise<void> {
    if (!this.song || (await this.guard(() => api.forgetTypedBpm())) === undefined) return;
    this.song.project.typedBpm = null;
  }

  toggleSongCollapsed(): void {
    this.songCollapsed = !this.songCollapsed;
    writeFlag("joshua.songCollapsed", this.songCollapsed);
  }

  async updateSongBus(gainDb: number | null, muted: boolean | null): Promise<void> {
    if (!this.song) return;
    if (gainDb !== null) this.song.project.songGainDb = gainDb;
    if (muted !== null) this.song.project.songMuted = muted;
    await this.guard(() => api.updateSongBus(gainDb, muted));
  }

  async setMasterGain(gainDb: number): Promise<void> {
    if (!this.song) return;
    this.song.project.masterGainDb = gainDb;
    await this.guard(() => api.setMasterGain(gainDb));
  }
}

export const app = new AppStore();
