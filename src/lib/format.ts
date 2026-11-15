import type { Lang } from "./types";

const UNITS: Record<Lang, string[]> = {
  ru: ["Б", "КБ", "МБ", "ГБ", "ТБ"],
  en: ["B", "KB", "MB", "GB", "TB"],
};

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
  const shown = lang === "ru" ? num.replace(".", ",") : num;
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
  return lang === "ru" ? s.replace(".", ",") : s;
}

/** Number with locale separators: 12 345 */
export function formatInt(n: number, lang: Lang = "ru"): string {
  if (!Number.isFinite(n)) return "0";
  return Math.round(n).toLocaleString(lang === "ru" ? "ru-RU" : "en-US");
}

/** Uptime: "1 д 2 ч 3 мин" */
export function formatDuration(sec: number, lang: Lang = "ru"): string {
  if (!Number.isFinite(sec) || sec < 0) sec = 0;
  const d = Math.floor(sec / 86400);
  const h = Math.floor((sec % 86400) / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const u = lang === "ru" ? { d: "д", h: "ч", m: "мин" } : { d: "d", h: "h", m: "min" };
  const parts: string[] = [];
  if (d) parts.push(`${d} ${u.d}`);
  if (h || d) parts.push(`${h} ${u.h}`);
  parts.push(`${m} ${u.m}`);
  return parts.join(" ");
}

/** Date from unix seconds -> "15 нояб. 2026, 21:04" */
export function formatDate(unixSec: number, lang: Lang = "ru"): string {
  if (!Number.isFinite(unixSec) || unixSec <= 0) return "—";
  return new Date(unixSec * 1000).toLocaleString(lang === "ru" ? "ru-RU" : "en-US", {
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
