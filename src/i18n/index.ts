import { useStore } from "../lib/store";
import type { Lang } from "../lib/types";
import { localeOf, plural } from "../lib/format";
import { de } from "./de";
import { en } from "./en";
import { es } from "./es";
import { fr } from "./fr";
import { pt } from "./pt";
import { ru, type Dict, type Forms } from "./ru";
import { uk } from "./uk";
import { zh } from "./zh";

export const dicts: Record<Lang, Dict> = { ru, en, uk, es, fr, de, pt, zh };

/** Languages in the pickers (settings and installer), each named in its own language. */
export const LANGS: { id: Lang; name: string; short: string }[] = [
  { id: "ru", name: "Русский", short: "RU" },
  { id: "uk", name: "Українська", short: "UK" },
  { id: "en", name: "English", short: "EN" },
  { id: "es", name: "Español", short: "ES" },
  { id: "fr", name: "Français", short: "FR" },
  { id: "de", name: "Deutsch", short: "DE" },
  { id: "pt", name: "Português", short: "PT" },
  { id: "zh", name: "中文（简体）", short: "中文" },
];

/** Replace {key} placeholders. Unknown keys stay as-is. */
export function fmt(template: string, vars: Record<string, string | number> = {}): string {
  return template.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m));
}

/** "5 файлов" / "5 files" / "5 个文件". Russian and Ukrainian use three forms, the rest one/other. */
export function countOf(n: number, forms: Forms, lang: Lang): string {
  const word = lang === "ru" || lang === "uk" ? plural(n, forms) : n === 1 ? forms[0] : forms[1];
  return `${n.toLocaleString(localeOf(lang))} ${word}`;
}

export function useT(): Dict {
  const lang = useStore((s) => s.settings.language);
  return dicts[lang] ?? ru;
}

export function useLang(): Lang {
  return useStore((s) => s.settings.language);
}
