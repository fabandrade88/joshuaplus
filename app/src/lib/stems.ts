import type { MessageKey } from "./i18n/en";
import type { StemKind } from "./types";

export const STEM_COLORS: Record<StemKind | "metronome", string> = {
  vocals: "var(--stem-vocals)",
  drums: "var(--stem-drums)",
  bass: "var(--stem-bass)",
  guitar: "var(--stem-guitar)",
  acoustic: "var(--stem-acoustic)",
  piano: "var(--stem-piano)",
  other: "var(--stem-other)",
  metronome: "var(--stem-click)",
};

export function stemLabelKey(stem: StemKind): MessageKey {
  return `stem.${stem}` as MessageKey;
}

export type EngineKind = "studio" | "quick" | "cloud";

/** Which engine produced a song (from the separator id stored in the project). */
export function engineOf(separatorId: string): EngineKind {
  if (separatorId.startsWith("cloud")) return "cloud";
  if (separatorId === "spectral") return "quick";
  return "studio";
}
