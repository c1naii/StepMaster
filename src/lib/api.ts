// Typed bridge to the Rust backend. In a plain browser (dev/testing) it falls back to mock data.
import type {
  AppInfo,
  DiagReport,
  DownloadsReport,
  FixResult,
  CleanupCategory,
  CleanupHistory,
  CleanupId,
  CleanupResult,
  Connection,
  DiskFolder,
  DiskEntry,
  DnsPreset,
  DuplicateGroup,
  GameModeOptions,
  GameModeStatus,
  GpuPref,
  HotFix,
  NetAdapter,
  ProcessInfo,
  ProgramInfo,
  ProtectionStatus,
  RecycleResult,
  RestoreStatus,
  ServiceInfo,
  Settings,
  SpeedResult,
  StartupItem,
  SystemOverview,
  SystemSnapshot,
  UninstallResult,
  UpdateCheck,
  InstallState,
  WifiProfile,
} from "./types";
import { mock } from "./mock";

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<T>(cmd, args);
  }
  return mock.call<T>(cmd, args);
}

/** Subscribe to a backend event. Returns an unsubscribe function. */
export async function on<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
  if (isTauri) {
    const { listen } = await import("@tauri-apps/api/event");
    return listen<T>(event, (e) => handler(e.payload));
  }
  return mock.on(event, handler as (p: unknown) => void);
}

/** Converts backend errors (strings) into readable messages. */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}

export const api = {
  // app
  appInfo: () => call<AppInfo>("app_info"),
  settingsGet: () => call<Settings>("settings_get"),
  settingsSet: (settings: Settings) => call<Settings>("settings_set", { settings }),
  relaunchAsAdmin: () => call<void>("relaunch_as_admin"),
  openTool: (tool: string) => call<void>("open_tool", { tool }),
  openDataDir: () => call<void>("open_data_dir"),
  openAppDir: () => call<void>("open_app_dir"),
  reveal: (path: string) => call<void>("reveal_path", { path }),
  updateCheck: () => call<UpdateCheck>("update_check"),
  updateInstall: () => call<void>("update_install"),
  icons: (specs: string[]) => call<Record<string, string | null>>("icons_get", { specs }),
  showWindow: () => call<void>("show_main_window"),
  quit: () => call<void>("quit_app"),
  installState: () => call<InstallState>("install_state"),
  installRun: (dir: string, desktop: boolean, startMenu: boolean) => call<void>("install_run", { dir, desktop, startMenu }),
  uninstallRun: (removeData: boolean) => call<void>("uninstall_run", { removeData }),

  // system
  snapshot: () => call<SystemSnapshot>("system_snapshot"),
  overview: () => call<SystemOverview>("system_overview"),

  // cleanup
  cleanupScan: () => call<CleanupCategory[]>("cleanup_scan"),
  cleanupRun: (ids: CleanupId[]) => call<CleanupResult>("cleanup_run", { ids }),
  history: () => call<CleanupHistory>("history_get"),

  // disk
  diskScan: (path: string) => call<DiskFolder>("disk_scan", { path }),
  diskChildren: (path: string) => call<DiskFolder>("disk_children", { path }),
  diskLargeFiles: () => call<DiskEntry[]>("disk_large_files"),
  diskCancel: () => call<void>("disk_cancel"),
  duplicatesScan: (path: string) => call<DuplicateGroup[]>("duplicates_scan", { path }),
  recycle: (paths: string[]) => call<RecycleResult>("fs_recycle", { paths }),
  downloadsScan: (place: "downloads" | "desktop") => call<DownloadsReport>("downloads_scan", { place }),

  // diagnostics
  diagRun: (defenderScan: boolean) => call<DiagReport>("diag_run", { defenderScan }),
  diagLast: () => call<DiagReport | null>("diag_last"),
  diagFix: (fix: string, args: string[]) => call<FixResult>("diag_fix", { fix, args }),

  // system tools
  startupList: () => call<StartupItem[]>("startup_list"),
  startupSet: (id: string, enabled: boolean) => call<void>("startup_set", { id, enabled }),
  processes: () => call<ProcessInfo[]>("processes_list"),
  processKill: (pid: number) => call<void>("process_kill", { pid }),
  services: () => call<ServiceInfo[]>("services_list"),
  serviceAction: (name: string, action: "start" | "stop" | "restart") =>
    call<void>("service_action", { name, action }),
  serviceStartMode: (name: string, mode: "auto" | "manual" | "disabled") =>
    call<void>("service_start_mode", { name, mode }),
  restoreStatus: () => call<RestoreStatus>("restore_status"),
  restoreCreate: (description: string) => call<string>("restore_create", { description }),
  restoreEnable: () => call<void>("restore_enable"),

  // programs
  programs: () => call<ProgramInfo[]>("programs_list"),
  programUninstall: (id: string) => call<UninstallResult>("program_uninstall", { id }),

  // protection
  protection: () => call<ProtectionStatus>("protection_status"),
  defenderQuickScan: () => call<void>("defender_quick_scan"),
  defenderUpdate: () => call<void>("defender_update"),
  firewallSet: (enabled: boolean) => call<void>("firewall_set", { enabled }),
  hotfixes: () => call<HotFix[]>("updates_history"),

  // network
  adapters: () => call<NetAdapter[]>("network_adapters"),
  dnsFlush: () => call<void>("dns_flush"),
  dnsSet: (preset: DnsPreset) => call<void>("dns_set", { preset }),
  networkReset: () => call<void>("network_reset"),
  speedTest: () => call<SpeedResult>("speed_test"),
  connections: () => call<Connection[]>("connections_list"),
  wifiProfiles: () => call<WifiProfile[]>("wifi_profiles"),

  // games
  gameStatus: () => call<GameModeStatus>("game_status"),
  gameApply: (options: GameModeOptions) => call<GameModeStatus>("game_apply", { options }),
  gameRevert: () => call<GameModeStatus>("game_revert"),
  gameCandidates: () => call<ProcessInfo[]>("game_candidates"),
  gpuPrefs: () => call<GpuPref[]>("gpu_prefs"),
  gpuPrefSet: (exe: string, pref: number) => call<void>("gpu_pref_set", { exe, pref }),
  gpuPrefRemove: (exe: string) => call<void>("gpu_pref_remove", { exe }),
};

/** Opens a native file/folder picker. Returns null if cancelled or unavailable. */
export async function pickPath(opts: { directory: boolean; filters?: { name: string; extensions: string[] }[] }) {
  if (!isTauri) return opts.directory ? "C:\\Users\\Step\\Downloads" : "C:\\Games\\Game\\game.exe";
  const { open } = await import("@tauri-apps/plugin-dialog");
  const res = await open({ directory: opts.directory, multiple: false, filters: opts.filters });
  return typeof res === "string" ? res : null;
}

/** Window controls (no-ops in browser). */
export const win = {
  async minimize() {
    if (!isTauri) return;
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().minimize();
  },
  async toggleMaximize() {
    if (!isTauri) return;
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().toggleMaximize();
  },
  async close() {
    if (!isTauri) return;
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().close();
  },
  async startDrag() {
    if (!isTauri) return;
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().startDragging();
  },
};
