// Mock backend for running the UI in a normal browser (UI tests, design work).
// Mode is chosen by localStorage "sm-mock": "normal" (default) | "worst" | "empty" | "error".
import type {
  AppInfo,
  InstallState,
  WingetState,
  WingetHit,
  CleanupCategory,
  GameModeStatus,
  ProgramInfo,
  Settings,
  StartupItem,
  WallItem,
  WallOptions,
  WallState,
} from "./types";

const WALL_BUILTIN: WallItem[] = (
  [
    ["win10", "10", 2000, 1125],
    ["win7", "7", 1920, 1200],
    ["xp", "XP", 1920, 1200],
    ["win11", "11", 2000, 1250],
    ["cat", "Cat", 1672, 941],
    ["iceberg", "Iceberg", 1672, 941],
    ["pinky", "Pinky", 1672, 941],
    ["backrooms", "Backrooms", 1672, 941],
    ["vi", "VI", 1672, 941],
  ] as const
).map(([key, name, width, height]) => ({
  id: `b:${key}`,
  name,
  builtin: true,
  path: `/assets/wallpapers/${key}.jpg`,
  thumb: `/assets/wallpapers/${key}.thumb.jpg`,
  width,
  height,
}));
const wall = {
  mine: [] as WallItem[],
  current: null as string | null,
  changed: false,
  options: { fit: "fill", auto: false, interval: 60, shuffle: true, source: "all" } as WallOptions,
};
function wallState(changed = false): WallState {
  if (changed) wall.changed = true;
  return { builtin: WALL_BUILTIN, mine: [...wall.mine], current: wall.current, options: wall.options, canRestore: wall.changed };
}

type Mode = "normal" | "worst" | "empty" | "error";

function mode(): Mode {
  try {
    const m = localStorage.getItem("sm-mock");
    if (m === "worst" || m === "empty" || m === "error") return m;
  } catch {
    /* storage unavailable */
  }
  return "normal";
}

const GB = 1024 ** 3;
const MB = 1024 ** 2;
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/* "Stop" buttons: op_cancel raises a flag, the long mocks below notice it while waiting. */
const stopFlags: Record<string, boolean> = {};
const STOPPED = "Остановлено";
async function opSleep(op: string, ms: number) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (stopFlags[op]) throw STOPPED;
    await sleep(Math.min(100, end - Date.now()));
  }
  if (stopFlags[op]) throw STOPPED;
}

/* Journal: a few example entries, reverting marks them. */
const now = () => Math.floor(Date.now() / 1000);
let journal: { id: number; at: number; kind: string; target: string; detail: string; undo: { type: string } | null; reverted: boolean }[] = [
  { id: 6, at: now() - 600, kind: "dns", target: "", detail: "cloudflare", undo: { type: "dns" }, reverted: false },
  { id: 5, at: now() - 3600, kind: "service_mode", target: "SysMain", detail: "disabled", undo: { type: "serviceMode" }, reverted: false },
  { id: 4, at: now() - 7200, kind: "startup_off", target: "Discord", detail: "", undo: { type: "startup" }, reverted: true },
  { id: 3, at: now() - 86400, kind: "cleanup", target: "", detail: String(6.2 * 1024 ** 3), undo: null, reverted: false },
  { id: 2, at: now() - 86400 - 500, kind: "uninstall", target: "Avast Free Antivirus", detail: "", undo: null, reverted: false },
  { id: 1, at: now() - 86400 * 3, kind: "game_mode", target: "", detail: "", undo: { type: "gameMode" }, reverted: false },
];
const LONG = "Очень-очень длинное название программы которое не помещается в строку Ultimate Edition 2026 x64 (Portable Build)";

let settings: Settings = {
  language: "ru",
  autostart: false,
  startMinimized: false,
  closeToTray: true,
  motion: "full",
  hotkey: "Ctrl+Shift+Space",
  trayMonitor: true,
};

