import { describe, expect, it } from "vitest";
import { baseName, clamp, formatBytes, formatDuration, greetingKey, initials, parentPath, pct, plural } from "./format";

describe("formatBytes", () => {
  it("handles zero, negative and NaN", () => {
    expect(formatBytes(0)).toBe("0 Б");
    expect(formatBytes(-5)).toBe("0 Б");
    expect(formatBytes(Number.NaN)).toBe("0 Б");
    expect(formatBytes(Number.POSITIVE_INFINITY)).toBe("0 Б");
  });
  it("formats units in ru and en", () => {
    expect(formatBytes(512)).toBe("512 Б");
    expect(formatBytes(1536)).toBe("1,5 КБ");
    expect(formatBytes(14.2 * 1024 ** 3)).toBe("14,2 ГБ");
    expect(formatBytes(14.2 * 1024 ** 3, "en")).toBe("14.2 GB");
    expect(formatBytes(465.8 * 1024 ** 3)).toBe("466 ГБ");
  });
  it("caps at TB", () => {
    expect(formatBytes(5 * 1024 ** 5)).toBe("5120 ТБ");
  });
});

describe("pct / clamp", () => {
  it("clamps and guards", () => {
    expect(pct(50, 100)).toBe(50);
    expect(pct(150, 100)).toBe(100);
    expect(pct(1, 0)).toBe(0);
    expect(pct(Number.NaN, 10)).toBe(0);
    expect(clamp(Number.NaN, 0, 1)).toBe(0);
    expect(clamp(5, 0, 1)).toBe(1);
  });
});

describe("formatDuration", () => {
  it("formats", () => {
    expect(formatDuration(59)).toBe("0 мин");
    expect(formatDuration(3700)).toBe("1 ч 1 мин");
    expect(formatDuration(93784)).toBe("1 д 2 ч 3 мин");
    expect(formatDuration(-1)).toBe("0 мин");
  });
});

describe("greetingKey", () => {
  it("maps hours", () => {
    expect(greetingKey(6)).toBe("morning");
    expect(greetingKey(13)).toBe("day");
    expect(greetingKey(20)).toBe("evening");
    expect(greetingKey(2)).toBe("night");
  });
});

describe("plural", () => {
  const f: [string, string, string] = ["файл", "файла", "файлов"];
  it("russian rules", () => {
    expect(plural(1, f)).toBe("файл");
    expect(plural(3, f)).toBe("файла");
    expect(plural(5, f)).toBe("файлов");
    expect(plural(11, f)).toBe("файлов");
    expect(plural(21, f)).toBe("файл");
    expect(plural(112, f)).toBe("файлов");
  });
});

describe("paths", () => {
  it("baseName / parentPath", () => {
    expect(baseName("C:\\Users\\Step\\")).toBe("Step");
    expect(baseName("C:\\")).toBe("C:");
    expect(parentPath("C:\\Users\\Step")).toBe("C:\\Users");
    expect(parentPath("C:\\Users")).toBe("C:\\");
    expect(parentPath("C:\\")).toBeNull();
  });
  it("initials", () => {
    expect(initials("Google Chrome")).toBe("GC");
    expect(initials("steam")).toBe("ST");
    expect(initials("")).toBe("?");
    expect(initials("(x86) — !!")).toBe("X8");
  });
});
