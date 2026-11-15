// Shared types between UI and the Rust backend (serde camelCase).

export type Lang = "ru" | "en";
export type MotionLevel = "full" | "reduced" | "off";

export interface Settings {
  language: Lang;
  autostart: boolean;
  startMinimized: boolean;
  closeToTray: boolean;
  motion: MotionLevel;
  hotkey: string; // e.g. "Ctrl+Shift+Space" or "" (off)
  trayMonitor: boolean;
}

export interface AppInfo {
  version: string;
  isAdmin: boolean;
  userName: string;
  exePath: string;
  dataDir: string;
  osName: string;
  osBuild: string;
  isWin11: boolean;
}

export interface DiskInfo {
  mount: string;
  label: string;
  total: number;
  available: number;
  kind: "SSD" | "HDD" | "Unknown";
  removable: boolean;
  fs: string;
}

export interface SystemSnapshot {
  cpu: number;
  cpuFreqMhz: number;
  memUsed: number;
  memTotal: number;
  disks: DiskInfo[];
}

export interface BatteryInfo {
  percent: number;
  charging: boolean;
  designCapacity: number | null;
  fullCapacity: number | null;
}

export interface SystemOverview {
  cpuName: string;
  cores: number;
  threads: number;
  memTotal: number;
  gpus: string[];
  osName: string;
  osVersion: string;
  osBuild: string;
  hostName: string;
  uptimeSec: number;
  board: string;
  battery: BatteryInfo | null;
}

export type CleanupId =
  | "user_temp"
  | "windows_temp"
  | "browser_cache"
  | "thumbnails"
  | "crash_dumps"
  | "error_reports"
  | "shader_cache"
  | "update_cache"
  | "delivery_opt"
  | "recycle_bin";

export interface CleanupCategory {
  id: CleanupId;
  size: number;
  files: number;
  needsAdmin: boolean;
  available: boolean;
}

export interface CleanupResult {
  freed: number;
  deleted: number;
  skipped: number;
  perCategory: { id: CleanupId; freed: number }[];
}

export interface CleanupProgress {
  id: CleanupId;
  phase: "start" | "done";
}

export interface HistoryEntry {
  at: number; // unix seconds
  freed: number;
}

export interface CleanupHistory {
  totalFreed: number;
  runs: HistoryEntry[];
}

export interface DiskEntry {
  name: string;
  path: string;
  size: number;
  isDir: boolean;
  files: number;
}

export interface DiskFolder {
  path: string;
  size: number;
  files: number;
  entries: DiskEntry[];
}

export interface ScanProgress {
  files: number;
  bytes: number;
  current: string;
}

export interface DuplicateGroup {
  size: number;
  files: string[];
}

export interface RecycleResult {
  ok: number;
  failed: string[];
}

export interface StartupItem {
  id: string;
  name: string;
  command: string;
  location: string;
  enabled: boolean;
  exePath: string | null;
}

export interface ProcessInfo {
  pid: number;
  name: string;
  cpu: number;
  memory: number;
  exe: string | null;
}

export interface ServiceInfo {
  name: string;
  displayName: string;
  status: string; // Running / Stopped / ...
  startMode: string; // Auto / Manual / Disabled
  description: string;
  critical: boolean;
}

export interface RestorePoint {
  sequence: number;
  description: string;
  createdAt: string;
}

export interface RestoreStatus {
  enabled: boolean;
  points: RestorePoint[];
}

export type ProgramKind = "win32" | "store" | "portable";

export interface ProgramInfo {
  id: string;
  name: string;
  publisher: string;
  version: string;
  size: number;
  installDate: string; // YYYY-MM-DD or ""
  kind: ProgramKind;
  location: string;
  protected: boolean;
  canUninstall: boolean;
  /** Icon source for api.icons ("" = none). */
  icon: string;
}

export interface UninstallResult {
  finished: boolean;
  leftovers: string[];
}

export interface DefenderStatus {
  available: boolean;
  antivirusEnabled: boolean;
  realtime: boolean;
  signatureUpdated: string;
  signatureAgeDays: number;
  lastQuickScan: string;
  threats: number;
}

export interface FirewallProfile {
  name: string;
  enabled: boolean;
}

export interface ProtectionStatus {
  defender: DefenderStatus;
  otherAntivirus: string[];
  firewall: FirewallProfile[];
}

export interface HotFix {
  id: string;
  description: string;
  installedOn: string;
}

export interface NetAdapter {
  name: string;
  description: string;
  status: string;
  kind: "wifi" | "ethernet" | "other";
  mac: string;
  ipv4: string[];
  gateway: string;
  dns: string[];
  linkSpeed: string;
}

export type DnsPreset = "auto" | "cloudflare" | "google" | "quad9" | "adguard";

export interface SpeedResult {
  pingMs: number;
  downMbps: number;
  upMbps: number;
}

export interface SpeedProgress {
  phase: "ping" | "download" | "upload";
  value: number;
}

export interface Connection {
  process: string;
  pid: number;
  local: string;
  remote: string;
  state: string;
}

export interface WifiProfile {
  ssid: string;
  password: string | null;
  auth: string;
}

export interface GameModeOptions {
  powerPlan: boolean;
  gameMode: boolean;
  visualEffects: boolean;
  pauseUpdates: boolean;
  closeApps: string[]; // process names, e.g. "chrome.exe" (all instances are closed)
}

export interface GameModeStatus {
  active: boolean;
  since: number | null;
  applied: string[];
  isLaptop: boolean;
}

export interface GpuPref {
  exe: string;
  pref: number; // 0 auto, 1 power saving, 2 high performance
}

export interface UpdateCheck {
  available: boolean;
}
