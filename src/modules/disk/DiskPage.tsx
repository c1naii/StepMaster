import { motion } from "motion/react";
import { ArrowUp, ChevronRight, Copy, File, Folder, FolderOpen, HardDrive, Search, Trash2 } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { Bar, Empty, ErrorState, PageHeader, SkeletonRows, Spinner, Tabs, notify, stagger, useCalm } from "../../components/ui";
import { countOf, fmt, useLang, useT } from "../../i18n";
import { api, errorText, on, pickPath } from "../../lib/api";
import { baseName, formatBytes, parentPath, pct } from "../../lib/format";
import { usePoll } from "../../lib/hooks";
import { ask, useStore } from "../../lib/store";
import type { DiskEntry, DiskFolder, DuplicateGroup, ScanProgress } from "../../lib/types";
import { Downloads } from "./Downloads";

type Tab = "analyze" | "downloads" | "large" | "dupes";

async function confirmRecycle(paths: string[], t: ReturnType<typeof useT>, lang: "ru" | "en"): Promise<boolean> {
  return ask({
    title: t.disk.recycleTitle,
    text: fmt(t.disk.recycleText, { n: countOf(paths.length, t.common.items, lang) }),
    details: paths.slice(0, 50),
    confirmLabel: t.common.toRecycle,
    cancelLabel: t.common.cancel,
    danger: true,
  });
}

async function recycle(paths: string[], t: ReturnType<typeof useT>, lang: "ru" | "en"): Promise<boolean> {
  if (!(await confirmRecycle(paths, t, lang))) return false;
  try {
    const r = await api.recycle(paths);
    if (r.ok > 0) notify.ok(fmt(t.disk.recycled, { n: countOf(r.ok, t.common.items, lang) }));
    if (r.failed.length > 0) notify.err(fmt(t.disk.recycleFailed, { n: r.failed.length }) + "\n" + r.failed.slice(0, 3).join("\n"));
    return r.ok > 0;
  } catch (e) {
    notify.err(e);
    return false;
  }
}

