// Mirrors of the Rust IPC types (serde camelCase).

export type StemKind = "vocals" | "drums" | "bass" | "guitar" | "acoustic" | "piano" | "other";
export type SeparatorChoice = "studio" | "quick" | "cloud";
export type ImportStage = "downloadingModel" | "validating" | "decoding" | "analyzingTempo" | "separating" | "finalizing";
export type JobStatus = "queued" | "running" | "done" | "failed" | "cancelled";

export interface ApiError {
  code: string;
  message: string;
}

export interface AppInfo {
  version: string;
  locale: string | null;
  workspace: string | null;
  inbox: string | null;
  separator: SeparatorChoice;
  modelInstalled: boolean;
  modelSizeMb: number;
  cloudConfigured: boolean;
  audio: { device: string; sampleRate: number; channels: number } | null;
  audioError: string | null;
  outputDevice: string | null;
  exportDir: string | null;
  exportDirCustom: boolean;
  suggestedWorkspace: string | null;
  musicDir: string | null;
  /** Tell the user about new releases. */
  updateCheck: boolean;
  autoBalance: boolean;
}

/** A newer published release. */
export interface UpdateInfo {
  version: string;
  url: string;
}

/** An audio interface was plugged in or out (see the device watchdog). */
export interface DevicesChanged {
  lostOutput: string | null;
  output: string | null;
  restored: boolean;
  inputsChanged: boolean;
}

/** Linux AppImage application-menu entry. */
export interface DesktopEntryStatus {
  available: boolean;
  installed: boolean;
  asked: boolean;
}

export interface ExportFile {
  name: string;
  path: string;
  kind: "audio" | "video";
  size: number;
  modifiedUnix: number;
}

export interface ExportList {
  dir: string;
  files: ExportFile[];
}

export interface SongSummary {
  id: string;
  title: string;
  bpm: number;
  durationSecs: number;
  separator: string;
  stems: number;
  createdUnix: number;
  folder: string | null;
  artist?: string | null;
  hasCover?: boolean;
  /** Thumbnail as a data: URL. */
  cover?: string | null;
}

export interface SongMetadata {
  title?: string | null;
  artist?: string | null;
  album?: string | null;
  albumArtist?: string | null;
  year?: string | null;
  genre?: string | null;
  track?: string | null;
  composer?: string | null;
  label?: string | null;
}

export type FolderColor = "amber" | "rose" | "violet" | "green" | "cyan" | "slate";

export interface Folder {
  id: string;
  name: string;
  color: FolderColor;
  createdUnix: number;
  order: string[];
}

export interface TrackSettings {
  stem: StemKind;
  file: string;
  gainDb: number;
  pan: number;
  muted: boolean;
  solo: boolean;
}

/** A recorded take (one input), aligned to the song. */
export interface TakeSettings {
  id: string;
  name: string;
  file: string;
  offsetSecs: number;
  createdUnix: number;
  gainDb: number;
  pan: number;
  muted: boolean;
  solo: boolean;
  /** Room send 0..1: puts the take in the same space as the band. */
  room: number;
}

export interface BeatAnalysis {
  bpm: number;
  beats: number[];
  beatsPerBar: number;
  downbeatPhase: number;
  confidence: number;
  variableTempo?: boolean;
}

export interface SongProject {
  schemaVersion: number;
  id: string;
  title: string;
  sourceFile: string;
  sampleRate: number;
  durationSecs: number;
  separator: string;
  createdUnix: number;
  analysis: BeatAnalysis;
  tracks: TrackSettings[];
  metronome: { gainDb: number; muted: boolean; countIn: boolean; countInBars: number; sound: ClickSound };
  masterGainDb: number;
  songGainDb: number;
  songMuted: boolean;
  metadata?: SongMetadata;
  hasCover?: boolean;
  titleEdited?: boolean;
  folder?: string | null;
  takes?: TakeSettings[];
}

export interface SongView {
  project: SongProject;
  peaks: number[][];
  audioReady: boolean;
  cover?: string | null;
}

