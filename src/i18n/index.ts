import { useStore } from "../lib/store";
import type { Lang } from "../lib/types";
import { plural } from "../lib/format";
import { en } from "./en";
import { ru, type Dict, type Forms } from "./ru";

export const dicts: Record<Lang, Dict> = { ru, en };

/** Replace {key} placeholders. Unknown keys stay as-is. */
export function fmt(template: string, vars: Record<string, string | number> = {}): string {
  return template.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m));
}

/** "5 файлов" / "5 files" */
export function countOf(n: number, forms: Forms, lang: Lang): string {
  const word = lang === "ru" ? plural(n, forms) : n === 1 ? forms[0] : forms[1];
  return `${n.toLocaleString(lang === "ru" ? "ru-RU" : "en-US")} ${word}`;
}

export function useT(): Dict {
  const lang = useStore((s) => s.settings.language);
  return dicts[lang] ?? ru;
}

export function useLang(): Lang {
  return useStore((s) => s.settings.language);
}
