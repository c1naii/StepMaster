import { AnimatePresence, motion } from "motion/react";
import { Check, FolderOpen, Globe, X } from "lucide-react";
import { useEffect, useState } from "react";
import { LANGS, fmt, useT } from "../i18n";
import { api, errorText, pickPath, win, subscribe } from "../lib/api";
import type { InstallState, Lang } from "../lib/types";
import { useStore } from "../lib/store";
import { Checkbox, Spinner, useCalm } from "./ui";

const STEPS = ["copy", "register", "shortcuts", "launch"] as const;
type Step = (typeof STEPS)[number];

/** Same rule as the backend: the program always gets its own "StepMaster" folder. */
export function finalDir(dir: string): string {
  const d = dir.trim().replace(/[\\/]+$/, "");
  return /(^|\\)stepmaster$/i.test(d) ? d : `${d}\\StepMaster`;
}

/** Language picker right in the setup and uninstall windows. Saved to settings, so the installed
 *  program opens in the language chosen here. */
function LangSwitch() {
  const settings = useStore((s) => s.settings);
  const setSettings = useStore((s) => s.setSettings);
  const change = async (language: Lang) => {
    if (language === settings.language) return;
    const prev = settings;
    const next = { ...settings, language };
    setSettings(next); // switch at once
    try {
      setSettings(await api.settingsSet(next));
    } catch {
      // Could not save (e.g. no rights yet): keep the choice for this window anyway.
      setSettings({ ...prev, language });
    }
  };
  return (
    <div className="installer-lang">
      <Globe size={15} strokeWidth={1.7} aria-hidden />
      <select className="select" value={settings.language} aria-label="Language" onChange={(e) => void change(e.target.value as Lang)}>
        {LANGS.map((l) => (
          <option key={l.id} value={l.id}>
            {l.name}
          </option>
        ))}
      </select>
    </div>
  );
}

function Frame({ children }: { children: React.ReactNode }) {
  const t = useT();
  const calm = useCalm();
  return (
    <div className="installer">
      <header className="installer-bar">
        <LangSwitch />
        <div className="titlebar-drag" data-tauri-drag-region />
        <button className="window-btn close" aria-label={t.shell.closeWindow} onClick={() => void win.close()}>
          <X size={17} strokeWidth={1.6} />
        </button>
      </header>
      <motion.div
        className="installer-body"
        initial={calm ? { opacity: 0 } : { opacity: 0, transform: "translateY(8px)" }}
        animate={calm ? { opacity: 1 } : { opacity: 1, transform: "translateY(0px)" }}
        transition={{ duration: calm ? 0.12 : 0.32, ease: [0.23, 1, 0.32, 1] }}
      >
        <img src="/logo-mark.png" alt="" width={52} height={52} draggable={false} />
        {children}
      </motion.div>
    </div>
  );
}

/** Without admin rights the action button asks Windows for them (UAC) instead of failing. */
function useElevate() {
  const isAdmin = useStore((s) => s.app?.isAdmin ?? true);
  return { isAdmin, elevate: () => api.relaunchAsAdmin().catch(() => undefined) };
}

function Setup({ state }: { state: InstallState }) {
  const t = useT();
  const calm = useCalm();
  const { isAdmin, elevate } = useElevate();
  const [dir, setDir] = useState(state.alreadyInstalled ?? state.defaultDir);
  const [desktop, setDesktop] = useState(true);
  const [startMenu, setStartMenu] = useState(true);
  const [busy, setBusy] = useState(false);
  const [step, setStep] = useState<Step | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let off: (() => void) | undefined;
    off = subscribe<{ step: Step }>("install://progress", (p) => setStep(p.step));
    return () => off?.();
  }, []);

  const choose = async () => {
    const p = await pickPath({ directory: true }).catch(() => null);
    if (p) setDir(finalDir(p));
  };

  const run = async () => {
    if (!isAdmin) return void elevate();
    setBusy(true);
    setError(null);
    setStep(null);
    try {
      await api.installRun(dir, desktop, startMenu); // on success the backend starts the installed copy and exits
    } catch (e) {
      setError(fmt(t.install.failed, { e: errorText(e) }));
      setBusy(false);
      setStep(null);
    }
  };

  const labels: Record<Step, string> = {
    copy: t.install.sCopy,
    register: t.install.sRegister,
    shortcuts: t.install.sShortcuts,
    launch: t.install.sLaunch,
  };
  const current = step ? STEPS.indexOf(step) : 0;

  return (
    <Frame>
      <h1 className="installer-title">{t.install.title}</h1>

      <AnimatePresence mode="wait" initial={false}>
        {busy ? (
          <motion.ol
            key="steps"
            className="installer-steps"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: calm ? 0.1 : 0.18 }}
            aria-live="polite"
          >
            {STEPS.map((s, i) => (
              <li key={s} data-state={i < current ? "done" : i === current ? "active" : "wait"}>
                <span className="installer-step-icon">{i < current ? <Check size={13} strokeWidth={3} /> : i === current ? <Spinner size={14} /> : null}</span>
                {labels[s]}
              </li>
            ))}
          </motion.ol>
        ) : (
          <motion.div
            key="form"
            className="installer-form"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: calm ? 0.1 : 0.18 }}
          >
            <div className="installer-label">{t.install.folder}</div>
            <div className="installer-path">
              <span className="installer-path-text" title={dir}>
                {dir}
              </span>
              <button className="btn btn-secondary sm" onClick={() => void choose()}>
                <FolderOpen size={15} />
                {t.install.change}
              </button>
            </div>
            <label className="installer-opt">
              <Checkbox checked={desktop} onChange={setDesktop} label={t.install.desktop} />
              <span>{t.install.desktop}</span>
            </label>
            <label className="installer-opt">
              <Checkbox checked={startMenu} onChange={setStartMenu} label={t.install.startMenu} />
              <span>{t.install.startMenu}</span>
            </label>
            {error ? <div className="notice warn">{error}</div> : null}
          </motion.div>
        )}
      </AnimatePresence>

      <div className="installer-actions">
        <button className="btn btn-primary" disabled={busy} onClick={() => void run()} autoFocus>
          {busy ? t.install.installing : t.install.install}
        </button>
      </div>
    </Frame>
  );
}

function Uninstall() {
  const t = useT();
  const { isAdmin, elevate } = useElevate();
  const [removeData, setRemoveData] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async () => {
    if (!isAdmin) return void elevate();
    setBusy(true);
    setError(null);
    try {
      await api.uninstallRun(removeData); // the backend exits when done
    } catch (e) {
      setError(fmt(t.install.uFailed, { e: errorText(e) }));
      setBusy(false);
    }
  };

  return (
    <Frame>
      <h1 className="installer-title">{t.install.uTitle}</h1>
      <div className="installer-form">
        <label className="installer-opt">
          <Checkbox checked={removeData} onChange={setRemoveData} label={t.install.uData} />
          <span>{t.install.uData}</span>
        </label>
        {error ? <div className="notice warn">{error}</div> : null}
      </div>
      <div className="installer-actions">
        <button className="btn btn-secondary" disabled={busy} onClick={() => void api.quit()} autoFocus>
          {t.common.cancel}
        </button>
        <button className="btn btn-danger" disabled={busy} onClick={() => void run()}>
          {busy ? (
            <>
              <Spinner size={14} />
              {t.install.uBusy}
            </>
          ) : (
            t.install.uRun
          )}
        </button>
      </div>
    </Frame>
  );
}

export function InstallerScreen({ state }: { state: InstallState }) {
  return state.mode === "uninstall" ? <Uninstall /> : <Setup state={state} />;
}