let game: GameModeStatus = { active: false, since: null, applied: [], isLaptop: true };
const startup: StartupItem[] = [
  { id: "hkcu:Discord", name: "Discord", command: "C:\\Users\\Step\\AppData\\Local\\Discord\\Update.exe --processStart Discord.exe", location: "HKCU Run", enabled: true, exePath: null },
  { id: "hkcu:Steam", name: "Steam", command: "\"C:\\Program Files (x86)\\Steam\\steam.exe\" -silent", location: "HKCU Run", enabled: true, exePath: null },
  { id: "hklm:SecurityHealth", name: "Windows Security notification icon", command: "%windir%\\system32\\SecurityHealthSystray.exe", location: "HKLM Run", enabled: true, exePath: null },
  { id: "folder:Telegram", name: "Telegram", command: "C:\\Users\\Step\\AppData\\Roaming\\Telegram Desktop\\Telegram.exe -autostart", location: "Startup folder", enabled: false, exePath: null },
];

const listeners = new Map<string, Set<(p: unknown) => void>>();
function emit(event: string, payload: unknown) {
  listeners.get(event)?.forEach((h) => h(payload));
}

function cleanupCats(): CleanupCategory[] {
  const m = mode();
  const base: [CleanupCategory["id"], number, number, boolean][] = [
    ["user_temp", 6.1 * GB, 18234, false],
    ["windows_temp", 1.2 * GB, 3200, true],
    ["browser_cache", 3.4 * GB, 52011, false],
    ["thumbnails", 410 * MB, 12, false],
    ["crash_dumps", 820 * MB, 7, false],
    ["error_reports", 64 * MB, 33, true],
    ["shader_cache", 1.1 * GB, 4211, false],
    ["update_cache", 2.8 * GB, 140, true],
    ["delivery_opt", 900 * MB, 61, true],
    ["recycle_bin", 1.2 * GB, 87, false],
  ];
  return base.map(([id, size, files, needsAdmin]) => ({
    id,
    size: m === "empty" ? 0 : m === "worst" ? size * 913 : size,
    files: m === "empty" ? 0 : m === "worst" ? files * 999 : files,
    needsAdmin,
    available: true,
  }));
}

function programs(): ProgramInfo[] {
  if (mode() === "empty") return [];
  const list: Omit<ProgramInfo, "icon">[] = [
    { id: "w:Google Chrome", name: "Google Chrome", publisher: "Google LLC", version: "142.0.7444.60", size: 612 * MB, installDate: "2026-09-12", kind: "win32", location: "C:\\Program Files\\Google\\Chrome\\Application", protected: false, canUninstall: true },
    { id: "w:Steam", name: "Steam", publisher: "Valve Corporation", version: "2.10.91.91", size: 1.4 * GB, installDate: "2025-03-02", kind: "win32", location: "C:\\Program Files (x86)\\Steam", protected: false, canUninstall: true },
    { id: "w:Discord", name: "Discord", publisher: "Discord Inc.", version: "1.0.9210", size: 380 * MB, installDate: "2026-10-01", kind: "win32", location: "C:\\Users\\Step\\AppData\\Local\\Discord", protected: false, canUninstall: true },
    { id: "w:WebView2", name: "Microsoft Edge WebView2 Runtime", publisher: "Microsoft Corporation", version: "142.0.3595.53", size: 640 * MB, installDate: "2026-11-01", kind: "win32", location: "", protected: true, canUninstall: false },
    { id: "s:Microsoft.WindowsCalculator", name: "Калькулятор", publisher: "Microsoft Corporation", version: "11.2502.2.0", size: 0, installDate: "", kind: "store", location: "", protected: false, canUninstall: true },
    { id: "p:C:\\Program Files\\OBS-Portable", name: "OBS-Portable", publisher: "", version: "", size: 512 * MB, installDate: "2026-06-11", kind: "portable", location: "C:\\Program Files\\OBS-Portable", protected: false, canUninstall: true },
  ];
  if (mode() === "worst") {
    for (let i = 0; i < 400; i++) {
      list.push({ id: `w:x${i}`, name: i % 3 === 0 ? LONG : `Программа ${i}`, publisher: i % 2 ? LONG : "", version: "1.0.0.0.0.0.0.0.0.0.1", size: i * 37 * GB, installDate: i % 4 ? "" : "1999-01-01", kind: i % 5 ? "win32" : "store", location: "C:\\" + "очень\\длинный\\путь\\".repeat(10), protected: i % 7 === 0, canUninstall: i % 7 !== 0 });
    }
  }
  // Every 7th entry has no icon, to show the initials fallback.
  return list.map((p, i) => ({ ...p, icon: i % 7 === 6 ? `C:\\${p.id}.none` : `C:\\${p.id}.exe` }));
}

