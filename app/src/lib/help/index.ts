import type { Locale } from "../i18n/index.svelte";
import { helpDe } from "./de";
import { helpEn } from "./en";
import { helpPtBR } from "./pt-BR";
import type { HelpSection } from "./types";

export type { HelpItem, HelpSection } from "./types";

export const helpCatalogs: Record<Locale, HelpSection[]> = { en: helpEn, "pt-BR": helpPtBR, de: helpDe };

/** Case/accent-insensitive search over terms and texts. */
export function filterHelp(sections: HelpSection[], query: string): HelpSection[] {
  const fold = (s: string) => s.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();
  const q = fold(query.trim());
  if (!q) return sections;
  return sections
    .map((s) => ({
      ...s,
      items: fold(s.title).includes(q) ? s.items : s.items.filter((i) => fold(`${i.term} ${i.text}`).includes(q)),
    }))
    .filter((s) => s.items.length > 0);
}