/* ---------------- Analyze ---------------- */
function Analyze({ onScanned }: { onScanned: () => void }) {
  const t = useT();
  const lang = useLang();
  const calm = useCalm();
  const { data: snap, error: snapError } = usePoll(api.snapshot, 10_000);
  const [folder, setFolder] = useState<DiskFolder | null>(null);
  const [root, setRoot] = useState<string | null>(null);
  const [scanning, setScanning] = useState(false);
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loadingChild, setLoadingChild] = useState(false);
  const alive = useRef(true);

  useEffect(() => {
    alive.current = true;
    let off: (() => void) | undefined;
    void on<ScanProgress>("disk://progress", (p) => setProgress(p)).then((u) => (off = u));
    return () => {
      alive.current = false;
      off?.();
    };
  }, []);

  const scan = async (path: string) => {
    setScanning(true);
    setError(null);
    setProgress(null);
    setFolder(null);
    setRoot(path);
    try {
      const res = await api.diskScan(path);
      if (!alive.current) return;
      setFolder(res);
      onScanned();
    } catch (e) {
      if (!alive.current) return;
      setError(errorText(e));
    } finally {
      if (alive.current) setScanning(false);
    }
  };

  const open = async (path: string) => {
    setLoadingChild(true);
    try {
      const res = await api.diskChildren(path);
      if (alive.current) setFolder(res);
    } catch (e) {
      notify.err(e);
    } finally {
      if (alive.current) setLoadingChild(false);
    }
  };

  const atRoot = !folder || !root || folder.path.replace(/\\+$/, "").toLowerCase() === root.replace(/\\+$/, "").toLowerCase();
  const up = folder && !atRoot ? parentPath(folder.path) : null;
  const maxSize = folder?.entries[0]?.size ?? 1;

  return (
    <>
      {snapError && !snap ? <div className="notice warn">{t.common.loadFailed}: {snapError}</div> : null}
      {/* Drives */}
      <motion.div className="grid-3" variants={stagger.container} initial={calm ? false : "hidden"} animate="show">
        {(snap?.disks ?? []).map((d) => {
          const used = d.total - d.available;
          return (
            <motion.button key={d.mount} variants={stagger.item} className="card" disabled={scanning} onClick={() => void scan(d.mount)}>
              <div className="row">
                <HardDrive size={20} strokeWidth={1.6} />
                <span style={{ fontWeight: 600 }} className="truncate" title={`${d.mount} ${d.label}`}>
                  {d.mount.replace(/\\$/, "")} {d.label ? `· ${d.label}` : ""}
                </span>
                <span className="spacer" />
                <span className="badge">{d.kind}</span>
              </div>
              <div className="num" style={{ fontSize: 30, fontWeight: 300, marginTop: 14, letterSpacing: "-0.02em" }}>
                {fmt(t.disk.free, { size: formatBytes(d.available, lang) })}
              </div>
              <div className="card-meta num">{fmt(t.disk.of, { size: formatBytes(d.total, lang) })}</div>
              <div style={{ marginTop: 14 }}>
                <Bar value={pct(used, d.total)} thin />
              </div>
            </motion.button>
          );
        })}
        <button
          className="card"
          disabled={scanning}
          style={{ display: "grid", placeItems: "center", gap: 10, minHeight: 150, color: "var(--text-2)" }}
          onClick={async () => {
            const p = await pickPath({ directory: true });
            if (p) void scan(p);
          }}
        >
          <FolderOpen size={24} strokeWidth={1.5} />
          <span>{t.disk.pickFolder}</span>
        </button>
      </motion.div>

      <section className="card flush">
        {scanning ? (
          <div style={{ padding: 28, display: "grid", gap: 14 }}>
            <div className="row">
              <Spinner />
              <span>{fmt(t.disk.scanning, { n: countOf(progress?.files ?? 0, t.common.files, lang) })}</span>
              <span className="spacer" />
              <span className="num muted">{formatBytes(progress?.bytes ?? 0, lang)}</span>
              <button className="btn btn-secondary sm" onClick={() => void api.diskCancel()}>
                {t.disk.stop}
              </button>
            </div>
            <Bar value={0} thin indeterminate />
            <div className="small faint truncate">{progress?.current ?? root}</div>
          </div>
        ) : error ? (
          <ErrorState error={error} onRetry={root ? () => void scan(root) : undefined} />
        ) : !folder ? (
          <Empty icon={<HardDrive size={28} />} text={t.disk.emptyAnalyze} />
        ) : (
          <>
            <div className="list-head" style={{ textTransform: "none", letterSpacing: 0, fontSize: 13 }}>
              <button className="icon-btn" disabled={!up || loadingChild} aria-label={t.disk.up} onClick={() => up && void open(up)}>
                <ArrowUp size={16} />
              </button>
              <span className="truncate" style={{ flex: 1, color: "var(--text-2)", userSelect: "text" }}>
                {folder.path}
              </span>
              <span className="num">{formatBytes(folder.size, lang)}</span>
              {loadingChild ? <Spinner size={14} /> : null}
            </div>
            {folder.entries.length === 0 ? (
              <Empty text={t.disk.emptyFolder} />
            ) : (
              <div className="list">
                {folder.entries.map((e) => (
                  <div key={e.path} className="list-row hoverable" style={{ minHeight: 60 }}>
                    {e.isDir ? <Folder size={18} strokeWidth={1.6} /> : <File size={18} strokeWidth={1.6} className="faint" />}
                    <button
                      className="cell-main"
                      style={{ textAlign: "left", cursor: e.isDir ? "pointer" : "default" }}
                      disabled={!e.isDir || loadingChild}
                      onClick={() => e.isDir && void open(e.path)}
                    >
                      <div className="cell-title row" style={{ gap: 6 }}>
                        <span className="truncate" title={e.path}>{e.name}</span>
                        {e.isDir ? <ChevronRight size={14} className="faint" style={{ flexShrink: 0 }} /> : null}
                      </div>
                      <div style={{ marginTop: 8, maxWidth: 420 }}>
                        <Bar value={(e.size / Math.max(1, maxSize)) * 100} thin />
                      </div>
                    </button>
                    <div className="num" style={{ width: 90, textAlign: "right", fontWeight: 500 }}>
                      {formatBytes(e.size, lang)}
                    </div>
                    <div className="num faint small" style={{ width: 48, textAlign: "right" }}>
                      {pct(e.size, folder.size)}%
                    </div>
                    <button className="icon-btn" aria-label={t.common.showInExplorer} onClick={() => api.reveal(e.path).catch(notify.err)}>
                      <FolderOpen size={16} />
                    </button>
                    <button
                      className="icon-btn"
                      aria-label={t.common.toRecycle}
                      onClick={async () => {
                        if (await recycle([e.path], t, lang)) void open(folder.path);
                      }}
                    >
                      <Trash2 size={16} />
                    </button>
                  </div>
                ))}
              </div>
            )}
          </>
        )}
      </section>
    </>
  );
}

/* ---------------- Large files ---------------- */
function LargeFiles({ version }: { version: number }) {
  const t = useT();
  const lang = useLang();
  const [files, setFiles] = useState<DiskEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = () => {
    setError(null);
    api.diskLargeFiles().then(setFiles, (e) => setError(errorText(e)));
  };
  useEffect(load, [version]);

  if (error) return <div className="card"><ErrorState error={error} onRetry={load} /></div>;
  if (!files) return <div className="card flush"><SkeletonRows /></div>;
  if (files.length === 0) return <div className="card"><Empty icon={<Search size={26} />} text={t.disk.largeEmpty} /></div>;

  return (
    <section className="card flush">
      <div className="list">
        {files.map((f) => (
          <div key={f.path} className="list-row hoverable">
            <File size={18} strokeWidth={1.6} className="faint" />
            <div className="cell-main">
              <div className="cell-title">{f.name}</div>
              <div className="cell-sub" style={{ userSelect: "text" }}>{f.path}</div>
            </div>
            <div className="num" style={{ fontWeight: 500, whiteSpace: "nowrap" }}>{formatBytes(f.size, lang)}</div>
            <button className="icon-btn" aria-label={t.common.showInExplorer} onClick={() => api.reveal(f.path).catch(notify.err)}>
              <FolderOpen size={16} />
            </button>
            <button
              className="icon-btn"
              aria-label={t.common.toRecycle}
              onClick={async () => {
                if (await recycle([f.path], t, lang)) setFiles((list) => (list ?? []).filter((x) => x.path !== f.path));
              }}
            >
              <Trash2 size={16} />
            </button>
          </div>
        ))}
      </div>
    </section>
  );
}

