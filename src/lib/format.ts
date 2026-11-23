import type { Lang } from "./types";

const LATIN = ["B", "KB", "MB", "GB", "TB"];
const CYRILLIC = ["Б", "КБ", "МБ", "ГБ", "ТБ"];
const UNITS: Record<Lang, string[]> = {
  ru: CYRILLIC,
  uk: CYRILLIC,
  en: LATIN,
  es: LATIN,
  de: LATIN,
  pt: LATIN,
  zh: LATIN,
  fr: ["o", "Ko", "Mo", "Go", "To"],
};

const LOCALES: Record<Lang, string> = {
  ru: "ru-RU",
  en: "en-US",
  uk: "uk-UA",
  es: "es-ES",
  fr: "fr-FR",
  de: "de-DE",
  pt: "pt-BR",
  zh: "zh-CN",
};

/** BCP-47 locale for dates and numbers. */
export function localeOf(lang: Lang): string {
  return LOCALES[lang] ?? "en-US";
}

/** Languages that write 14,2 instead of 14.2. */
export function decimalComma(lang: Lang): boolean {
  return lang !== "en" && lang !== "zh";
}

/** Human-readable size: 14.2 ГБ / 14.2 GB. Handles 0, negatives, NaN. */
export function formatBytes(bytes: number, lang: Lang = "ru", digits = 1): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return `0 ${UNITS[lang][0]}`;
  let i = 0;
  let v = bytes;
  while (v >= 1024 && i < UNITS[lang].length - 1) {
    v /= 1024;
    i++;
  }
  const d = i === 0 ? 0 : v >= 100 ? 0 : digits;
  const num = v.toFixed(d);
  const shown = decimalComma(lang) ? num.replace(".", ",") : num;
  return `${shown} ${UNITS[lang][i]}`;
}

/** Percentage 0..100, clamped, rounded. */
export function pct(part: number, total: number): number {
  if (!Number.isFinite(part) || !Number.isFinite(total) || total <= 0) return 0;
  return Math.min(100, Math.max(0, Math.round((part / total) * 100)));
}

export function clamp(v: number, min: number, max: number): number {
  if (!Number.isFinite(v)) return min;
  return Math.min(max, Math.max(min, v));
}

/** Fixed decimals with the locale's separator: 14,2 (ru) / 14.2 (en). */
export function formatDecimal(n: number, digits: number, lang: Lang = "ru"): string {
  if (!Number.isFinite(n)) n = 0;
  const s = n.toFixed(digits);
  return decimalComma(lang) ? s.replace(".", ",") : s;
}

/** Number with locale separators: 12 345 */
export function formatInt(n: number, lang: Lang = "ru"): string {
  if (!Number.isFinite(n)) return "0";
  return Math.round(n).toLocaleString(localeOf(lang));
}

/** Uptime: "1 д 2 ч 3 мин" */
export function formatDuration(sec: number, lang: Lang = "ru"): string {
  if (!Number.isFinite(sec) || sec < 0) sec = 0;
  const d = Math.floor(sec / 86400);
  const h = Math.floor((sec % 86400) / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const units: Partial<Record<Lang, { d: string; h: string; m: string }>> = {
    ru: { d: "д", h: "ч", m: "мин" },
    uk: { d: "д", h: "год", m: "хв" },
    fr: { d: "j", h: "h", m: "min" },
    zh: { d: "天", h: "小时", m: "分钟" },
  };
  const u = units[lang] ?? { d: "d", h: "h", m: "min" };
  const parts: string[] = [];
  if (d) parts.push(`${d} ${u.d}`);
  if (h || d) parts.push(`${h} ${u.h}`);
  parts.push(`${m} ${u.m}`);
  return parts.join(" ");
}

/** Date from unix seconds -> "15 нояб. 2026, 21:04" */
export function formatDate(unixSec: number, lang: Lang = "ru"): string {
  if (!Number.isFinite(unixSec) || unixSec <= 0) return "—";
  return new Date(unixSec * 1000).toLocaleString(localeOf(lang), {
    day: "numeric",
    month: "short",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** Greeting key by hour. */
export function greetingKey(hour: number): "morning" | "day" | "evening" | "night" {
  if (hour >= 5 && hour < 12) return "morning";
  if (hour >= 12 && hour < 18) return "day";
  if (hour >= 18 && hour < 23) return "evening";
  return "night";
}

/** Russian plural: plural(5, ["файл","файла","файлов"]) */
export function plural(n: number, forms: [string, string, string]): string {
  const a = Math.abs(n) % 100;
  const b = a % 10;
  if (a > 10 && a < 20) return forms[2];
  if (b > 1 && b < 5) return forms[1];
  if (b === 1) return forms[0];
  return forms[2];
}

/** Last path segment of a Windows path. */
export function baseName(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const i = Math.max(trimmed.lastIndexOf("\\"), trimmed.lastIndexOf("/"));
  return i >= 0 ? trimmed.slice(i + 1) || trimmed : trimmed;
}

/** Parent folder of a Windows path, or null at drive root. */
export function parentPath(path: string): string | null {
  const trimmed = path.replace(/[\\/]+$/, "");
  if (/^[A-Za-z]:$/.test(trimmed)) return null;
  const i = Math.max(trimmed.lastIndexOf("\\"), trimmed.lastIndexOf("/"));
  if (i < 0) return null;
  const parent = trimmed.slice(0, i);
  return /^[A-Za-z]:$/.test(parent) ? parent + "\\" : parent;
}

/** First letters for avatar: "Google Chrome" -> "GC" */
export function initials(name: string): string {
  const words = name
    .replace(/[^\p{L}\p{N}\s]/gu, " ")
    .split(/\s+/)
    .filter(Boolean);
  if (words.length === 0) return "?";
  if (words.length === 1) return words[0].slice(0, 2).toUpperCase();
  return (words[0][0] + words[1][0]).toUpperCase();
}
