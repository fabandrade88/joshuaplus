import { de } from "./de";
import { en, type MessageKey, type Messages } from "./en";
import { ptBR } from "./pt-BR";

export type Locale = "en" | "pt-BR" | "de";

export const LOCALES: { id: Locale; label: string }[] = [
  { id: "en", label: "English" },
  { id: "pt-BR", label: "Português (Brasil)" },
  { id: "de", label: "Deutsch" },
];

export const catalogs: Record<Locale, Messages> = { en, "pt-BR": ptBR, de };

/** Picks the best supported locale for a BCP-47 tag (e.g. from the OS). */
export function matchLocale(tag: string | null | undefined): Locale {
  const t = (tag ?? "").toLowerCase();
  if (t.startsWith("pt")) return "pt-BR";
  if (t.startsWith("de")) return "de";
  return "en";
}

/** Replaces `{name}` placeholders. Values are plain text (rendered escaped by Svelte). */
export function format(template: string, vars: Record<string, string | number> = {}): string {
  return template.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m));
}

class I18n {
  locale = $state<Locale>(matchLocale(globalThis.navigator?.language));

  t = (key: MessageKey, vars?: Record<string, string | number>): string =>
    format(catalogs[this.locale][key] ?? en[key], vars);

  /** "1 song" / "{n} songs". */
  songs = (n: number): string => (n === 1 ? this.t("home.songCountOne") : this.t("home.songCount", { n }));

  /** Translates an error code coming from the backend. */
  error = (code: string | undefined): string => {
    const key = `error.${code}` as MessageKey;
    return key in en ? this.t(key) : this.t("error.unknown");
  };
}

export const i18n = new I18n();
