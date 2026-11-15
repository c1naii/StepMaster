import { FolderOpen, RotateCw, Sparkles, Trash2 } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { AppIcon } from "../../components/AppIcon";
import { Checkbox, Empty, ErrorState, SkeletonRows, Tabs, notify } from "../../components/ui";
import { countOf, fmt, useLang, useT } from "../../i18n";
import { api, errorText } from "../../lib/api";
import { formatBytes } from "../../lib/format";
import type { DownloadCategory, DownloadItem, DownloadsReport } from "../../lib/types";
import { recycle } from "./recycle";

type Place = "downloads" | "desktop";
type Filter = "recommended" | "all" | "old";

const RECOMMENDED = new Set(["installed", "extracted", "duplicate", "partial"]);
const isRecommended = (i: DownloadItem) => i.flags.some((f) => RECOMMENDED.has(f));

// Keep the last scan per folder while switching tabs/sections.
const cache = new Map<Place, DownloadsReport>();

export function Downloads() {
  const t = useT();
  const lang = useLang();
  const [place, setPlace] = useState<Place>("downloads");
  const [report, setReport] = useState<DownloadsReport | null>(cache.get("downloads") ?? null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<Filter>("recommended");
  const [category, setCategory] = useState<DownloadCategory | "any">("any");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const seq = useRef(0);

  const scan = async (p: Place, force = false) => {
    const my = ++seq.current;
    setError(null);
    setSelected(new Set());
    if (!force && cache.has(p)) {
      setReport(cache.get(p)!);
      return;
    }
    setLoading(true);
    try {
      const r = await api.downloadsScan(p);
      cache.set(p, r);
      if (my === seq.current) setReport(r);
    } catch (e) {
      if (my === seq.current) setError(errorText(e));
    } finally {
      if (my === seq.current) setLoading(false);
    }
  };

  useEffect(() => {
    void scan(place);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [place]);

  const items = report?.items ?? [];
  const recommended = useMemo(() => items.filter(isRecommended), [items]);
  const old = useMemo(() => items.filter((i) => i.flags.includes("old")), [items]);
  const sum = (list: DownloadItem[]) => list.reduce((s, i) => s + i.size, 0);
  const categories = useMemo(() => [...new Set(items.map((i) => i.category))], [items]);

  const visible = useMemo(() => {
    let list = filter === "recommended" ? recommended : filter === "old" ? old : items;
    if (category !== "any") list = list.filter((i) => i.category === category);
    return list;
  }, [filter, category, items, recommended, old]);

  // Only what is on screen can be selected, so nothing hidden is ever deleted.
  const shown = visible.slice(0, 500);
  const selectedItems = shown.filter((i) => selected.has(i.path));
  const allSelected = shown.length > 0 && selectedItems.length === shown.length;

  const reason = (i: DownloadItem) => {
    const parts: string[] = [];
    for (const f of i.flags) {
      if (f === "installed") parts.push(fmt(t.disk.dlInstalled, { name: i.installedAs ?? "" }));
      else if (f === "extracted") parts.push(t.disk.dlExtracted);
      else if (f === "duplicate") parts.push(t.disk.dlDuplicate);
      else if (f === "partial") parts.push(t.disk.dlPartial);
    }
    return parts;
  };

  const doRecycle = async () => {
    const paths = selectedItems.map((i) => i.path);
    const done = await recycle(paths, t, lang);
    if (done.length === 0) return;
    const gone = new Set(done);
    const next = { ...(report as DownloadsReport) };
    next.items = next.items.filter((i) => !gone.has(i.path));
    next.total = next.items.reduce((s, i) => s + i.size, 0);
    cache.set(place, next);
    setReport(next);
    setSelected(new Set());
  };

  const toggle = (path: string, v: boolean) =>
    setSelected((s) => {
      const n = new Set(s);
      if (v) n.add(path);
      else n.delete(path);
      return n;
    });

  return (
    <>
      <div className="row wrap" style={{ gap: 12 }}>
        <Tabs
          value={place}
          onChange={(p) => {
            setPlace(p);
            setCategory("any");
          }}
          items={[
            { id: "downloads", label: t.disk.dlDownloads },
            { id: "desktop", label: t.disk.dlDesktop },
          ]}
        />
        <span className="spacer" />
        {report ? (
          <button className="btn btn-ghost sm" title={report.root} onClick={() => api.reveal(report.root).catch(notify.err)}>
            <FolderOpen size={14} />
            <span className="truncate" style={{ maxWidth: 280 }}>{report.root}</span>
          </button>
        ) : null}
        <button className="btn btn-secondary sm" disabled={loading} onClick={() => void scan(place, true)}>
          <RotateCw size={14} />
          {t.common.refresh}
        </button>
      </div>

      {/* Summary */}
      <div className="grid-3">
        <div className="card tight">
          <div className="card-label">{t.disk.dlTotal}</div>
          <div className="card-value num" style={{ fontSize: 30 }}>{report ? formatBytes(report.total, lang) : "—"}</div>
          <div className="card-meta">{report ? countOf(items.length, t.common.items, lang) : "…"}</div>
        </div>
        <button className="card tight" onClick={() => setFilter("recommended")} title={t.disk.dlCanFreeHint}>
          <div className="card-label row" style={{ gap: 6 }}>
            <Sparkles size={14} />
            {t.disk.dlCanFree}
          </div>
          <div className="card-value num" style={{ fontSize: 30 }}>{report ? formatBytes(sum(recommended), lang) : "—"}</div>
          <div className="card-meta">{report ? countOf(recommended.length, t.common.items, lang) : "…"}</div>
        </button>
        <button className="card tight" onClick={() => setFilter("old")}>
          <div className="card-label">{t.disk.dlOld}</div>
          <div className="card-value num" style={{ fontSize: 30 }}>{report ? formatBytes(sum(old), lang) : "—"}</div>
          <div className="card-meta">{report ? countOf(old.length, t.common.items, lang) : "…"}</div>
        </button>
      </div>

      <div className="row wrap" style={{ gap: 12 }}>
        <Tabs
          value={filter}
          onChange={(f) => {
            setFilter(f);
            setSelected(new Set());
          }}
          items={[
            { id: "recommended", label: t.disk.dlFilterRecommended },
            { id: "all", label: t.disk.dlFilterAll },
            { id: "old", label: t.disk.dlFilterOld },
          ]}
        />
        <select className="select" style={{ height: 36 }} value={category} onChange={(e) => setCategory(e.target.value as DownloadCategory | "any")} aria-label="category">
          <option value="any">{t.common.all}</option>
          {categories.map((c) => (
            <option key={c} value={c}>
              {t.disk.cat[c]}
            </option>
          ))}
        </select>
        <span className="spacer" />
        <button className="btn btn-ghost sm" disabled={shown.length === 0} onClick={() => setSelected(allSelected ? new Set() : new Set(shown.map((i) => i.path)))}>
          {allSelected ? t.disk.dlSelectNone : t.disk.dlSelectAll}
        </button>
        <button className="btn btn-danger sm" disabled={selectedItems.length === 0} onClick={() => void doRecycle()}>
          <Trash2 size={14} />
          {fmt(t.disk.dlRecycle, { n: selectedItems.length, size: formatBytes(sum(selectedItems), lang) })}
        </button>
      </div>

      <section className="card flush">
        {error ? (
          <ErrorState error={error} onRetry={() => void scan(place, true)} />
        ) : loading && !report ? (
          <SkeletonRows rows={7} />
        ) : items.length === 0 ? (
          <Empty icon={<FolderOpen size={26} />} text={t.disk.dlEmpty} />
        ) : visible.length === 0 ? (
          <Empty text={t.disk.dlNothing} />
        ) : (
          <div className="list">
            {loading ? <div className="small faint" style={{ padding: "10px 20px" }}>{t.disk.dlScanning}</div> : null}
            {shown.map((i) => {
              const why = reason(i);
              return (
                <div key={i.path} className="list-row hoverable">
                  <Checkbox checked={selected.has(i.path)} onChange={(v) => toggle(i.path, v)} label={i.name} />
                  <AppIcon spec={i.path} name={i.name} size={34} />
                  <div className="cell-main">
                    <div className="cell-title" title={i.path}>{i.name}</div>
                    <div className="cell-sub">
                      {t.disk.cat[i.category]}
                      {" · "}
                      {i.ageDays === 0 ? t.disk.dlToday : fmt(t.disk.dlAge, { n: i.ageDays })}
                      {why.length > 0 ? <span style={{ color: "var(--text-2)" }}>{" · " + why.join(" · ")}</span> : null}
                    </div>
                  </div>
                  <div className="num" style={{ width: 90, textAlign: "right", fontWeight: 500 }}>{formatBytes(i.size, lang)}</div>
                  <button className="icon-btn" aria-label={t.common.showInExplorer} onClick={() => api.reveal(i.path).catch(notify.err)}>
                    <FolderOpen size={16} />
                  </button>
                </div>
              );
            })}
          </div>
        )}
      </section>
    </>
  );
}
