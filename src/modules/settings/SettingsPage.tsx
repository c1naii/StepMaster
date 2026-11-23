import { AlertCircle, CheckCircle2, Download, FolderOpen, Keyboard, RefreshCw } from "lucide-react";
import { HotkeysDialog } from "./HotkeysDialog";
import { useRef, useState } from "react";
import { PageHeader, Spinner, Tabs, Toggle, notify } from "../../components/ui";
import { LANGS, fmt, useT } from "../../i18n";
import { api, errorText } from "../../lib/api";
import { useStore } from "../../lib/store";
import type { Lang, MotionLevel, Settings } from "../../lib/types";

const HOTKEYS = ["", "Ctrl+Shift+Space", "Ctrl+Alt+S", "Alt+Shift+S"];

export function SettingsPage() {
  const t = useT();
  const settings = useStore((s) => s.settings);
  const setSettings = useStore((s) => s.setSettings);
  const app = useStore((s) => s.app);
  const [saving, setSaving] = useState(false);
  const [showKeys, setShowKeys] = useState(false);
  const [checking, setChecking] = useState(false);
  const [upd, setUpd] = useState<{ kind: "none" | "available" | "error"; msg?: string } | null>(null);

  // Quick clicks (RU→EN→RU) send several saves: only the latest answer is applied.
  const seq = useRef(0);
  const save = async (patch: Partial<Settings>) => {
    const my = ++seq.current;
    const prev = settings;
    const next = { ...settings, ...patch };
    setSettings(next); // optimistic
    setSaving(true);
    try {
      const saved = await api.settingsSet(next);
      if (my === seq.current) setSettings(saved);
    } catch (e) {
      if (my === seq.current) setSettings(prev);
      notify.err(e);
    } finally {
      if (my === seq.current) setSaving(false);
    }
  };

  const checkUpdates = async () => {
    setChecking(true);
    setUpd(null);
    try {
      const r = await api.updateCheck();
      setUpd({ kind: r.available ? "available" : "none" });
      // Open the same centered update dialog as on startup.
      if (r.available) window.dispatchEvent(new Event("sm:update-available"));
    } catch (e) {
      setUpd({ kind: "error", msg: errorText(e) });
    } finally {
      setChecking(false);
    }
  };

  return (
    <>
      <PageHeader title={t.settings.title} sub={t.settings.sub} />

      <div className="section-title">{t.settings.general}</div>
      <section className="card flush">
        <div className="setting-row">
          <div className="setting-text">
            <div className="setting-title">{t.settings.language}</div>
            <div className="setting-desc">{t.settings.languageDesc}</div>
          </div>
          <select
            className="select"
            style={{ minWidth: 190 }}
            value={settings.language}
            aria-label={t.settings.language}
            onChange={(e) => void save({ language: e.target.value as Lang })}
          >
            {LANGS.map((l) => (
              <option key={l.id} value={l.id}>
                {l.name}
              </option>
            ))}
          </select>
        </div>
        <div className="setting-row">
          <div className="setting-text">
            <div className="setting-title">{t.settings.autostart}</div>
            <div className="setting-desc">{t.settings.autostartDesc}</div>
          </div>
          <Toggle checked={settings.autostart} disabled={saving} label={t.settings.autostart} onChange={(v) => void save({ autostart: v })} />
        </div>
        <div className="setting-row" style={{ opacity: settings.autostart ? 1 : 0.5 }}>
          <div className="setting-text">
            <div className="setting-title">{t.settings.startMinimized}</div>
            <div className="setting-desc">{t.settings.startMinimizedDesc}</div>
          </div>
          <Toggle
            checked={settings.startMinimized}
            disabled={saving || !settings.autostart}
            label={t.settings.startMinimized}
            onChange={(v) => void save({ startMinimized: v })}
          />
        </div>
        <div className="setting-row">
          <div className="setting-text">
            <div className="setting-title">{t.settings.closeToTray}</div>
            <div className="setting-desc">{t.settings.closeToTrayDesc}</div>
          </div>
          <Toggle checked={settings.closeToTray} disabled={saving} label={t.settings.closeToTray} onChange={(v) => void save({ closeToTray: v })} />
        </div>
        <div className="setting-row">
          <div className="setting-text">
            <div className="setting-title">{t.settings.trayMonitor}</div>
            <div className="setting-desc">{t.settings.trayMonitorDesc}</div>
          </div>
          <Toggle checked={settings.trayMonitor} disabled={saving} label={t.settings.trayMonitor} onChange={(v) => void save({ trayMonitor: v })} />
        </div>
        <div className="setting-row">
          <div className="setting-text">
            <div className="setting-title">{t.settings.updateNotify}</div>
            <div className="setting-desc">{t.settings.updateNotifyDesc}</div>
          </div>
          <Toggle checked={settings.updateNotify} disabled={saving} label={t.settings.updateNotify} onChange={(v) => void save({ updateNotify: v })} />
        </div>
        <div className="setting-row">
          <div className="setting-text">
            <div className="setting-title">{t.settings.hotkey}</div>
            <div className="setting-desc">{t.settings.hotkeyDesc}</div>
            <button className="link setting-link" onClick={() => setShowKeys(true)}>
              <Keyboard size={13} />
              {t.settings.hotkeysAll}
            </button>
          </div>
          <select
            className="select"
            style={{ height: 36 }}
            value={settings.hotkey}
            disabled={saving}
            aria-label={t.settings.hotkey}
            onChange={(e) => void save({ hotkey: e.target.value })}
          >
            {HOTKEYS.map((h) => (
              <option key={h} value={h}>
                {h || t.settings.hotkeyOff}
              </option>
            ))}
          </select>
        </div>
        <div className="setting-row">
          <div className="setting-text">
            <div className="setting-title">{t.settings.motion}</div>
            <div className="setting-desc">{t.settings.motionDesc}</div>
          </div>
          <Tabs<MotionLevel>
            value={settings.motion}
            onChange={(v) => void save({ motion: v })}
            items={[
              { id: "full", label: t.settings.motionFull },
              { id: "reduced", label: t.settings.motionReduced },
              { id: "off", label: t.settings.motionOff },
            ]}
          />
        </div>
      </section>

      <div className="section-title" style={{ marginTop: 8 }}>{t.settings.files}</div>
      <section className="card flush">
        <div className="setting-row">
          <div className="setting-text">
            <div className="setting-title">{t.settings.appDir}</div>
            <div className="setting-desc truncate" style={{ userSelect: "text" }}>{app?.exePath ?? t.settings.appDirDesc}</div>
          </div>
          <button className="btn btn-secondary sm" onClick={() => api.openAppDir().catch(notify.err)}>
            <FolderOpen size={15} />
            {t.common.open}
          </button>
        </div>
        <div className="setting-row">
          <div className="setting-text">
            <div className="setting-title">{t.settings.dataDir}</div>
            <div className="setting-desc truncate" style={{ userSelect: "text" }}>{app?.dataDir ?? t.settings.dataDirDesc}</div>
          </div>
          <button className="btn btn-secondary sm" onClick={() => api.openDataDir().catch(notify.err)}>
            <FolderOpen size={15} />
            {t.common.open}
          </button>
        </div>
      </section>

      <div className="section-title" style={{ marginTop: 8 }}>{t.settings.about}</div>
      <section className="card row" style={{ gap: 20 }}>
        <img src="/logo-mark.png" alt="" width={40} height={40} />
        <div className="cell-main">
          <div style={{ fontWeight: 600, fontSize: 17 }}>StepMaster</div>
          <div className="small muted">
            {fmt(t.settings.version, { v: app?.version ?? "—" })}
            {app ? ` · ${app.osName} (${app.osBuild})` : ""}
          </div>
          {app ? (
            <div className="small muted row" style={{ gap: 8, marginTop: 4 }}>
              <span className={`dot ${app.isAdmin ? "" : "warn"}`} />
              {app.isAdmin ? t.shell.admin : t.shell.notAdmin}
              {!app.isAdmin ? (
                <button className="btn btn-ghost sm" onClick={() => void api.relaunchAsAdmin().catch(notify.err)}>
                  {t.shell.restartAdmin}
                </button>
              ) : null}
            </div>
          ) : null}
        </div>
        <button className="btn btn-secondary" disabled={checking} onClick={() => void checkUpdates()}>
          {checking ? <Spinner /> : <RefreshCw size={15} />}
          {checking ? t.settings.checking : t.settings.checkUpdates}
        </button>
      </section>
      {upd ? (
        <div className={`notice ${upd.kind === "error" ? "warn" : ""}`} role="status" style={{ gap: 12 }}>
          {upd.kind === "none" ? <CheckCircle2 size={16} /> : upd.kind === "available" ? <Download size={16} /> : <AlertCircle size={16} />}
          <span style={{ flex: 1, color: upd.kind === "error" ? undefined : "var(--text)" }}>
            {upd.kind === "none"
              ? fmt(t.settings.noUpdates, { v: app?.version ?? "" })
              : upd.kind === "available"
                ? t.settings.updateAvailable
                : fmt(t.settings.checkFailed, { e: upd.msg ?? "" })}
          </span>
          {upd.kind === "available" ? (
            <button className="btn btn-primary sm" onClick={() => window.dispatchEvent(new Event("sm:update-available"))}>
              {t.update.update}
            </button>
          ) : null}
        </div>
      ) : null}
      <HotkeysDialog open={showKeys} onClose={() => setShowKeys(false)} />
    </>
  );
}
