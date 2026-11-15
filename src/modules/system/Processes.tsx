import { X } from "lucide-react";
import { useMemo, useState } from "react";
import { ErrorState, SearchInput, SkeletonRows, notify } from "../../components/ui";
import { fmt, useLang, useT } from "../../i18n";
import { api } from "../../lib/api";
import { AppIcon } from "../../components/AppIcon";
import { formatBytes, formatDecimal } from "../../lib/format";
import { usePoll } from "../../lib/hooks";
import { ask } from "../../lib/store";

type Sort = "cpu" | "memory" | "name";

export function Processes() {
  const t = useT();
  const lang = useLang();
  const { data, error } = usePoll(api.processes, 2500);
  const [q, setQ] = useState("");
  const [sort, setSort] = useState<Sort>("memory");

  const rows = useMemo(() => {
    const needle = q.trim().toLowerCase();
    const arr = (data ?? []).filter((p) => !needle || p.name.toLowerCase().includes(needle) || String(p.pid) === needle);
    const byName = (a: { name: string }, b: { name: string }) => a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" });
    return [...arr].sort((a, b) => (sort === "cpu" ? b.cpu - a.cpu : sort === "memory" ? b.memory - a.memory : byName(a, b))).slice(0, 200);
  }, [data, q, sort]);

  const kill = async (pid: number, name: string) => {
    const ok = await ask({
      title: fmt(t.system.endTitle, { name }),
      text: t.system.endText,
      confirmLabel: t.system.end,
      cancelLabel: t.common.cancel,
      danger: true,
    });
    if (!ok) return;
    try {
      await api.processKill(pid);
      notify.ok(t.system.ended);
    } catch (e) {
      notify.err(e);
    }
  };

  const head = (id: Sort, label: string, width?: number) => (
    <button
      style={{ width, textAlign: width ? "right" : "left", flex: width ? undefined : 1, color: sort === id ? "var(--text)" : undefined, textTransform: "inherit", letterSpacing: "inherit", fontWeight: "inherit", fontSize: "inherit" }}
      onClick={() => setSort(id)}
    >
      {label}
    </button>
  );

  return (
    <>
      <div className="row">
        <SearchInput value={q} onChange={setQ} placeholder={t.system.procSearch} />
      </div>
      <section className="card flush">
        <div className="list-head">
          {head("name", t.system.colName)}
          {head("cpu", t.system.colCpu, 70)}
          {head("memory", t.system.colMemory, 100)}
          <span style={{ width: 34 }} />
        </div>
        {error && !data ? (
          <ErrorState error={error} />
        ) : !data ? (
          <SkeletonRows rows={8} />
        ) : (
          <div className="list">
            {rows.map((p) => (
              <div key={p.pid} className="list-row hoverable" style={{ minHeight: 46, padding: "6px 20px" }}>
                <AppIcon spec={p.exe} name={p.name} size={28} />
                <div className="cell-main">
                  <div className="cell-title" title={p.exe ?? p.name}>{p.name}</div>
                </div>
                <div className="num muted small" style={{ width: 70, textAlign: "right" }}>
                  {formatDecimal(p.cpu, 1, lang)}%
                </div>
                <div className="num small" style={{ width: 100, textAlign: "right" }}>
                  {formatBytes(p.memory, lang)}
                </div>
                <button className="icon-btn" aria-label={t.system.end} onClick={() => void kill(p.pid, p.name)}>
                  <X size={16} />
                </button>
              </div>
            ))}
          </div>
        )}
      </section>
    </>
  );
}