let mockCancel = false;

const handlers: Record<string, (args: Record<string, unknown>) => unknown | Promise<unknown>> = {
  app_info: (): AppInfo => ({
    version: "1.0.0",
    isAdmin: mode() !== "worst",
    userName: mode() === "worst" ? "Константин-Александрович Длинноимённый" : "Step",
    exePath: "C:\\Users\\Step\\Desktop\\stepmaster.exe",
    dataDir: "C:\\Users\\Step\\AppData\\Local\\com.stepmaster.app",
    osName: "Windows 11 Pro",
    osBuild: "26200",
    isWin11: true,
  }),
  settings_get: () => ({ ...settings }),
  settings_set: (a) => {
    settings = { ...(a.settings as Settings) };
    return { ...settings };
  },
  relaunch_as_admin: () => undefined,
  open_tool: () => undefined,
  open_data_dir: () => undefined,
  open_app_dir: () => undefined,
  reveal_path: () => undefined,
  // Update window preview: localStorage.setItem("sm-update", "1") (or the "worst" mode).
  update_check: () => ({ available: mode() === "worst" || localStorage.getItem("sm-update") === "1" }),
  // The real release files, so this window always shows what the next release will show.
  update_notes: async () => {
    const [{ default: md }, { default: png }] = await Promise.all([
      import("../../scripts/release-notes.md?raw"),
      import("../../scripts/banner.png"),
    ]);
    const sections: { title: string; items: string[] }[] = [];
    for (const line of md.split(/\r?\n---\r?\n/)[0].split(/\r?\n/)) {
      if (line.startsWith("## ")) sections.push({ title: line.slice(3).trim(), items: [] });
      else if (line.startsWith("- ") && sections.length) sections[sections.length - 1].items.push(line.slice(2).trim());
    }
    return { image: localStorage.getItem("sm-update-banner") ?? png, sections };
  },
  whats_new: async () =>
    localStorage.getItem("sm-whatsnew") === "1" ? { version: "1.0.7", notes: await (handlers.update_notes as () => Promise<unknown>)() } : null,
  whats_new_seen: () => localStorage.removeItem("sm-whatsnew"),
  update_install: async () => {
    for (let i = 0; i <= 100; i += 10) {
      emit("update://progress", { percent: i });
      await sleep(120);
    }
  },
  show_main_window: () => undefined,
  icons_get: (a) => {
    // A simple drawn icon so the UI can be checked in a browser.
    const svg = (c: string) =>
      "data:image/svg+xml;base64," +
      btoa(`<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect x="6" y="6" width="52" height="52" rx="14" fill="${c}"/></svg>`);
    const colors = ["#5865F2", "#1DB954", "#E34F26", "#F7DF1E", "#0A84FF"];
    return Object.fromEntries((a.specs as string[]).map((s, i) => [s, s.endsWith(".none") ? null : svg(colors[(s.length + i) % colors.length])]));
  },
  quit_app: () => undefined,
  // Installer screen in the browser: localStorage.setItem('sm-install', 'setup' | 'uninstall')
  wallpaper_state: () => wallState(),
  wallpaper_view: async (a) => {
    await sleep(300);
    return [...wall.mine, ...WALL_BUILTIN].find((x) => x.id === a.id)?.path ?? "";
  },
  wallpaper_set: async (a) => {
    await sleep(500);
    wall.current = String(a.id);
    return wallState(true);
  },
  wallpaper_next: async () => {
    await sleep(400);
    const all = [...wall.mine, ...WALL_BUILTIN];
    const i = all.findIndex((x) => x.id === wall.current);
    wall.current = all[(i + 1) % all.length].id;
    return wallState(true);
  },
  wallpaper_options: (a) => {
    wall.options = a.options as WallOptions;
    return wallState();
  },
  wallpaper_add: async (a) => {
    await sleep(600);
    const paths = a.paths as string[];
    for (const p of paths) {
      const src = WALL_BUILTIN[wall.mine.length % WALL_BUILTIN.length];
      const name = (p.split(/[\\/]/).pop() ?? p).replace(/\.[^.]+$/, "");
      wall.mine.unshift({ ...src, id: `u:${Date.now()}${wall.mine.length}.jpg`, name, builtin: false });
    }
    return { state: wallState(), added: paths.length, failed: [] };
  },
  wallpaper_rename: (a) => {
    wall.mine = wall.mine.map((x) => (x.id === a.id ? { ...x, name: String(a.name).trim().slice(0, 60) } : x));
    return wallState();
  },
  wallpaper_remove: (a) => {
    wall.mine = wall.mine.filter((x) => x.id !== a.id);
    return wallState();
  },
  wallpaper_restore: async () => {
    await sleep(400);
    wall.current = null;
    wall.changed = false;
    wall.options = { ...wall.options, auto: false };
    return wallState();
  },
  winget_state: (): WingetState => ({
    available: mode() !== "empty",
    version: "v1.29.380",
    installed: ["Valve.Steam", "VideoLAN.VLC", "7zip.7zip", "Microsoft.VCRedist.2015+.x64"],
  }),
  winget_setup: async (): Promise<WingetState> => {
    await new Promise((r) => setTimeout(r, 8000));
    return { available: true, version: "v1.29.380", installed: [] };
  },
  winget_search: (a): WingetHit[] =>
    String(a.query).length < 2
      ? []
      : [
          { name: `${a.query} Pro`, id: "Example.ProTool", version: "2.4.1" },
          { name: "Notepad++", id: "Notepad++.Notepad++", version: "8.8.1" },
          { name: mode() === "worst" ? "Очень-очень длинное название программы, которое не помещается" : "Tiny", id: "Some.Very.Long.Package.Identifier.That.Goes.On", version: "1.0.0.0.0.1" },
        ],
  open_site: () => undefined,
  winget_cancel: () => {
    mockCancel = true;
  },
  winget_install: (a) => {
    const ids = a.ids as string[];
    mockCancel = false;
    void (async () => {
      for (const id of ids) emit("winget://progress", { id, state: "queued", note: null });
      for (const [i, id] of ids.entries()) {
        if (mockCancel) {
          emit("winget://progress", { id, state: "failed", note: "cancelled" });
          continue;
        }
        emit("winget://progress", { id, state: "running", note: null, percent: null });
        for (let p = 0; p <= 100; p += 20) {
          if (mockCancel) break;
          emit("winget://progress", { id, state: "running", note: null, percent: p, done: (p / 100) * 85_000_000, total: 85_000_000 });
          await sleep(250);
        }
        if (mockCancel) {
          emit("winget://progress", { id, state: "failed", note: "cancelled" });
          continue;
        }
        await sleep(500);
        const fail = mode() === "worst" && i % 2 === 1;
        emit("winget://progress", { id, state: fail ? "failed" : "done", note: fail ? "Ошибка установщика 0x8A150014 (тест)" : i === 2 ? "reboot" : null });
      }
      emit("winget://finished", null);
    })();
  },
  install_state: (): InstallState => {
    let m: InstallState["mode"] = "normal";
    try {
      const v = localStorage.getItem("sm-install");
      if (v === "setup" || v === "uninstall") m = v;
    } catch {
      /* storage unavailable */
    }
    return { mode: m, defaultDir: "C:\\Program Files\\StepMaster", alreadyInstalled: null };
  },
  install_run: async () => {
    for (const step of ["copy", "register", "shortcuts", "launch"]) {
      emit("install://progress", { step });
      await sleep(600);
    }
    if (mode() === "worst") throw "Отказано в доступе (тест)";
  },
  uninstall_run: async () => {
    await sleep(1200);
  },

  system_snapshot: () => {
    const m = mode();
    return {
      cpu: m === "worst" ? 100 : 12 + Math.random() * 20,
      cpuFreqMhz: 3600,
      memUsed: (m === "worst" ? 15.9 : 8.4 + Math.random()) * GB,
      memTotal: 16 * GB,
      disks:
        m === "empty"
          ? []
          : [
              { mount: "C:\\", label: "Windows", total: 465.8 * GB, available: (m === "worst" ? 0.2 : 37.2) * GB, kind: "SSD", removable: false, fs: "NTFS" },
              { mount: "D:\\", label: m === "worst" ? LONG : "Data", total: 931.5 * GB, available: 402 * GB, kind: "HDD", removable: false, fs: "NTFS" },
            ],
    };
  },
  system_overview: () => ({
    cpuName: "AMD Ryzen 7 5800H with Radeon Graphics",
    cores: 8,
    threads: 16,
    memTotal: 16 * GB,
    gpus: ["NVIDIA GeForce RTX 3060 Laptop GPU", "AMD Radeon(TM) Graphics"],
    osName: "Windows 11 Pro",
    osVersion: "25H2",
    osBuild: "26200.7171",
    hostName: "STEP-PC",
    uptimeSec: 93784,
    board: "ASUSTeK COMPUTER INC. ROG Strix G513QM",
    battery: { percent: 84, charging: true, designCapacity: 90000, fullCapacity: 81200 },
  }),

  cleanup_scan: async () => {
    await sleep(900);
    return cleanupCats();
  },
  cleanup_run: async (a) => {
    stopFlags.cleanup = false;
    const ids = a.ids as CleanupCategory["id"][];
    const cats = cleanupCats();
    let freed = 0;
    let stopped = false;
    const per: { id: CleanupCategory["id"]; freed: number }[] = [];
    for (const id of ids) {
      if (stopFlags.cleanup) {
        stopped = true;
        break;
      }
      emit("cleanup://progress", { id, phase: "start" });
      await sleep(450);
      const c = cats.find((x) => x.id === id);
      const f = (c?.size ?? 0) * 0.93;
      freed += f;
      per.push({ id, freed: f });
      emit("cleanup://progress", { id, phase: "done" });
    }
    return { stopped, freed, deleted: 12345, skipped: 42, perCategory: per };
  },
  history_get: () => ({ totalFreed: mode() === "empty" ? 0 : 48.3 * GB, runs: [{ at: Date.now() / 1000 - 86400 * 3, freed: 6.2 * GB }] }),

  disk_scan: async (a) => {
    for (let i = 1; i <= 8; i++) {
      emit("disk://progress", { files: i * 41234, bytes: i * 31 * GB, current: `${a.path}Users\\Step\\AppData\\Local\\Packages\\${i}` });
      await sleep(180);
    }
    return handlers.disk_children(a);
  },
  disk_children: (a) => {
    const p = String(a.path);
    const sep = p.endsWith("\\") ? "" : "\\";
    const names: [string, number, boolean][] = [
      ["Users", 182 * GB, true],
      ["Windows", 38.4 * GB, true],
      ["Program Files", 64.1 * GB, true],
      ["Program Files (x86)", 22.7 * GB, true],
      ["ProgramData", 14.3 * GB, true],
      ["hiberfil.sys", 12.7 * GB, false],
      ["pagefile.sys", 9.5 * GB, false],
      [mode() === "worst" ? LONG : "Games", 96.2 * GB, true],
    ];
    const entries = names.map(([n, s, d]) => ({ name: n, path: p + sep + n, size: s, isDir: d, files: d ? 12034 : 1 }));
    return { path: p, size: entries.reduce((x, e) => x + e.size, 0), files: 412345, entries: mode() === "empty" ? [] : entries };
  },
  disk_large_files: () =>
    mode() === "empty"
      ? []
      : [
          { name: "Cyberpunk2077.zip", path: "C:\\Users\\Step\\Downloads\\Cyberpunk2077.zip", size: 64.2 * GB, isDir: false, files: 1 },
          { name: "hiberfil.sys", path: "C:\\hiberfil.sys", size: 12.7 * GB, isDir: false, files: 1 },
          { name: "Windows11.iso", path: "C:\\Users\\Step\\Downloads\\Windows11.iso", size: 6.1 * GB, isDir: false, files: 1 },
          { name: "record_2026-10-11.mp4", path: "C:\\Users\\Step\\Videos\\record_2026-10-11.mp4", size: 3.8 * GB, isDir: false, files: 1 },
        ],
  disk_cancel: () => undefined,
  duplicates_scan: async () => {
    stopFlags.dupes = false;
    await opSleep("dupes", 2500);
    if (mode() === "empty") return [];
    return [
      { size: 2.1 * GB, files: ["C:\\Users\\Step\\Downloads\\movie.mkv", "C:\\Users\\Step\\Videos\\movie (1).mkv"] },
      { size: 48 * MB, files: ["C:\\Users\\Step\\Downloads\\setup.exe", "C:\\Users\\Step\\Desktop\\setup.exe", "C:\\Users\\Step\\Documents\\old\\setup.exe"] },
    ];
  },
  fs_recycle: (a) => ({ ok: (a.paths as string[]).length, failed: [] }),
  downloads_scan: async (a) => {
    await sleep(700);
    if (mode() === "empty") return { root: "C:\\Users\\Step\\Downloads", total: 0, items: [] };
    const base = a.place === "desktop" ? "C:\\Users\\Step\\Desktop" : "C:\\Users\\Step\\Downloads";
    const mk = (name: string, size: number, category: string, ageDays: number, flags: string[], installedAs: string | null = null, isDir = false) => ({
      path: `${base}\\${name}`,
      name,
      size,
      isDir,
      category,
      ageDays,
      flags,
      installedAs,
    });
    const items = [
      mk("Windows11.iso", 6.1 * GB, "images", 210, ["old"]),
      mk("Cyberpunk2077.zip", 64.2 * GB, "archives", 30, ["extracted"]),
      mk("Cyberpunk2077", 70.1 * GB, "folders", 30, [], null, true),
      mk("DiscordSetup.exe", 110 * MB, "installers", 12, ["installed"], "Discord"),
      mk("ChromeSetup.exe", 1.4 * MB, "installers", 400, ["installed", "old"], "Google Chrome"),
      mk("movie.mkv", 2.1 * GB, "videos", 5, []),
      mk("movie (1).mkv", 2.1 * GB, "videos", 4, ["duplicate"]),
      mk("report.pdf", 3 * MB, "documents", 2, []),
      mk("photo.jpg", 4 * MB, "pictures", 120, ["old"]),
      mk("big_file.zip.crdownload", 900 * MB, "partial", 9, ["partial"]),
      mk(mode() === "worst" ? LONG + ".exe" : "game_setup.exe", 2.3 * GB, "installers", 1, []),
    ];
    return { root: base, total: items.reduce((s, i) => s + i.size, 0), items };
  },
  diag_last: () => null,
  diag_run: async (a) => {
    stopFlags.diag = false;
    const steps = a.defenderScan ? ["defender", "system", "performance", "evaluate"] : ["system", "performance", "evaluate"];
    for (let i = 0; i < steps.length; i++) {
      emit("diag://progress", { step: steps[i], done: i, total: steps.length });
      await opSleep("diag", steps[i] === "defender" ? 1800 : 700);
    }
    emit("diag://progress", { step: "done", done: steps.length, total: steps.length });
    if (mode() === "empty") {
      return { at: Date.now() / 1000, durationMs: 2100, score: 100, admin: true, defenderScanned: Boolean(a.defenderScan), issues: [], checks: [{ code: "cpu_load", category: "performance", status: "ok" }] };
    }
    const issues = [
      { code: "threats", category: "security", severity: "critical", params: { n: "1" }, details: ["Trojan:Win32/Wacatac.B!ml — C:\\Users\\Step\\Downloads\\free_game.exe"], fix: "remove_threats", fixArgs: [], open: "tool:defender" },
      { code: "pua", category: "security", severity: "medium", params: { n: "1" }, details: ["PUA:Win32/Packunwan — C:\\Program Files\\Tool\\tool.exe"], fix: null, fixArgs: [], open: "tool:defender" },
      { code: "disk_space", category: "performance", severity: "high", params: { drive: "C", free: "18.2", pct: "4" }, details: [], fix: "cleanup_safe", fixArgs: [], open: "nav:disk" },
      { code: "firewall", category: "security", severity: "high", params: {}, details: ["Public"], fix: "enable_firewall", fixArgs: [], open: "nav:protection" },
      { code: "startup_suspicious", category: "security", severity: "medium", params: { n: "1" }, details: ["updater — temp"], fix: "disable_startup", fixArgs: ["hkcu|updater"], open: "nav:system:startup" },
      { code: "update_errors", category: "system", severity: "medium", params: { n: "43", last: "2026-11-12 15:06" }, details: [], fix: "repair_system", fixArgs: [], open: "tool:updates" },
      { code: "shutdowns", category: "system", severity: "medium", params: { n: "3", last: "2026-11-07 20:46" }, details: [], fix: null, fixArgs: [], open: null },
      { code: "startup", category: "performance", severity: "low", params: { n: "15" }, details: [], fix: null, fixArgs: [], open: "nav:system:startup" },
      { code: "uptime", category: "performance", severity: "info", params: { days: "9" }, details: [], fix: null, fixArgs: [], open: null },
    ];
    const codes = ["cpu_load", "memory", "junk", "power_plan", "reboot_pending", "cpu_temp", "gpu_temp", "throttling", "disk_health", "devices", "hw_errors", "bsod", "disk_errors", "app_crashes", "wu_service", "uac", "defender_off", "signatures", "last_scan", "exclusions", "processes_suspicious", "tasks_suspicious", "hosts", "proxy"];
    const cat = (c: string) => (["cpu_temp", "gpu_temp", "throttling"].includes(c) ? "thermal" : ["disk_health", "devices", "hw_errors"].includes(c) ? "hardware" : ["bsod", "disk_errors", "app_crashes", "wu_service", "uac"].includes(c) ? "system" : ["defender_off", "signatures", "last_scan", "exclusions", "processes_suspicious", "tasks_suspicious", "hosts", "proxy"].includes(c) ? "security" : "performance");
    const checks = [
      ...issues.map((i) => ({ code: i.code, category: i.category, status: "issue" })),
      ...codes.map((c) => ({ code: c, category: cat(c), status: c === "cpu_temp" ? "skipped" : "ok" })),
    ];
    return { at: Date.now() / 1000, durationMs: 6400, score: 37, admin: mode() !== "worst", defenderScanned: Boolean(a.defenderScan), issues, checks };
  },
  diag_fix: async (a) => {
    await sleep(a.fix === "repair_system" ? 2500 : 900);
    return { ok: true, note: a.fix === "enable_uac" || a.fix === "repair_system" ? "reboot" : "" };
  },

  startup_list: () => (mode() === "empty" ? [] : startup.map((s) => ({ ...s }))),
  startup_set: (a) => {
    const s = startup.find((x) => x.id === a.id);
    if (s) s.enabled = Boolean(a.enabled);
  },
  processes_list: () =>
    Array.from({ length: mode() === "worst" ? 300 : 24 }, (_, i) => ({
      pid: 1000 + i * 4,
      name: i === 0 ? "chrome.exe" : i === 1 ? "Discord.exe" : i % 9 === 0 && mode() === "worst" ? LONG + ".exe" : `process_${i}.exe`,
      cpu: Math.max(0, 14 - i * 0.6),
      memory: (900 - i * 30) * MB,
      exe: i % 5 === 4 ? null : `C:\\Apps\\proc${i}.exe`,
    })),
  process_kill: () => undefined,
  services_list: () => [
    { name: "wuauserv", displayName: "Центр обновления Windows", status: "Running", startMode: "Manual", description: "Обеспечивает обнаружение, загрузку и установку обновлений", critical: false },
    { name: "Spooler", displayName: "Диспетчер печати", status: "Running", startMode: "Auto", description: "Ставит в очередь задания печати", critical: false },
    { name: "SysMain", displayName: "SysMain", status: "Running", startMode: "Auto", description: "Поддерживает и улучшает производительность системы", critical: false },
    { name: "RpcSs", displayName: "Удалённый вызов процедур (RPC)", status: "Running", startMode: "Auto", description: "", critical: true },
    { name: "DiagTrack", displayName: "Функциональные возможности для подключённых пользователей и телеметрия", status: "Stopped", startMode: "Disabled", description: "", critical: false },
  ],
  service_action: () => undefined,
  service_start_mode: () => undefined,
  restore_status: () => ({ enabled: true, points: [{ sequence: 41, description: "StepMaster: перед игровым режимом", createdAt: "2026-11-10 21:04" }] }),
  restore_create: async () => {
    await sleep(1500);
    return "created";
  },
  restore_enable: () => undefined,

  programs_list: async () => {
    await sleep(500);
    return programs();
  },
  program_uninstall: async () => {
    await sleep(1200);
    return { finished: true, leftovers: ["C:\\Users\\Step\\AppData\\Roaming\\Discord", "C:\\Users\\Step\\AppData\\Local\\Discord"] };
  },

  protection_status: () => ({
    defender: { available: true, antivirusEnabled: true, realtime: mode() !== "worst", signatureUpdated: "2026-11-15 08:12", signatureAgeDays: mode() === "worst" ? 40 : 0, lastQuickScan: "2026-11-14 03:00", threats: mode() === "worst" ? 3 : 0 },
    otherAntivirus: mode() === "worst" ? ["Kaspersky Free"] : [],
    firewall: [
      { name: "Domain", enabled: true },
      { name: "Private", enabled: true },
      { name: "Public", enabled: mode() !== "worst" },
    ],
  }),
  defender_quick_scan: async () => {
    stopFlags.defender = false;
    await opSleep("defender", 4000);
  },
  op_cancel: (a) => {
    stopFlags[a.op as string] = true;
  },
  journal_get: () => (mode() === "empty" ? [] : journal),
  journal_revert: async (a) => {
    await sleep(500);
    journal = journal.map((e) => (e.id === a.id ? { ...e, reverted: true } : e));
    return journal;
  },
  defender_update: async () => {
    await sleep(1500);
  },
  firewall_set: () => undefined,
  updates_history: () => [
    { id: "KB5048685", description: "Update", installedOn: "2026-11-12" },
    { id: "KB5047621", description: "Security Update", installedOn: "2026-10-14" },
  ],

  network_adapters: () => [
    { name: "Wi-Fi", description: "Intel(R) Wi-Fi 6 AX200 160MHz", status: "Up", kind: "wifi", mac: "A4-B1-C1-11-22-33", ipv4: ["192.168.1.34"], gateway: "192.168.1.1", dns: ["192.168.1.1"], linkSpeed: "866.7 Mbps" },
    { name: "Ethernet", description: "Realtek PCIe GbE Family Controller", status: "Disconnected", kind: "ethernet", mac: "00-1A-2B-3C-4D-5E", ipv4: [], gateway: "", dns: [], linkSpeed: "0 bps" },
  ],
  dns_flush: () => undefined,
  dns_set: () => undefined,
  network_reset: () => undefined,
  speed_test: async () => {
    stopFlags.speed = false;
    emit("speed://progress", { phase: "ping", value: 14 });
    await opSleep("speed", 500);
    for (let i = 0; i < 8; i++) {
      emit("speed://progress", { phase: "download", value: 80 + i * 12 });
      await opSleep("speed", 200);
    }
    for (let i = 0; i < 6; i++) {
      emit("speed://progress", { phase: "upload", value: 20 + i * 5 });
      await opSleep("speed", 200);
    }
    return { pingMs: 14, downMbps: 164.2, upMbps: 45.1 };
  },
  connections_list: () => [
    { process: "chrome.exe", pid: 1000, local: "192.168.1.34:51234", remote: "142.250.74.14:443", state: "Established" },
    { process: "Discord.exe", pid: 1004, local: "192.168.1.34:51240", remote: "162.159.135.232:443", state: "Established" },
  ],
  wifi_profiles: () => [
    { ssid: "Home_5G", password: "super-secret-123", auth: "WPA2PSK" },
    { ssid: "Cafe Free WiFi", password: null, auth: "open" },
  ],

  game_status: () => ({ ...game }),
  game_apply: async () => {
    await sleep(900);
    game = { active: true, since: Date.now() / 1000, applied: ["powerPlan", "gameMode", "visualEffects", "pauseUpdates"], isLaptop: true };
    return { ...game };
  },
  game_revert: async () => {
    await sleep(600);
    game = { active: false, since: null, applied: [], isLaptop: true };
    return { ...game };
  },
  game_candidates: () => [
    { pid: 1000, name: "chrome.exe", cpu: 6.2, memory: 1.8 * GB, exe: null },
    { pid: 1004, name: "Discord.exe", cpu: 1.1, memory: 420 * MB, exe: null },
    { pid: 1008, name: "OneDrive.exe", cpu: 0.4, memory: 160 * MB, exe: null },
  ],
  gpu_prefs: () => [{ exe: "C:\\Games\\Valorant\\VALORANT.exe", pref: 2 }],
  gpu_pref_set: () => undefined,
  gpu_pref_remove: () => undefined,
};

export const mock = {
  async call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    await sleep(120);
    if (mode() === "error" && cmd !== "app_info" && cmd !== "settings_get") {
      throw `Ошибка (тест): команда ${cmd} не выполнена`;
    }
    const h = handlers[cmd];
    if (!h) throw `Нет мок-обработчика для ${cmd}`;
    return (await h(args ?? {})) as T;
  },
  on(event: string, handler: (p: unknown) => void): () => void {
    if (!listeners.has(event)) listeners.set(event, new Set());
    listeners.get(event)!.add(handler);
    return () => listeners.get(event)?.delete(handler);
  },
};
