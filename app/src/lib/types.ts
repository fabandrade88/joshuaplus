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
}

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
  error: string | null;
}

export interface InputPatch {
  firstChannel?: number;
  stereo?: boolean;
  monitor?: boolean;
  gainDb?: number;
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
