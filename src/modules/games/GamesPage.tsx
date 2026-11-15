import { Gamepad2, Plus, Rocket, Trash2, Zap } from "lucide-react";
import { useEffect, useState } from "react";
import { Checkbox, Empty, ErrorState, PageHeader, Skeleton, Spinner, Toggle, notify } from "../../components/ui";
import { fmt, useLang, useT } from "../../i18n";
import { api, pickPath } from "../../lib/api";
import { baseName, formatBytes, formatDate } from "../../lib/format";
import { useAsync } from "../../lib/hooks";
import { AppIcon } from "../../components/AppIcon";
import { useStore } from "../../lib/store";
import type { GameModeOptions } from "../../lib/types";

export function GamesPage() {
  const t = useT();
  const lang = useLang();
  const status = useAsync(api.gameStatus);
  const candidates = useAsync(api.gameCandidates);
  const prefs = useAsync(api.gpuPrefs);
  const [busy, setBusy] = useState<"apply" | "revert" | null>(null);
  const [opts, setOpts] = useState<Omit<GameModeOptions, "closeApps">>({ powerPlan: true, gameMode: true, visualEffects: true, pauseUpdates: true });
  const [close, setClose] = useState<Set<string>>(new Set());
  const isAdmin = useStore((s) => s.app?.isAdmin ?? true);

  // Pre-select heavy browsers/launchers? No — nothing is closed unless the user checks it.
  useEffect(() => setClose(new Set()), [candidates.data]);

  const active = status.data?.active ?? false;

  const apply = async () => {
    setBusy("apply");
    try {
      const st = await api.gameApply({ ...opts, closeApps: [...close] });
      status.setData(st);
      notify.ok(t.games.enabled);
      void candidates.reload();
    } catch (e) {
      notify.err(e);
      void status.reload();
    } finally {
      setBusy(null);
    }
  };

  const revert = async () => {
    setBusy("revert");
    try {
      const st = await api.gameRevert();
      status.setData(st);
      notify.ok(t.games.disabled);
    } catch (e) {
      notify.err(e);
      void status.reload();
    } finally {
      setBusy(null);
    }
  };

  const options: { key: keyof typeof opts; title: string; desc: string }[] = [
    { key: "powerPlan", title: t.games.optPower, desc: t.games.optPowerDesc },
    { key: "gameMode", title: t.games.optGameMode, desc: t.games.optGameModeDesc },
    { key: "visualEffects", title: t.games.optVisual, desc: t.games.optVisualDesc },
    { key: "pauseUpdates", title: t.games.optUpdates, desc: t.games.optUpdatesDesc },
  ];

  const addGame = async () => {
    const p = await pickPath({ directory: false, filters: [{ name: "Game", extensions: ["exe"] }] });
    if (!p) return;
    try {
      await api.gpuPrefSet(p, 2);
      notify.ok(t.games.gpuSaved);
      void prefs.reload();
    } catch (e) {
      notify.err(e);
    }
  };

  return (
    <>
      <PageHeader title={t.games.title} sub={t.games.sub} />

      {/* Hero */}
      <section className="card" style={{ display: "flex", alignItems: "center", gap: 32, padding: "34px 40px" }}>
        <div
          style={{
            width: 72,
            height: 72,
            borderRadius: 20,
            display: "grid",
            placeItems: "center",
            background: active ? "var(--accent)" : "var(--panel-raised)",
            color: active ? "var(--text-inverse)" : "var(--text)",
            border: "1px solid var(--border-strong)",
            transition: "background-color 300ms ease, color 300ms ease",
            flexShrink: 0,
          }}
        >
          {active ? <Zap size={32} strokeWidth={1.8} /> : <Gamepad2 size={32} strokeWidth={1.5} />}
        </div>
        <div className="cell-main">
          {status.loading && !status.data ? (
            <Skeleton h={26} w={260} />
          ) : (
            <div style={{ fontSize: 26, fontWeight: 600, letterSpacing: "-0.02em" }}>{active ? t.games.onTitle : t.games.offTitle}</div>
          )}
          <div className="muted" style={{ marginTop: 6 }}>
            {active && status.data?.since ? fmt(t.games.since, { time: formatDate(status.data.since, lang) }) : t.games.honest}
          </div>
        </div>
        {active ? (
          <button className="btn btn-secondary lg" disabled={busy !== null} onClick={() => void revert()}>
            {busy === "revert" ? <Spinner /> : null}
            {busy === "revert" ? t.games.reverting : t.games.disable}
          </button>
        ) : (
          <button
            className="btn btn-primary lg"
            title={!isAdmin ? t.common.needsAdmin : undefined}
            disabled={busy !== null || !status.data || !isAdmin}
            onClick={() => void apply()}
          >
            {busy === "apply" ? <Spinner /> : <Rocket size={18} />}
            {busy === "apply" ? t.games.applying : t.games.enable}
          </button>
        )}
      </section>

      {status.error ? <div className="card"><ErrorState error={status.error} onRetry={() => void status.reload()} /></div> : null}
      {status.data?.isLaptop ? <div className="notice">{t.games.laptopNote}</div> : null}

      <div className="grid-2" style={{ alignItems: "start" }}>
        <section className="card flush">
          <div className="list-head">{t.games.whatDoes}</div>
          {options.map((o) => (
            <div key={o.key} className="setting-row">
              <div className="setting-text">
                <div className="setting-title">{o.title}</div>
                <div className="setting-desc">{o.desc}</div>
              </div>
              <Toggle
                checked={active ? (status.data?.applied.includes(o.key) ?? false) : opts[o.key]}
                disabled={active || busy !== null}
                label={o.title}
                onChange={(v) => setOpts((s) => ({ ...s, [o.key]: v }))}
              />
            </div>
          ))}
        </section>

        <section className="card flush">
          <div className="list-head">{t.games.closeApps}</div>
          <div className="small faint" style={{ padding: "12px 20px 0" }}>{t.games.closeAppsDesc}</div>
          {candidates.error ? (
            <ErrorState error={candidates.error} onRetry={() => void candidates.reload()} />
          ) : !candidates.data ? (
            <div style={{ padding: 20, display: "grid", gap: 12 }}>
              <Skeleton h={14} />
              <Skeleton h={14} w="70%" />
            </div>
          ) : candidates.data.length === 0 ? (
            <div className="empty small">{t.games.noCandidates}</div>
          ) : (
            <div className="list" style={{ marginTop: 6, maxHeight: 300, overflowY: "auto" }}>
              {candidates.data.map((p) => (
                <div key={p.name} className="list-row" style={{ minHeight: 48 }}>
                  <Checkbox
                    checked={close.has(p.name)}
                    label={p.name}
                    onChange={(v) =>
                      setClose((s) => {
                        const n = new Set(s);
                        if (v) n.add(p.name);
                        else n.delete(p.name);
                        return n;
                      })
                    }
                  />
                  <AppIcon spec={p.exe} name={p.name} size={28} />
                  <span className="cell-main truncate">{p.name}</span>
                  <span className="num small muted">{formatBytes(p.memory, lang)}</span>
                </div>
              ))}
            </div>
          )}
        </section>
      </div>

      {/* GPU preference */}
      <section className="card flush">
        <div className="row" style={{ padding: "20px 24px" }}>
          <div className="cell-main">
            <div className="section-title">{t.games.gpuTitle}</div>
            <div className="small muted" style={{ marginTop: 4 }}>{t.games.gpuDesc}</div>
          </div>
          <button className="btn btn-secondary" onClick={() => void addGame()}>
            <Plus size={16} />
            {t.games.gpuAdd}
          </button>
        </div>
        <div style={{ borderTop: "1px solid var(--divider)" }}>
          {prefs.error ? (
            <ErrorState error={prefs.error} onRetry={() => void prefs.reload()} />
          ) : !prefs.data ? null : prefs.data.length === 0 ? (
            <Empty text={t.games.gpuEmpty} />
          ) : (
            <div className="list">
              {prefs.data.map((g) => (
                <div key={g.exe} className="list-row">
                  <AppIcon spec={g.exe} name={baseName(g.exe)} />
                  <div className="cell-main">
                    <div className="cell-title">{baseName(g.exe)}</div>
                    <div className="cell-sub" title={g.exe}>{g.exe}</div>
                  </div>
                  <select
                    className="select"
                    style={{ height: 32, fontSize: 13 }}
                    value={g.pref}
                    aria-label={baseName(g.exe)}
                    onChange={async (e) => {
                      try {
                        await api.gpuPrefSet(g.exe, Number(e.target.value));
                        notify.ok(t.games.gpuSaved);
                        void prefs.reload();
                      } catch (err) {
                        notify.err(err);
                      }
                    }}
                  >
                    <option value={0}>{t.games.gpuAuto}</option>
                    <option value={1}>{t.games.gpuSaving}</option>
                    <option value={2}>{t.games.gpuHigh}</option>
                  </select>
                  <button
                    className="icon-btn"
                    aria-label={t.common.delete}
                    onClick={async () => {
                      try {
                        await api.gpuPrefRemove(g.exe);
                        void prefs.reload();
                      } catch (err) {
                        notify.err(err);
                      }
                    }}
                  >
                    <Trash2 size={16} />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
      </section>
    </>
  );
}
