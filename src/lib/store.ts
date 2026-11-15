import { create } from "zustand";
import type { AppInfo, Settings } from "./types";

export type ModuleId =
  | "home"
  | "cleanup"
  | "disk"
  | "programs"
  | "system"
  | "protection"
  | "network"
  | "games"
  | "settings";

export interface ConfirmRequest {
  title: string;
  text?: string;
  confirmLabel: string;
  cancelLabel: string;
  danger?: boolean;
  details?: string[];
  resolve: (ok: boolean) => void;
}

export const DEFAULT_SETTINGS: Settings = {
  language: "ru",
  autostart: false,
  startMinimized: false,
  closeToTray: true,
  motion: "full",
  hotkey: "Ctrl+Shift+Space",
  trayMonitor: true,
};

interface State {
  route: ModuleId;
  /** Tab to open on the next page mount (deep link from palette/home). */
  pendingTab: string | null;
  /** Action to run on the next page mount (e.g. "scan" on cleanup). */
  pendingAction: string | null;
  settings: Settings;
  app: AppInfo | null;
  paletteOpen: boolean;
  confirm: ConfirmRequest | null;
  navigate: (route: ModuleId, tab?: string | null, action?: string | null) => void;
  takePendingTab: () => string | null;
  takePendingAction: () => string | null;
  setSettings: (s: Settings) => void;
  setApp: (a: AppInfo) => void;
  setPalette: (open: boolean) => void;
  ask: (req: Omit<ConfirmRequest, "resolve">) => Promise<boolean>;
  closeConfirm: (ok: boolean) => void;
}

export const useStore = create<State>((set, get) => ({
  route: "home",
  pendingTab: null,
  pendingAction: null,
  settings: DEFAULT_SETTINGS,
  app: null,
  paletteOpen: false,
  confirm: null,
  navigate: (route, tab = null, action = null) => set({ route, pendingTab: tab, pendingAction: action, paletteOpen: false }),
  takePendingTab: () => {
    const t = get().pendingTab;
    if (t !== null) set({ pendingTab: null });
    return t;
  },
  takePendingAction: () => {
    const a = get().pendingAction;
    if (a !== null) set({ pendingAction: null });
    return a;
  },
  setSettings: (settings) => set({ settings }),
  setApp: (app) => set({ app }),
  setPalette: (paletteOpen) => set({ paletteOpen }),
  ask: (req) =>
    new Promise<boolean>((resolve) => {
      // Resolve a previous pending dialog as cancelled, so callers never hang.
      get().confirm?.resolve(false);
      set({ confirm: { ...req, resolve } });
    }),
  closeConfirm: (ok) => {
    const c = get().confirm;
    if (!c) return;
    set({ confirm: null });
    c.resolve(ok);
  },
}));

/** Shortcut for confirmation dialogs outside React components. */
export const ask = (req: Omit<ConfirmRequest, "resolve">) => useStore.getState().ask(req);
