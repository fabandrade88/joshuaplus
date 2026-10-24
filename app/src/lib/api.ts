// Typed, single point of contact with the Rust backend.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ApiError,
  DrumHit,
  DrumPad,
  DevicesChanged,
  DrumState,
  ExportList,
  InputDeviceView,
  InputPatch,
  InputView,
  AppInfo,
  DeviceView,
  Folder,
  FolderColor,
  JobUpdate,
  MetronomePatch,
  RecordingResult,
  SeparatorChoice,
  SongSummary,
  SongView,
  TempoEdit,
  TempoMode,
  BeatAnalysis,
  TrackPatch,
  TransportTick,
} from "./types";

export function isApiError(e: unknown): e is ApiError {
  return typeof e === "object" && e !== null && typeof (e as ApiError).code === "string";
}

export const api = {
  appInfo: () => invoke<AppInfo>("get_app_info"),
  setLocale: (locale: string) => invoke<void>("set_locale", { locale }),
  setSeparator: (separator: SeparatorChoice) => invoke<void>("set_separator", { separator }),
  setWorkspace: (path: string, moveLibrary = false) => invoke<string>("set_workspace", { path, moveLibrary }),
  setExportDir: (path: string | null) => invoke<string | null>("set_export_dir", { path }),
  listSongs: () => invoke<SongSummary[]>("list_songs"),
  renameSong: (id: string, title: string) => invoke<string>("rename_song", { id, title }),
  importFiles: (paths: string[], folder: string | null) => invoke<number[]>("import_files", { paths, folder }),
  listFolders: () => invoke<Folder[]>("list_folders"),
  createFolder: (name: string, color: FolderColor) => invoke<Folder>("create_folder", { name, color }),
  updateFolder: (id: string, name: string | null, color: FolderColor | null) =>
    invoke<Folder>("update_folder", { id, name, color }),
  deleteFolder: (id: string) => invoke<void>("delete_folder", { id }),
  moveSong: (id: string, folder: string | null) => invoke<void>("move_song", { id, folder }),
  reorderFolder: (id: string, order: string[]) => invoke<string[]>("reorder_folder", { id, order }),
  downloadModel: () => invoke<void>("download_model"),
  setCloudKey: (key: string) => invoke<number>("set_cloud_key", { key }),
  clearCloudKey: () => invoke<void>("clear_cloud_key"),
  cancelJob: (id: number) => invoke<boolean>("cancel_job", { id }),
  deleteSong: (id: string) => invoke<void>("delete_song", { id }),
  loadSong: (id: string) => invoke<SongView>("load_song", { id }),
  play: () => invoke<void>("play"),
  pause: () => invoke<void>("pause"),
  seek: (seconds: number) => invoke<void>("seek", { seconds }),
  setLoop: (start: number | null, end: number | null) => invoke<void>("set_loop", { start, end }),
  updateTrack: (index: number, patch: TrackPatch) => invoke<void>("update_track", { index, patch }),
  updateMetronome: (patch: MetronomePatch) => invoke<void>("update_metronome", { patch }),
  editTempo: (edit: TempoEdit) => invoke<BeatAnalysis>("edit_tempo", { edit }),
  reanalyzeTempo: (mode: TempoMode) => invoke<BeatAnalysis>("reanalyze_tempo", { mode }),
  setMasterGain: (gainDb: number) => invoke<void>("set_master_gain", { gainDb }),
  updateSongBus: (gainDb: number | null, muted: boolean | null) =>
    invoke<void>("update_song_bus", { gainDb, muted }),
  listOutputDevices: () => invoke<DeviceView[]>("list_output_devices"),
  setOutputDevice: (id: string | null) => invoke<void>("set_output_device", { id }),
  listAudioInputs: () => invoke<InputDeviceView[]>("list_audio_inputs"),
  getInputs: () => invoke<InputView[]>("get_inputs"),
  addAudioInput: (deviceId: string) => invoke<InputView[]>("add_audio_input", { deviceId }),
  removeAudioInput: (index: number) => invoke<InputView[]>("remove_audio_input", { index }),
  updateAudioInput: (index: number, patch: InputPatch) => invoke<InputView[]>("update_audio_input", { index, patch }),
  getDrums: () => invoke<DrumState>("get_drums"),
  setMidiPort: (portId: string | null) => invoke<DrumState>("set_midi_port", { portId }),
  setDrumBrand: (brandId: string) => invoke<DrumState>("set_drum_brand", { brandId }),
  drumLearn: (pad: DrumPad | null) => invoke<DrumState>("drum_learn", { pad }),
  startRecording: (video = false) => invoke<void>("start_recording", { video }),
  stopRecording: () => invoke<RecordingResult>("stop_recording"),
  updateTake: (id: string, patch: TrackPatch) => invoke<void>("update_take", { id, patch }),
  deleteTake: (id: string) => invoke<SongView>("delete_take", { id }),
  setRecordOffset: (ms: number) => invoke<void>("set_record_offset", { ms }),
  exportMix: (includeClick: boolean) => invoke<string>("export_mix", { includeClick }),
  revealExport: (path: string) => invoke<void>("reveal_export", { path }),
  listExports: () => invoke<ExportList>("list_exports"),
  openExport: (path: string) => invoke<void>("open_export", { path }),
  openExportDir: () => invoke<void>("open_export_dir"),
  renderClipAudio: (startSecs: number, durationSecs: number) =>
    invoke<ArrayBuffer>("render_clip_audio", { startSecs, durationSecs }),
  videoSaveBegin: (tag: string, ext: string) => invoke<number>("video_save_begin", { tag, ext }),
  videoSaveChunk: (handle: number, bytes: Uint8Array) =>
    invoke<void>("video_save_chunk", bytes, { headers: { "x-handle": String(handle) } }),
  videoSaveEnd: (handle: number, keep: boolean) => invoke<string | null>("video_save_end", { handle, keep }),
};

export const events = {
  onJob: (cb: (u: JobUpdate) => void): Promise<UnlistenFn> => listen<JobUpdate>("job-update", (e) => cb(e.payload)),
  onTransport: (cb: (t: TransportTick) => void): Promise<UnlistenFn> =>
    listen<TransportTick>("transport", (e) => cb(e.payload)),
  onLibraryChanged: (cb: () => void): Promise<UnlistenFn> => listen("library-changed", () => cb()),
  onDevicesChanged: (cb: (c: DevicesChanged) => void): Promise<UnlistenFn> =>
    listen<DevicesChanged>("audio-devices-changed", (e) => cb(e.payload)),
  onInputLevels: (cb: (levels: number[]) => void): Promise<UnlistenFn> =>
    listen<number[]>("input-levels", (e) => cb(e.payload)),
  onDrumHit: (cb: (hit: DrumHit) => void): Promise<UnlistenFn> => listen<DrumHit>("drum-hit", (e) => cb(e.payload)),
  onDrumLearned: (cb: (pad: DrumPad, note: number) => void): Promise<UnlistenFn> =>
    listen<[DrumPad, number]>("drum-learned", (e) => cb(e.payload[0], e.payload[1])),
  onModelProgress: (cb: (p: number) => void): Promise<UnlistenFn> =>
    listen<number>("model-progress", (e) => cb(e.payload)),
  onExportProgress: (cb: (p: number) => void): Promise<UnlistenFn> =>
    listen<number>("export-progress", (e) => cb(e.payload)),
};
