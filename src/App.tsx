import { motion } from "motion/react";
import { ShieldAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { Toaster } from "sonner";
import { ConfirmDialog, UpdateModal } from "./components/Dialogs";
import { InstallerScreen } from "./components/Installer";
import { Palette } from "./components/Palette";
import { Sidebar } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { notify, useCalm } from "./components/ui";
import { useT } from "./i18n";
import { api, on } from "./lib/api";
import type { InstallState } from "./lib/types";
import { useStore, type ModuleId } from "./lib/store";
import { MODULES } from "./modules/registry";

function AdminBanner() {
  const t = useT();
  const app = useStore((s) => s.app);
  if (!app || app.isAdmin) return null;
  return (
    <div className="notice warn" style={{ margin: "0 32px 8px", maxWidth: 1216 }}>
      <ShieldAlert size={16} />
      <span style={{ flex: 1 }}>{t.shell.adminBanner}</span>
      <button className="btn btn-secondary sm" onClick={() => api.relaunchAsAdmin().catch(notify.err)}>
        {t.shell.restartAdmin}
      </button>
    </div>
  );
}

export default function App() {
  const route = useStore((s) => s.route);
  const motionSetting = useStore((s) => s.settings.motion);
  const setPalette = useStore((s) => s.setPalette);
  const calm = useCalm();
  // null until known; setup / uninstall replace the whole app with the installer screen.
  const [install, setInstall] = useState<InstallState | null>(null);

  // Boot: settings + app info + install mode, then reveal the window.
  useEffect(() => {
    const st = useStore.getState();
    Promise.allSettled([api.settingsGet(), api.appInfo(), api.installState()]).then(([s, a, i]) => {
      if (s.status === "fulfilled") st.setSettings(s.value);
      else notify.err(s.reason);
      if (a.status === "fulfilled") st.setApp(a.value);
      else notify.err(a.reason);
      setInstall(i.status === "fulfilled" ? i.value : { mode: "normal", defaultDir: "", alreadyInstalled: null });
      requestAnimationFrame(() => void api.showWindow().catch(() => undefined));
    });
  }, []);

  useEffect(() => {
    document.documentElement.dataset.motion = motionSetting;
  }, [motionSetting]);

  // Ctrl+K opens the palette; backend events from tray and global hotkey.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && (e.key === "k" || e.key === "K" || e.code === "KeyK")) {
        e.preventDefault();
        setPalette(!useStore.getState().paletteOpen);
      }
    };
    window.addEventListener("keydown", onKey);
    const offs: (() => void)[] = [];
    void on("palette://open", () => setPalette(true)).then((u) => offs.push(u));
    void on<string>("app://navigate", (id) => {
      if (MODULES.some((m) => m.id === id)) useStore.getState().navigate(id as ModuleId);
    }).then((u) => offs.push(u));
    return () => {
      window.removeEventListener("keydown", onKey);
      offs.forEach((f) => f());
    };
  }, [setPalette]);

  // Block the browser context menu and accidental reload/zoom shortcuts in the desktop app.
  useEffect(() => {
    const onMenu = (e: MouseEvent) => {
      const el = e.target as HTMLElement;
      if (!el.closest("input, textarea, [data-allow-menu]")) e.preventDefault();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "F5" || (e.ctrlKey && (e.key === "r" || e.key === "R" || e.key === "p" || e.key === "P"))) e.preventDefault();
    };
    window.addEventListener("contextmenu", onMenu);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("contextmenu", onMenu);
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  const Current = MODULES.find((m) => m.id === route)?.component ?? MODULES[0].component;

  if (!install) return null;
  if (install.mode !== "normal") {
    return (
      <>
        <InstallerScreen state={install} />
        <Toaster theme="dark" position="bottom-right" offset={20} toastOptions={{ className: "sm-toast", duration: 3500 }} />
      </>
    );
  }

  return (
    <>
      <div className="app">
        <Sidebar />
        <main className="main">
          <TitleBar />
          <AdminBanner />
          <div className="page-scroll" key={route}>
            <motion.div
              className="page"
              initial={calm ? { opacity: 0 } : { opacity: 0, transform: "translateY(6px)" }}
              animate={calm ? { opacity: 1 } : { opacity: 1, transform: "translateY(0px)" }}
              transition={{ duration: calm ? 0.12 : 0.22, ease: [0.23, 1, 0.32, 1] }}
            >
              <Current />
            </motion.div>
          </div>
        </main>
      </div>
      {/* Overlays live outside the grid so they never add rows to the layout. */}
      <Palette />
      <ConfirmDialog />
      <UpdateModal />
      <Toaster theme="dark" position="bottom-right" offset={20} toastOptions={{ className: "sm-toast", duration: 3500 }} />
    </>
  );
}
