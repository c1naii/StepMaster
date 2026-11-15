import { FolderOpen, LayoutGrid, RotateCw, Trash2 } from "lucide-react";
import { useMemo, useState } from "react";
import { Empty, ErrorState, PageHeader, SearchInput, SkeletonRows, Spinner, Tabs, notify } from "../../components/ui";
import { countOf, fmt, useLang, useT } from "../../i18n";
import { api } from "../../lib/api";
import { AppIcon } from "../../components/AppIcon";
import { formatBytes } from "../../lib/format";
import { useAsync } from "../../lib/hooks";
import { ask, useStore } from "../../lib/store";
import type { ProgramInfo, ProgramKind } from "../../lib/types";

type Filter = "all" | ProgramKind;
type Sort = "name" | "size" | "date";

export function ProgramsPage() {
  const t = useT();
  const lang = useLang();
  const list = useAsync(api.programs);
  const [q, setQ] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const [sort, setSort] = useState<Sort>("name");
  const [busyId, setBusyId] = useState<string | null>(null);
  const isAdmin = useStore((s) => s.app?.isAdmin ?? true);

  const items = useMemo(() => {
    const needle = q.trim().toLowerCase();
    let arr = (list.data ?? []).filter((p) => filter === "all" || p.kind === filter);
    if (needle) arr = arr.filter((p) => `${p.name} ${p.publisher}`.toLowerCase().includes(needle));
    const collator = new Intl.Collator(lang === "ru" ? "ru" : "en", { sensitivity: "base", numeric: true });
    return [...arr].sort((a, b) => {
      if (sort === "size") return b.size - a.size || collator.compare(a.name, b.name);
      if (sort === "date") return (b.installDate || "").localeCompare(a.installDate || "") || collator.compare(a.name, b.name);
      return collator.compare(a.name, b.name);
    });
  }, [list.data, q, filter, sort, lang]);

  const counts = useMemo(() => {
    const c = { all: 0, win32: 0, store: 0, portable: 0 };
    for (const p of list.data ?? []) {
      c.all++;
      c[p.kind]++;
    }
    return c;
  }, [list.data]);

  const uninstall = async (p: ProgramInfo) => {
    const text = p.kind === "portable" ? t.programs.uninstallPortable : p.kind === "store" ? t.programs.uninstallStore : t.programs.uninstallText;
    const ok = await ask({
      title: fmt(t.programs.uninstallTitle, { name: p.name }),
      text,
      confirmLabel: t.programs.uninstall,
      cancelLabel: t.common.cancel,
      danger: true,
    });
    if (!ok) return;
    setBusyId(p.id);
    try {
      const res = await api.programUninstall(p.id);
      if (!res.finished) {
        notify.info(t.programs.notFinished);
      } else {
        notify.ok(fmt(t.programs.removed, { name: p.name }));
        if (res.leftovers.length > 0) {
          const clean = await ask({
            title: t.programs.leftoversTitle,
            text: t.programs.leftoversText,
            details: res.leftovers,
            confirmLabel: t.common.toRecycle,
            cancelLabel: t.common.later,
          });
          if (clean) {
            const r = await api.recycle(res.leftovers);
            if (r.failed.length) notify.err(fmt(t.disk.recycleFailed, { n: r.failed.length }));
            else notify.ok(fmt(t.disk.recycled, { n: countOf(r.ok, t.common.items, lang) }));
          }
        }
      }
    } catch (e) {
      notify.err(e);
    } finally {
      setBusyId(null);
      void list.reload();
    }
  };

  return (
    <>
      <PageHeader
        title={t.programs.title}
        sub={t.programs.sub}
        right={
          <button className="btn btn-secondary" disabled={list.loading} onClick={() => void list.reload()}>
            <RotateCw size={15} />
            {t.common.refresh}
          </button>
        }
      />

      <div className="row wrap" style={{ gap: 12 }}>
        <Tabs
          value={filter}
          onChange={setFilter}
          items={[
            { id: "all", label: `${t.programs.fAll} ${counts.all || ""}`.trim() },
            { id: "win32", label: `${t.programs.fWin32} ${counts.win32 || ""}`.trim() },
            { id: "store", label: `${t.programs.fStore} ${counts.store || ""}`.trim() },
            { id: "portable", label: `${t.programs.fPortable} ${counts.portable || ""}`.trim() },
          ]}
        />
        <span className="spacer" />
        <select className="select" value={sort} onChange={(e) => setSort(e.target.value as Sort)} aria-label="sort">
          <option value="name">{t.programs.sortName}</option>
          <option value="size">{t.programs.sortSize}</option>
          <option value="date">{t.programs.sortDate}</option>
        </select>
        <SearchInput value={q} onChange={setQ} placeholder={t.programs.search} />
      </div>

      {filter === "portable" ? <div className="notice">{t.programs.portableNote}</div> : null}

      <section className="card flush">
        {list.error ? (
          <ErrorState error={list.error} onRetry={() => void list.reload()} />
        ) : list.loading && !list.data ? (
          <SkeletonRows rows={8} />
        ) : items.length === 0 ? (
          <Empty icon={<LayoutGrid size={26} />} text={t.programs.nothingFound} />
        ) : (
          <div className="list">
            {items.map((p) => (
              <div key={p.id} className="list-row hoverable">
                <AppIcon spec={p.icon} name={p.name} />
                <div className="cell-main">
                  <div className="cell-title" title={p.name}>{p.name}</div>
                  <div className="cell-sub" title={[p.publisher, p.version].filter(Boolean).join(" · ")}>
                    {[p.publisher, p.version].filter(Boolean).join(" · ") || "—"}
                  </div>
                </div>
                {p.protected ? <span className="badge">{t.programs.protected}</span> : null}
                <span className="badge" style={{ minWidth: 0 }}>
                  {p.kind === "store" ? t.programs.kindStore : p.kind === "portable" ? t.programs.kindPortable : t.programs.kindWin32}
                </span>
                <div className="num muted small" style={{ width: 86, textAlign: "right" }}>
                  {p.installDate || "—"}
                </div>
                <div className="num" style={{ width: 80, textAlign: "right", fontWeight: 500 }}>
                  {p.size > 0 ? formatBytes(p.size, lang) : "—"}
                </div>
                <button
                  className="icon-btn"
                  aria-label={t.common.showInExplorer}
                  disabled={!p.location}
                  style={{ opacity: p.location ? 1 : 0.3 }}
                  onClick={() => api.reveal(p.location).catch(notify.err)}
                >
                  <FolderOpen size={16} />
                </button>
                <button
                  className="btn btn-secondary sm"
                  style={{ width: 104 }}
                  title={!isAdmin ? t.common.needsAdmin : undefined}
                  disabled={!p.canUninstall || busyId !== null || !isAdmin}
                  onClick={() => void uninstall(p)}
                >
                  {busyId === p.id ? <Spinner size={14} /> : <Trash2 size={14} />}
                  {t.programs.uninstall}
                </button>
              </div>
            ))}
          </div>
        )}
      </section>
    </>
  );
}
