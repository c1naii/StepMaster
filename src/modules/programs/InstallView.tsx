import { AnimatePresence, motion } from "motion/react";
import { AlertTriangle, Check, Download, ExternalLink, Package, PackageOpen, Search, Trash2 } from "lucide-react";
import { useEffect, useMemo, useReducer, useRef, useState } from "react";
import { Empty, ErrorState, SearchInput, SkeletonRows, Spinner, Tabs, notify, useCalm } from "../../components/ui";
import { dicts, fmt, useLang, useT } from "../../i18n";
import { api, on } from "../../lib/api";
import { formatBytes } from "../../lib/format";
import { useAsync } from "../../lib/hooks";
import { useStore } from "../../lib/store";
import type { ProgramInfo, WingetHit, WingetProgress, WingetStep } from "../../lib/types";
import { CATALOG, CATEGORIES, type CatalogApp, type CatalogCategory } from "./catalog";

type Item = { state: WingetStep; note: string | null; percent?: number | null; done?: number | null; total?: number | null };
/** What gets installed: a catalog app or a package found in the winget catalog. */
export type Pick = { key: string; name: string; ids: string[] };

/** Whole words, so "Git" doesn't match "GitHub" and "Steam" matches "Steam". */
export function nameRegex(name: string) {
  return new RegExp(`(^|[^\\w])${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}($|[^\\w])`, "i");
}

/** One entry may install several packages: show the "worst" state of them. */
export function aggregate(app: Pick, progress: Record<string, Item>): Item | null {
  const parts = app.ids.map((id) => progress[id]).filter(Boolean);
  if (parts.length === 0) return null;
  const failed = parts.find((p) => p.state === "failed");
  if (failed) return failed;
  const running = parts.find((p) => p.state === "running");
  if (running) return { state: "running", note: null, percent: running.percent, done: running.done, total: running.total };
  if (parts.length === app.ids.length && parts.every((p) => p.state === "done")) {
    return { state: "done", note: parts.some((p) => p.note === "reboot") ? "reboot" : null };
  }
  return { state: parts.some((p) => p.state === "done") ? "running" : "queued", note: null };
}

/* The install queue keeps running in the backend when the user leaves the page, so its state
   and event handlers live here, outside the component, and a remounted view picks them up. */
type Session = {
  picked: Map<string, Pick>;
  progress: Record<string, Item>;
  busy: boolean;
  cancelling: boolean;
  /** Packages installed during this run: counted as installed before winget's list catches up. */
  installedHere: Set<string>;
  batch: Pick[];
  runStart: Record<string, number>;
  /** Bar value at the moment "Cancel" was pressed: the bar stops there instead of creeping on. */
  frozen: number;
};
const session: Session = { picked: new Map(), progress: {}, busy: false, cancelling: false, installedHere: new Set(), batch: [], runStart: {}, frozen: 0 };
const sessionViews = new Set<() => void>();
/** Set by the mounted view: reloads winget's and Windows' lists after a run. */
let afterRun: (() => void) | null = null;
let listening = false;

function listenOnce() {
  if (listening) return;
  listening = true;
  const redraw = () => sessionViews.forEach((f) => f());
  void on<WingetProgress>("winget://progress", (p) => {
    const prev = session.progress[p.id];
    session.progress = {
      ...session.progress,
      [p.id]: { state: p.state, note: p.note, percent: p.percent ?? prev?.percent ?? null, done: p.done ?? prev?.done ?? null, total: p.total ?? prev?.total ?? null },
    };
    if (p.state === "running" && !session.runStart[p.id]) session.runStart[p.id] = Date.now();
    if (p.state === "done") session.installedHere = new Set(session.installedHere).add(p.id);
    redraw();
  });
  void on("winget://finished", () => {
    const t = dicts[useStore.getState().settings.language];
    const states = session.batch.map((a) => [a, aggregate(a, session.progress)] as const);
    const failed = states.filter(([, s]) => s?.state === "failed" && s.note !== "cancelled").map(([a]) => a.name);
    if (states.some(([, s]) => s?.note === "cancelled")) notify.info(t.programs.iCancelledToast);
    else if (failed.length) notify.err(fmt(t.programs.iFinishedFail, { names: failed.join(", ") }));
    else notify.ok(fmt(t.programs.iFinished, { ok: states.filter(([, s]) => s?.state === "done").length, n: session.batch.length }));
    session.busy = false;
    session.cancelling = false;
    session.picked = new Map();
    afterRun?.();
    redraw();
  });
}