export interface JobUpdate {
  id: number;
  fileName: string;
  status: JobStatus;
  stage: ImportStage | null;
  progress: number;
  error: ApiError | null;
  songId: string | null;
  folder: string | null;
}

export interface TransportTick {
  positionSecs: number;
  playing: boolean;
  countingIn: boolean;
  meters: number[];
  master: [number, number];
  songMeter: number;
  ended: boolean;
  deviceError: boolean;
  /** Takes are being recorded. */
  recording?: boolean;
}

/** The open song's mixer levels after a whole-mix change. */
export interface MixView {
  tracks: TrackSettings[];
  takes: TakeSettings[];
  songGainDb: number;
  songMuted: boolean;
  masterGainDb: number;
}

export interface BalancedTake {
  id: string;
  /** The original instrument it replaces (now muted), if one matched. */
  stem: StemKind | null;
  gainDb: number;
}

export interface BalanceResult {
  mix: MixView;
  takes: BalancedTake[];
}

export interface RecordingResult {
  song: SongView;
  added: number;
  overflowed: boolean;
}

export type RecordMode = "audio" | "video";

export interface DeviceView {
  id: string;
  name: string;
  isDefault: boolean;
}

export interface TrackPatch {
  gainDb?: number;
  pan?: number;
  muted?: boolean;
  solo?: boolean;
  room?: number;
}

export type ClickSound = "classic" | "woodblock" | "beep" | "cowbell" | "sticks" | "drums";

export interface MetronomePatch {
  gainDb?: number;
  muted?: boolean;
  countIn?: boolean;
  countInBars?: number;
  sound?: ClickSound;
}

export type TempoEdit =
  | { kind: "double" }
  | { kind: "half" }
  | { kind: "nudge"; value: number }
  | { kind: "downbeatAt"; value: number }
  | { kind: "beatsPerBar"; value: number }
  | { kind: "tap"; value: number[] }
  | { kind: "setBpm"; value: { bpm: number; anchor: number } };

export type TempoMode = "auto" | "constant" | "variable";

// ── Live inputs ─────────────────────────────────────────────────────────────

export interface InputDeviceView {
  id: string;
  name: string;
  channels: number;
  sampleRate: number;
  isDefault: boolean;
}

export interface InputView {
  index: number;
  deviceId: string;
  name: string;
  channels: number;
  sampleRate: number;
  firstChannel: number;
  stereo: boolean;
  monitor: boolean;
  gainDb: number;
  monitorOk: boolean;
  /** Same interface as the output: it monitors itself; the app won't play it again. */
  directMonitor: boolean;
  /** Armed: the next take records this input. */
  armed: boolean;
  canRecord: boolean;
  error: string | null;
}

export interface InputPatch {
  firstChannel?: number;
  stereo?: boolean;
  monitor?: boolean;
  gainDb?: number;
  armed?: boolean;
}

export type DrumPad =
  | "kick"
  | "snare"
  | "snareRim"
  | "snareCrossStick"
  | "hiHatClosed"
  | "hiHatOpen"
  | "hiHatPedal"
  | "tom1"
  | "tom2"
  | "tom3"
  | "tom4"
  | "crash1"
  | "crash2"
  | "ride"
  | "rideBell"
  | "splash"
  | "china"
  | "aux";

export interface KitBrand {
  id: string;
  name: string;
  models: string;
  preset: string;
  usbAudio: boolean;
}

export interface MidiPortInfo {
  id: string;
  name: string;
}

export interface DrumState {
  ports: MidiPortInfo[];
  portId: string | null;
  connected: string | null;
  error: string | null;
  brand: string;
  brands: KitBrand[];
  map: { notes: Record<string, DrumPad> };
  learning: DrumPad | null;
}

export type DrumHit =
  | { type: "strike"; pad: DrumPad | null; note: number; velocity: number }
  | { type: "hiHatPedal"; openness: number }
  | { type: "choke"; pad: DrumPad | null; note: number };
