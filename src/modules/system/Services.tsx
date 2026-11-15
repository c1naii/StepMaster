import { Play, RotateCw, Square } from "lucide-react";
import { useMemo, useState } from "react";
import { ErrorState, SearchInput, SkeletonRows, Spinner, Tabs, notify } from "../../components/ui";
import { useT } from "../../i18n";
import { api } from "../../lib/api";
import { useAsync } from "../../lib/hooks";
import { useStore } from "../../lib/store";
import type { ServiceInfo } from "../../lib/types";

type Filter = "running" | "all";

export function Services() {
  const t = useT();
  const list = useAsync(api.services);
  const [q, setQ] = useState("");
  const [filter, setFilter] = useState<Filter>("running");
  const [busy, setBusy] = useState<string | null>(null);
  const isAdmin = useStore((s) => s.app?.isAdmin ?? true);

  const rows = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return (list.data ?? [])
      .filter((s) => filter === "all" || s.status === "Running")
      .filter((s) => !needle || `${s.displayName} ${s.name} ${s.description}`.toLowerCase().includes(needle))
      .sort((a, b) => a.displayName.localeCompare(b.displayName));
  }, [list.data, q, filter]);

  const act = async (s: ServiceInfo, fn: () => Promise<void>) => {
    setBusy(s.name);
    try {
      await fn();
      notify.ok(t.system.svcDone);
    } catch (e) {
      notify.err(e);
    } finally {
      setBusy(null);
      void list.reload();
    }
  };

  const modeValue = (m: string) => (m.toLowerCase().startsWith("auto") ? "auto" : m.toLowerCase().startsWith("disab") ? "disabled" : "manual");

  return (
    <>
      <div className="row wrap">
        <Tabs
          value={filter}
          onChange={setFilter}
          items={[
            { id: "running", label: t.system.svcRunning },
            { id: "all", label: t.system.svcAll },
          ]}
        />
        <span className="spacer" />
        <SearchInput value={q} onChange={setQ} placeholder={t.system.svcSearch} />
      </div>
      <section className="card flush">
        {list.error ? (
          <ErrorState error={list.error} onRetry={() => void list.reload()} />
        ) : !list.data ? (
          <SkeletonRows rows={8} />
        ) : (
          <div className="list">
            {rows.map((s) => {
              const running = s.status === "Running";
              const locked = s.critical || busy !== null || !isAdmin;
              return (
                <div key={s.name} className="list-row hoverable">
                  <span className={`dot ${running ? "" : "muted"}`} />
                  <div className="cell-main">
                    <div className="cell-title" title={s.displayName}>{s.displayName}</div>
                    <div className="cell-sub" title={s.description}>{s.critical ? t.system.critical : s.description || s.name}</div>
                  </div>
                  <span className="small muted" style={{ width: 92, textAlign: "right" }}>
                    {running ? t.system.stRunning : t.system.stStopped}
                  </span>
                  <select
                    className="select"
                    style={{ height: 32, fontSize: 13, width: 150 }}
                    disabled={locked}
                    value={modeValue(s.startMode)}
                    aria-label={s.displayName}
                    onChange={(e) => void act(s, () => api.serviceStartMode(s.name, e.target.value as "auto" | "manual" | "disabled"))}
                  >
                    <option value="auto">{t.system.modeAuto}</option>
                    <option value="manual">{t.system.modeManual}</option>
                    <option value="disabled">{t.system.modeDisabled}</option>
                  </select>
                  {busy === s.name ? (
                    <span className="icon-btn"><Spinner size={15} /></span>
                  ) : running ? (
                    <>
                      <button className="icon-btn" disabled={locked} aria-label={t.system.restart} title={t.system.restart} onClick={() => void act(s, () => api.serviceAction(s.name, "restart"))}>
                        <RotateCw size={15} />
                      </button>
                      <button className="icon-btn" disabled={locked} aria-label={t.system.stop} title={t.system.stop} onClick={() => void act(s, () => api.serviceAction(s.name, "stop"))}>
                        <Square size={14} />
                      </button>
                    </>
                  ) : (
                    <>
                      <span style={{ width: 34 }} />
                      <button className="icon-btn" disabled={locked} aria-label={t.system.start} title={t.system.start} onClick={() => void act(s, () => api.serviceAction(s.name, "start"))}>
                        <Play size={15} />
                      </button>
                    </>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </section>
    </>
  );
}