/** useState backed by the module-level session. */
function useSession<K extends keyof Session>(key: K): [Session[K], (v: Session[K] | ((cur: Session[K]) => Session[K])) => void] {
  const [, force] = useReducer((x: number) => x + 1, 0);
  useEffect(() => {
    sessionViews.add(force);
    return () => void sessionViews.delete(force);
  }, []);
  const set = (v: Session[K] | ((cur: Session[K]) => Session[K])) => {
    session[key] = typeof v === "function" ? (v as (cur: Session[K]) => Session[K])(session[key]) : v;
    sessionViews.forEach((f) => f());
  };
  return [session[key], set];
}

type CatKey = "cBrowsers" | "cChat" | "cGames" | "cDrivers" | "cComponents" | "cMedia" | "cGraphics" | "cDocs" | "cUtils" | "cHardware" | "cDev";
const CAT_LABEL: Record<CatalogCategory, CatKey> = {
  browsers: "cBrowsers",
  chat: "cChat",
  games: "cGames",
  drivers: "cDrivers",
  components: "cComponents",
  media: "cMedia",
  graphics: "cGraphics",
  docs: "cDocs",
  utils: "cUtils",
  hardware: "cHardware",
  dev: "cDev",
};

export function InstallView({
  programs,
  onInstalled,
  onUninstall,
  onFind,
  uninstalling,
}: {
  programs: ProgramInfo[] | null;
  onInstalled: () => void;
  onUninstall: (p: ProgramInfo) => void;
  onFind: (name: string) => void;
  uninstalling: string | null;
}) {
  const t = useT();
  const lang = useLang();
  const calm = useCalm();
  const isAdmin = useStore((s) => s.app?.isAdmin ?? true);
  const wg = useAsync(api.wingetState);
  const [q, setQ] = useState("");
  const [picked, setPicked] = useSession("picked");
  const [progress, setProgress] = useSession("progress");
  const [busy, setBusy] = useSession("busy");
  const [hits, setHits] = useState<{ q: string; list: WingetHit[] } | null>(null);
  const [searching, setSearching] = useState(false);
  const [kind, setKind] = useState<"all" | "direct" | "site">("all");
  const [installedHere, setInstalledHere] = useSession("installedHere");
  const [cancelling, setCancelling] = useSession("cancelling");
  const [, setTick] = useState(0);
  // Redraw twice a second while installing, so the bar keeps creeping even when winget gives no percent.
  useEffect(() => {
    if (!busy) return;
    const timer = setInterval(() => setTick((n) => n + 1), 500);
    return () => clearInterval(timer);
  }, [busy]);

  // Installed by winget's list, plus what was just installed here (no need to wait for a re-scan).
  const installedIds = useMemo(() => {
    const ids = new Set(wg.data?.installed ?? []);
    for (const [id, p] of Object.entries(progress)) if (p.state === "done") ids.add(id);
    installedHere.forEach((id) => ids.add(id));
    return ids;
  }, [wg.data, progress, installedHere]);

  // Installed = present in Windows' installed list (what "My programs" shows). winget alone isn't
  // trusted for apps: it keeps listing packages it installed even after they were removed elsewhere.
  // Components (Visual C++, .NET…) have no usable entry there, so for them winget decides.
  const owned = useMemo(() => {
    const res = new Map<string, ProgramInfo | null>();
    for (const a of CATALOG) {
      if (a.url) continue;
      const viaWinget = a.ids.every((id) => installedIds.has(id));
      const thisVisit = a.ids.every((id) => installedHere.has(id));
      if (a.cat === "components" || a.byWinget || !programs) {
        if (viaWinget) res.set(a.key, null);
        continue;
      }
      const names = [a.name, ...(a.match ?? [])].map(nameRegex);
      const prog = programs.find((p) => names.some((re) => re.test(p.name))) ?? null;
      if (prog || thisVisit) res.set(a.key, prog);
    }
    return res;
  }, [installedIds, installedHere, programs]);

  // Lists to refresh after a run, from whichever view is mounted at that moment.
  const live = useRef({ reload: wg.reload, onInstalled });
  live.current = { reload: wg.reload, onInstalled };
  useEffect(() => {
    listenOnce();
    const mine = () => {
      void live.current.reload();
      live.current.onInstalled();
    };
    afterRun = mine;
    return () => {
      if (afterRun === mine) afterRun = null;
    };
  }, []);

  // The programs list was reloaded (install / uninstall finished): re-read winget's list too.
  const firstPrograms = useRef(true);
  useEffect(() => {
    if (firstPrograms.current) {
      firstPrograms.current = false;
      return;
    }
    if (!busy) {
      // Windows' list changed (maybe something was uninstalled elsewhere): from now on it decides,
      // "installed during this run" no longer overrides it.
      setInstalledHere(new Set());
      void wg.reload();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [programs]);

  const groups = useMemo(() => {
    const needle = q.trim().toLowerCase();
    const list = CATALOG.filter(
      (a) => (kind === "all" || (kind === "site") === !!a.url) && (!needle || `${a.name} ${a.ru} ${a.en}`.toLowerCase().includes(needle)),
    );
    return CATEGORIES.map((c) => ({ cat: c, apps: list.filter((a) => a.cat === c) })).filter((g) => g.apps.length > 0);
  }, [q, kind]);

  // Catalog apps: Windows' list decides (see `owned`). Search results: winget's list.
  const canPick = (key: string, ids: string[]) =>
    !busy && ids.length > 0 && (key.startsWith("w:") ? !ids.every((id) => installedIds.has(id)) : !owned.has(key));

  const toggle = (p: Pick) => {
    if (!canPick(p.key, p.ids)) return;
    setPicked((cur) => {
      const next = new Map(cur);
      if (next.has(p.key)) next.delete(p.key);
      else next.set(p.key, p);
      return next;
    });
  };

  const searchAll = async () => {
    const query = q.trim();
    if (query.length < 2) return;
    setSearching(true);
    try {
      setHits({ q: query, list: await api.wingetSearch(query) });
    } catch (e) {
      notify.err(e);
    } finally {
      setSearching(false);
    }
  };

  const start = async () => {
    const apps = [...picked.values()];
    if (!apps.length) return;
    session.batch = apps;
    session.runStart = {};
    setCancelling(false);
    setProgress({});
    setBusy(true);
    try {
      await api.wingetInstall(apps.flatMap((a) => a.ids));
    } catch (e) {
      setBusy(false);
      notify.err(e);
    }
  };

  const remove = (a: CatalogApp) => {
    // Forget "just installed" for it: after uninstalling, the fresh winget list decides.
    setProgress((cur) => {
      const next = { ...cur };
      for (const id of a.ids) delete next[id];
      return next;
    });
    setInstalledHere((cur) => {
      const next = new Set(cur);
      a.ids.forEach((id) => next.delete(id));
      return next;
    });
    const prog = owned.get(a.key);
    if (prog) onUninstall(prog);
    else onFind(a.name);
  };

  if (wg.error) return <ErrorState error={wg.error} onRetry={() => void wg.reload()} />;
  if (wg.loading && !wg.data)
    return (
      <section className="card flush">
        <SkeletonRows rows={8} />
      </section>
    );

  if (wg.data && !wg.data.available) {
    return (
      <div className="notice warn">
        <AlertTriangle size={16} />
        <span style={{ flex: 1 }}>{t.programs.iNoWinget}</span>
        <button className="btn btn-secondary sm" onClick={() => api.openTool("appinstaller").catch(notify.err)}>
          {t.programs.iGetWinget}
        </button>
      </div>
    );
  }

  const stateLabel = (s: Item) =>
    s.state === "queued"
      ? t.programs.iQueued
      : s.state === "running"
        ? s.total && s.done != null && s.percent != null && s.percent < 100
          ? fmt(t.programs.iSize, { a: formatBytes(s.done, lang), b: formatBytes(s.total, lang) })
          : s.percent != null && s.percent < 100
            ? fmt(t.programs.iDownloading, { p: s.percent })
            : t.programs.iRunning
        : s.state === "failed"
          ? s.note === "cancelled"
            ? t.programs.iCancelled
            : t.programs.iFailed
          : s.note === "reboot"
            ? t.programs.iDoneReboot
            : t.programs.iDone;

  // Overall install progress for the bar: finished packages + the running one's download share.
  const liveValue = (() => {
    const ids = session.batch.flatMap((a) => a.ids);
    if (!busy || ids.length === 0) return 0;
    const sum = ids.reduce((acc, id) => {
      const p = progress[id];
      if (!p) return acc;
      if (p.state === "done" || p.state === "failed") return acc + 1;
      if (p.state === "running") {
        // Real download percent when known, otherwise a slow creep that never quite reaches the end.
        const frac = p.percent != null ? p.percent / 100 : 1 - Math.exp(-(Date.now() - (session.runStart[id] ?? Date.now())) / 40000);
        return acc + frac * 0.9;
      }
      return acc;
    }, 0);
    return Math.min(100, (sum / ids.length) * 100);
  })();

  const overall = cancelling ? session.frozen : liveValue;

  // "Now: Telegram (2 of 5)": the package being installed right now in the queue.
  const nowText = (() => {
    const apps = session.batch;
    if (cancelling) return t.programs.iCancelling;
    const idx = apps.findIndex((a) => aggregate(a, progress)?.state === "running");
    return idx >= 0 ? fmt(t.programs.iNow, { name: apps[idx].name, i: idx + 1, n: apps.length }) : t.programs.iBusy;
  })();

  const card = (p: Pick, icon: React.ReactNode, desc: string, opts: { url?: string; catalog?: CatalogApp } = {}) => {
    const raw = aggregate(p, progress);
    // Freshly installed = installed: show the trash right away (keep "restart needed" visible).
    const justDone = raw?.state === "done" && raw.note !== "reboot";
    const st = justDone ? null : raw;
    const isOwned = justDone || (opts.catalog ? owned.has(p.key) : p.ids.every((id) => installedIds.has(id)));
    const sel = picked.has(p.key);
    const onClick = opts.url ? () => api.openSite(opts.url!).catch(notify.err) : () => toggle(p);
    return (
      <div key={p.key} className="install-wrap">
        <button
          type="button"
          className="install-card"
          data-on={sel || undefined}
          data-installed={isOwned || undefined}
          disabled={!opts.url && !st && !canPick(p.key, p.ids) && !sel}
          onClick={onClick}
          aria-pressed={opts.url ? undefined : sel}
          title={st?.state === "failed" && st.note && st.note !== "cancelled" ? st.note : opts.url ? t.programs.iSiteHint : undefined}
        >
          {icon}
          <span className="install-text">
            <span className="install-name">{p.name}</span>
            <span className="install-desc">{st ? stateLabel(st) : isOwned ? t.programs.iInstalled : opts.url ? t.programs.iSite : desc}</span>
          </span>
          <span className="install-mark">
            {st?.state === "running" ? (
              <Spinner size={16} />
            ) : st?.state === "failed" ? (
              <AlertTriangle size={16} color="var(--warn)" />
            ) : st?.state === "done" ? (
              <Check size={16} />
            ) : opts.url ? (
              <ExternalLink size={15} color="var(--text-3)" />
            ) : isOwned ? null : (
              <span className="check" role="presentation" aria-checked={sel}>
                {sel ? <Check size={13} strokeWidth={3} /> : null}
              </span>
            )}
          </span>
        </button>
        {isOwned && !st && opts.catalog && (owned.has(p.key) || justDone) ? (
          <button
            type="button"
            className="install-trash"
            aria-label={t.programs.trash}
            title={!isAdmin ? t.common.needsAdmin : t.programs.trash}
            disabled={busy || uninstalling !== null || !isAdmin}
            onClick={() => remove(opts.catalog!)}
          >
            {uninstalling && owned.get(p.key)?.id === uninstalling ? <Spinner size={14} /> : <Trash2 size={15} />}
          </button>
        ) : null}
      </div>
    );
  };

  return (
    <>
      <div className="row wrap" style={{ gap: 12 }}>
        <Tabs
          value={kind}
          onChange={setKind}
          items={[
            { id: "all", label: t.programs.fAll },
            { id: "direct", label: t.programs.fDirect },
            { id: "site", label: t.programs.fSite },
          ]}
        />
        <span className="spacer" />
        <SearchInput
          value={q}
          onChange={(v) => {
            setQ(v);
            if (hits && v.trim() !== hits.q) setHits(null);
          }}
          placeholder={t.programs.iSearch}
        />
      </div>

      <div className="muted small">{busy ? t.programs.iRunHint : t.programs.iHint}</div>

      {groups.map((g) => (
        <section key={g.cat} className="install-group">
          <h3 className="section-title">{t.programs[CAT_LABEL[g.cat]]}</h3>
          <div className="install-grid">
            {g.apps.map((a) =>
              card(toPick(a), <AppTile src={`/apps/${a.key}.png`} />, lang === "ru" ? a.ru : a.en, { url: a.url, catalog: a }),
            )}
          </div>
        </section>
      ))}

      {q.trim().length >= 2 ? (
        <section className="install-group">
          <h3 className="section-title">{t.programs.sTitle}</h3>
          {!hits ? (
            <button className="btn btn-secondary" style={{ justifySelf: "start" }} disabled={searching} onClick={() => void searchAll()}>
              {searching ? <Spinner size={15} /> : <Search size={15} />}
              {searching ? t.programs.sBusy : fmt(t.programs.sAll, { q: q.trim() })}
            </button>
          ) : hits.list.length === 0 ? (
            <Empty icon={<PackageOpen size={26} />} text={t.programs.sNone} />
          ) : (
            <>
              <div className="muted small">{t.programs.sHint}</div>
              <div className="install-grid">
                {hits.list.map((h) =>
                  card(
                    { key: `w:${h.id}`, name: h.name, ids: [h.id] },
                    <span className="install-icon generic">
                      <Package size={18} />
                    </span>,
                    [h.id, h.version].filter(Boolean).join(" · "),
                  ),
                )}
              </div>
            </>
          )}
        </section>
      ) : groups.length === 0 ? (
        <Empty icon={<PackageOpen size={26} />} text={t.programs.nothingFound} />
      ) : null}

      <AnimatePresence>
        {picked.size > 0 || busy ? (
          <motion.div
            className="install-bar"
            initial={calm ? { opacity: 0 } : { opacity: 0, transform: "translateY(16px)" }}
            animate={calm ? { opacity: 1 } : { opacity: 1, transform: "translateY(0px)" }}
            exit={calm ? { opacity: 0 } : { opacity: 0, transform: "translateY(16px)" }}
            transition={{ duration: 0.2, ease: [0.23, 1, 0.32, 1] }}
          >
            {busy ? (
              <div className="install-progress" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(overall)}>
                <div className="install-progress-fill" style={{ width: `${Math.max(3, overall)}%` }} />
              </div>
            ) : null}
            <span className="install-bar-text">{busy ? nowText : fmt(t.programs.iSelected, { n: picked.size })}</span>
            {!busy ? (
              <button className="btn btn-ghost sm" onClick={() => setPicked(new Map())}>
                {t.programs.iClear}
              </button>
            ) : null}
            {busy ? (
              <button
                className="btn btn-secondary"
                disabled={cancelling}
                onClick={() => {
                  session.frozen = liveValue;
                  setCancelling(true);
                  api.wingetCancel().catch(notify.err);
                }}
              >
                {t.programs.iCancel}
              </button>
            ) : null}
            <button
              className="btn btn-primary"
              disabled={busy || !isAdmin || picked.size === 0}
              title={!isAdmin ? t.common.needsAdmin : undefined}
              onClick={() => void start()}
            >
              {busy ? <Spinner size={15} /> : <Download size={15} />}
              {fmt(t.programs.iInstall, { n: picked.size })}
            </button>
          </motion.div>
        ) : null}
      </AnimatePresence>

    </>
  );
}

export function toPick(a: CatalogApp): Pick {
  return { key: a.key, name: a.name, ids: a.ids };
}

/** Site icons can be tiny (16 px): show those at their size on a tile instead of blurring them. */
function AppTile({ src }: { src: string }) {
  const [small, setSmall] = useState(false);
  return (
    <img
      className={`install-icon${small ? " small" : ""}`}
      src={src}
      alt=""
      width={36}
      height={36}
      draggable={false}
      onLoad={(e) => setSmall(e.currentTarget.naturalWidth < 32)}
    />
  );
}
