import { Cpu, Gamepad2, History, MonitorCog, Network, Package, Power, RotateCcw, Shield, Sparkles, Stethoscope, Trash2, X } from "lucide-react";
import type { ReactNode } from "react";
import { useMemo, useState } from "react";
import { Empty, ErrorState, PageHeader, SkeletonRows, Spinner, Tabs, notify } from "../../components/ui";
import { fmt, useLang, useT } from "../../i18n";
import type { Dict } from "../../i18n/ru";
import { api } from "../../lib/api";
import { invalidate } from "../../lib/cache";
import { formatBytes, localeOf } from "../../lib/format";
import { useAsync } from "../../lib/hooks";
import type { JournalEntry, Lang } from "../../lib/types";

const ICONS: Record<string, ReactNode> = {
  service_mode: <MonitorCog size={16} />,
  service_stop: <MonitorCog size={16} />,
  service_start: <MonitorCog size={16} />,
  service_restart: <MonitorCog size={16} />,
  startup_off: <Power size={16} />,
  startup_on: <Power size={16} />,
  dns: <Network size={16} />,
  network_reset: <Network size={16} />,
  firewall_on: <Shield size={16} />,
  firewall_off: <Shield size={16} />,
  gpu_pref: <Gamepad2 size={16} />,
  gpu_pref_remove: <Gamepad2 size={16} />,
  game_mode: <Gamepad2 size={16} />,
  process_kill: <X size={16} />,
  uninstall: <Package size={16} />,
  cleanup: <Sparkles size={16} />,
  recycle: <Trash2 size={16} />,
  restore_point: <History size={16} />,
  diag_fix: <Stethoscope size={16} />,
};

/** Human text of an entry; `detail` is a raw value the backend stored. */
export function entryText(e: JournalEntry, t: Dict, lang: Lang): string {
  const j = t.journal;
  let detail = e.detail;
  if (e.kind === "service_mode") detail = (j.modes as Record<string, string>)[e.detail] ?? e.detail;
  else if (e.kind === "dns") detail = (j.dns as Record<string, string>)[e.detail] ?? e.detail;
  else if (e.kind === "gpu_pref") detail = e.detail === "" ? "" : (j.gpu[Number(e.detail)] ?? e.detail);
  else if (e.kind === "cleanup") detail = formatBytes(Number(e.detail) || 0, lang);
  else if (e.kind === "recycle") {
    const [n, bytes] = e.detail.split("|");
    detail = `${n} · ${formatBytes(Number(bytes) || 0, lang)}`;
  } else if (e.kind === "diag_fix") detail = (t.diag.fixes as Record<string, string>)[e.target] ?? e.target;
  const tpl = (j.kinds as Record<string, string>)[e.kind];
  if (!tpl) return e.target || e.kind;
  // Long paths (GPU choice for an exe) read better by file name.
  const target = e.kind.startsWith("gpu_pref") ? e.target.split("\\").pop() ?? e.target : e.target;
  return fmt(tpl, { target, detail });
}

function dayLabel(at: number, t: Dict, lang: Lang): string {
  const d = new Date(at * 1000);
  const today = new Date();
  const start = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diff = Math.round((start(today) - start(d)) / 86_400_000);
  if (diff === 0) return t.journal.today;
  if (diff === 1) return t.journal.yesterday;
  return d.toLocaleDateString(localeOf(lang), { day: "numeric", month: "long", year: d.getFullYear() === today.getFullYear() ? undefined : "numeric" });
}

export function JournalPage() {
  const t = useT();
  const lang = useLang();
  const list = useAsync(api.journalGet);
  const [busy, setBusy] = useState<number | null>(null);
  const [filter, setFilter] = useState<"all" | "undo">("all");

  const groups = useMemo(() => {
    const items = (list.data ?? []).filter((e) => filter === "all" || (e.undo && !e.reverted));
    const out: { day: string; items: JournalEntry[] }[] = [];
    for (const e of items) {
      const day = dayLabel(e.at, t, lang);
      const last = out[out.length - 1];
      if (last && last.day === day) last.items.push(e);
      else out.push({ day, items: [e] });
    }
    return out;
  }, [list.data, filter, t, lang]);

  const revert = async (e: JournalEntry) => {
    if (busy !== null) return;
    setBusy(e.id);
    try {
      list.setData(await api.journalRevert(e.id));
      // Pages that show these settings must re-read them.
      invalidate("startup");
      invalidate("protection");
      invalidate("overview");
      notify.ok(t.journal.undone);
    } catch (err) {
      notify.err(err);
    } finally {
      setBusy(null);
    }
  };

  return (
    <>
      <PageHeader
        title={t.journal.title}
        sub={t.journal.subtitle}
        right={
          <Tabs
            value={filter}
            onChange={setFilter}
            items={[
              { id: "all", label: t.common.all },
              { id: "undo", label: t.journal.onlyUndo },
            ]}
          />
        }
      />
      <section className="card flush">
        {list.error ? (
          <ErrorState error={list.error} onRetry={() => void list.reload()} />
        ) : !list.data ? (
          <SkeletonRows rows={6} />
        ) : groups.length === 0 ? (
          <Empty icon={<History size={26} />} text={t.journal.empty} />
        ) : (
          groups.map((g) => (
            <div key={g.day}>
              <div className="list-head">{g.day}</div>
              <div className="list">
                {g.items.map((e) => (
                  <div key={e.id} className={`list-row journal-row ${e.reverted ? "is-reverted" : ""}`}>
                    <span className="journal-icon">{ICONS[e.kind] ?? <Cpu size={16} />}</span>
                    <div className="cell-main">
                      <div className="cell-title">{entryText(e, t, lang)}</div>
                      <div className="cell-sub">
                        {new Date(e.at * 1000).toLocaleTimeString(localeOf(lang), { hour: "2-digit", minute: "2-digit" })}
                      </div>
                    </div>
                    {e.reverted ? (
                      <span className="badge">{t.journal.reverted}</span>
                    ) : e.undo ? (
                      <button className="btn btn-secondary sm" disabled={busy !== null} onClick={() => void revert(e)}>
                        {busy === e.id ? <Spinner /> : <RotateCcw size={14} />}
                        {t.journal.undo}
                      </button>
                    ) : null}
                  </div>
                ))}
              </div>
            </div>
          ))
        )}
      </section>
    </>
  );
}
