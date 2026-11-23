// Shared types between UI and the Rust backend (serde camelCase).

export type Lang = "ru" | "en" | "uk" | "es" | "fr" | "de" | "pt" | "zh";
export type MotionLevel = "full" | "reduced" | "off";

export interface Settings {
  language: Lang;
  autostart: boolean;
  startMinimized: boolean;
  closeToTray: boolean;
  motion: MotionLevel;
  hotkey: string; // e.g. "Ctrl+Shift+Space" or "" (off)
  trayMonitor: boolean;
  updateNotify: boolean; // update popup in the bottom-right corner
}

export interface WingetState {
  available: boolean;
  version: string;
  /** winget package ids installed on this PC. */
  installed: string[];
}

export interface WingetHit {
  name: string;
  id: string;
  version: string;
}

export type WingetStep = "queued" | "running" | "done" | "failed";

export interface WingetProgress {
  id: string;
  state: WingetStep;
  /** "already" | "reboot" | error text */
  note: string | null;
  /** Download progress of the running package, 0..100. */
  percent?: number | null;
  /** Bytes downloaded / installer size, when known. */
  done?: number | null;
  total?: number | null;
}

export interface InstallState {
  /** "setup" = not installed here, "uninstall" = started by Windows to remove it, "normal" = usual app. */
  mode: "setup" | "uninstall" | "normal";
  defaultDir: string;
  alreadyInstalled: string | null;
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
  /** "Stop" was pressed: the rest of the categories were left as they were. */
  stopped: boolean;
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

/** Long operations with a "Stop" button. */
export type StoppableOp = "diag" | "cleanup" | "speed" | "dupes" | "defender" | "repair";

/** How a journal entry is put back (only the type matters to the UI). */
export type JournalUndo = { type: "serviceMode" | "serviceRun" | "startup" | "dns" | "firewall" | "gpuPref" | "gameMode" };

/** One change made by StepMaster ("What StepMaster changed"). */
export interface JournalEntry {
  id: number;
  /** Unix seconds. */
  at: number;
  kind: string;
  target: string;
  detail: string;
  undo: JournalUndo | null;
  reverted: boolean;
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
  disableProxy: boolean; // turn off the system proxy (VPN clients) while playing
  relaunch: boolean; // open the closed programs again when game mode is turned off
}

export interface GameModeStatus {
  active: boolean;
  since: number | null;
  applied: string[];
  isLaptop: boolean;
  relaunch: string[]; // file names that will be opened again
  proxyOn: boolean;
}

export interface DisplayInfo {
  device: string; // \\.\DISPLAY1
  name: string;
  width: number;
  height: number;
  current: number;
  rates: number[]; // ascending, at the current resolution
  primary: boolean;
}

export interface WifiAnalysis {
  connected: boolean;
  adapter: string;
  ssid: string;
  band: "2.4" | "5" | "6" | "";
  channel: number;
  signal: number;
  rssi: number | null;
  rxMbps: number;
  txMbps: number;
  standard: string;
  supports5: boolean | null;
  neighbors: number;
  networks: number;
  betterChannel: number | null;
  selectiveSuspend: boolean | null;
  tips: string[]; // adapter24 | use5 | crowded | channel | weak | suspend
}

export interface GpuPref {
  exe: string;
  pref: number; // 0 auto, 1 power saving, 2 high performance
}

export interface UpdateCheck {
  available: boolean;
}

/** "What's new" for the update window (from the GitHub release description and banner). */
export interface UpdateNotes {
  sections: { title: string; items: string[] }[];
  /** Banner picture as a data: URL. */
  image: string | null;
}

/** Shown once after an update: notes of the version now running. */
export interface WhatsNew {
  version: string;
  notes: UpdateNotes;
}

/* ---------- Downloads review ---------- */
export type DownloadFlag = "installed" | "extracted" | "duplicate" | "partial" | "old";
export type DownloadCategory =
  | "installers"
  | "archives"
  | "images"
  | "videos"
  | "pictures"
  | "audio"
  | "documents"
  | "torrents"
  | "partial"
  | "folders"
  | "shortcuts"
  | "other";

export interface DownloadItem {
  path: string;
  name: string;
  size: number;
  isDir: boolean;
  category: DownloadCategory;
  ageDays: number;
  flags: DownloadFlag[];
  installedAs: string | null;
}

export interface DownloadsReport {
  root: string;
  total: number;
  items: DownloadItem[];
}

/* ---------- Smart diagnostics ---------- */
export type Severity = "critical" | "high" | "medium" | "low" | "info";
export type DiagCategory = "performance" | "thermal" | "hardware" | "system" | "security";

export interface DiagIssue {
  code: string;
  category: DiagCategory;
  severity: Severity;
  params: Record<string, string>;
  details: string[];
  fix: string | null;
  fixArgs: string[];
  open: string | null;
}

export interface DiagCheck {
  code: string;
  category: DiagCategory;
  status: "ok" | "issue" | "skipped";
}

export interface DiagReport {
  at: number;
  durationMs: number;
  score: number;
  admin: boolean;
  defenderScanned: boolean;
  issues: DiagIssue[];
  checks: DiagCheck[];
}

export interface DiagProgress {
  step: "defender" | "system" | "performance" | "evaluate" | "done";
  done: number;
  total: number;
}

export interface FixResult {
  ok: boolean;
  note: string;
}

// ---------- wallpapers ----------
export interface WallItem {
  id: string;
  name: string;
  builtin: boolean;
  path: string;
  thumb: string;
  width: number;
  height: number;
}
export interface WallOptions {
  fit: "fill" | "fit" | "stretch" | "center" | "span";
  auto: boolean;
  /** Minutes between changes; 0 = at Windows sign-in. */
  interval: number;
  shuffle: boolean;
  source: "all" | "mine" | "builtin";
}
export interface WallState {
  builtin: WallItem[];
  mine: WallItem[];
  current: string | null;
  options: WallOptions;
  canRestore: boolean;
}
export interface WallAddResult {
  state: WallState;
  added: number;
  failed: string[];
}
