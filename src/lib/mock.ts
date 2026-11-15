// Mock backend for running the UI in a normal browser (UI tests, design work).
// Mode is chosen by localStorage "sm-mock": "normal" (default) | "worst" | "empty" | "error".
import type {
  AppInfo,
  CleanupCategory,
  GameModeStatus,
  ProgramInfo,
  Settings,
  StartupItem,
} from "./types";

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
  update_check: () => ({ available: mode() === "worst" }),
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
    const ids = a.ids as CleanupCategory["id"][];
    const cats = cleanupCats();
    let freed = 0;
    const per: { id: CleanupCategory["id"]; freed: number }[] = [];
    for (const id of ids) {
      emit("cleanup://progress", { id, phase: "start" });
      await sleep(450);
      const c = cats.find((x) => x.id === id);
      const f = (c?.size ?? 0) * 0.93;
      freed += f;
      per.push({ id, freed: f });
      emit("cleanup://progress", { id, phase: "done" });
    }
    return { freed, deleted: 12345, skipped: 42, perCategory: per };
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
    await sleep(800);
    if (mode() === "empty") return [];
    return [
      { size: 2.1 * GB, files: ["C:\\Users\\Step\\Downloads\\movie.mkv", "C:\\Users\\Step\\Videos\\movie (1).mkv"] },
      { size: 48 * MB, files: ["C:\\Users\\Step\\Downloads\\setup.exe", "C:\\Users\\Step\\Desktop\\setup.exe", "C:\\Users\\Step\\Documents\\old\\setup.exe"] },
    ];
  },
  fs_recycle: (a) => ({ ok: (a.paths as string[]).length, failed: [] }),

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
    await sleep(2500);
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
    emit("speed://progress", { phase: "ping", value: 14 });
    await sleep(500);
    for (let i = 0; i < 8; i++) {
      emit("speed://progress", { phase: "download", value: 80 + i * 12 });
      await sleep(200);
    }
    for (let i = 0; i < 6; i++) {
      emit("speed://progress", { phase: "upload", value: 20 + i * 5 });
      await sleep(200);
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