/* ---------------- Duplicates ---------------- */
function Duplicates() {
  const t = useT();
  const lang = useLang();
  const [path, setPath] = useState<string | null>(null);
  const [groups, setGroups] = useState<DuplicateGroup[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const run = async (p: string) => {
    setPath(p);
    setBusy(true);
    setError(null);
    setGroups(null);
    try {
      const g = await api.duplicatesScan(p);
      if (alive.current) setGroups(g);
    } catch (e) {
      if (alive.current) setError(errorText(e));
    } finally {
      if (alive.current) setBusy(false);
    }
  };

  const savings = useMemo(() => (groups ?? []).reduce((s, g) => s + g.size * (g.files.length - 1), 0), [groups]);

  return (
    <>
      <section className="card row" style={{ gap: 20 }}>
        <Copy size={22} strokeWidth={1.6} />
        <div className="cell-main">
          <div style={{ fontWeight: 500 }}>{path ?? t.disk.dupesScan}</div>
          <div className="small muted">{t.disk.dupesHint}</div>
        </div>
        <button
          className="btn btn-primary"
          disabled={busy}
          onClick={async () => {
            const p = await pickPath({ directory: true });
            if (p) void run(p);
          }}
        >
          {busy ? <Spinner /> : <FolderOpen size={16} />}
          {busy ? t.disk.dupesScanning : t.disk.pickFolder}
        </button>
      </section>

      {error ? <div className="card"><ErrorState error={error} onRetry={path ? () => void run(path) : undefined} /></div> : null}
      {busy ? <div className="card flush"><SkeletonRows rows={4} /></div> : null}
      {groups && groups.length === 0 ? <div className="card"><Empty text={t.disk.dupesNone} /></div> : null}
      {groups && groups.length > 0 ? (
        <>
          <div className="muted">{fmt(t.disk.dupesFound, { n: groups.length, size: formatBytes(savings, lang) })}</div>
          {groups.map((g) => (
            <section key={g.files.join("|")} className="card flush">
              <div className="list-head" style={{ textTransform: "none", letterSpacing: 0, fontSize: 13 }}>
                <span style={{ flex: 1 }} className="truncate">{baseName(g.files[0])}</span>
                <span className="num">{formatBytes(g.size, lang)} × {g.files.length}</span>
                <button
                  className="btn btn-secondary sm"
                  onClick={async () => {
                    const rest = g.files.slice(1);
                    if (await recycle(rest, t, lang)) setGroups((gs) => (gs ?? []).filter((x) => x !== g));
                  }}
                >
                  {t.disk.keepFirst}
                </button>
              </div>
              <div className="list">
                {g.files.map((f, i) => (
                  <div key={f} className="list-row">
                    <span className={`badge ${i === 0 ? "solid" : ""}`}>{i + 1}</span>
                    <div className="cell-main cell-sub" style={{ color: "var(--text-2)", userSelect: "text" }}>{f}</div>
                    <button className="icon-btn" aria-label={t.common.showInExplorer} onClick={() => api.reveal(f).catch(notify.err)}>
                      <FolderOpen size={16} />
                    </button>
                  </div>
                ))}
              </div>
            </section>
          ))}
        </>
      ) : null}
    </>
  );
}

export function DiskPage() {
  const t = useT();
  const [tab, setTab] = useState<Tab>(() => {
    const p = useStore.getState().takePendingTab();
    return p === "large" || p === "dupes" || p === "downloads" ? p : "analyze";
  });
  const [scanVersion, setScanVersion] = useState(0);

  return (
    <>
      <PageHeader title={t.disk.title} sub={t.disk.sub} />
      <Tabs
        value={tab}
        onChange={setTab}
        items={[
          { id: "analyze", label: t.disk.tabAnalyze },
          { id: "downloads", label: t.disk.tabDownloads },
          { id: "large", label: t.disk.tabLarge },
          { id: "dupes", label: t.disk.tabDupes },
        ]}
      />
      {/* Analyze stays mounted so its scan isn't lost when switching tabs. */}
      <div style={{ display: tab === "analyze" ? "contents" : "none" }}>
        <Analyze onScanned={() => setScanVersion((v) => v + 1)} />
      </div>
      {tab === "downloads" ? <Downloads /> : null}
      {tab === "large" ? <LargeFiles version={scanVersion} /> : null}
      {tab === "dupes" ? <Duplicates /> : null}
    </>
  );
}
