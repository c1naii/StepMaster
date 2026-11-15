import { AnimatePresence, motion } from "motion/react";
import { Check, RotateCw, Sparkles } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { Checkbox, ErrorState, PageHeader, Ring, Skeleton, Spinner, notify, useCalm } from "../../components/ui";
import { countOf, fmt, useLang, useT } from "../../i18n";
import { api, errorText, on } from "../../lib/api";
import { invalidate } from "../../lib/cache";
import { formatBytes } from "../../lib/format";
import { useStore } from "../../lib/store";
import type { CleanupCategory, CleanupId, CleanupProgress, CleanupResult } from "../../lib/types";

type Phase = "idle" | "scanning" | "ready" | "cleaning" | "done" | "error";

export function CleanupPage() {
  const t = useT();
  const lang = useLang();
  const isAdmin = useStore((s) => s.app?.isAdmin ?? true);
  const calm = useCalm();

  const [phase, setPhase] = useState<Phase>("idle");
  const [cats, setCats] = useState<CleanupCategory[]>([]);
  const [selected, setSelected] = useState<Set<CleanupId>>(new Set());
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<CleanupResult | null>(null);
  const [progress, setProgress] = useState<{ current: CleanupId | null; done: Set<CleanupId> }>({ current: null, done: new Set() });
  const alive = useRef(true);

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const scan = async () => {
    setPhase("scanning");
    setError(null);
    setResult(null);
    try {
      const res = await api.cleanupScan();
      if (!alive.current) return;
      setCats(res);
      setSelected(new Set(res.filter((c) => c.available && c.size > 0 && (!c.needsAdmin || isAdmin)).map((c) => c.id)));
      setPhase("ready");
    } catch (e) {
      if (!alive.current) return;
      setError(errorText(e));
      setPhase("error");
    }
  };

  // Auto-scan on open (also when opened from Home / palette).
  useEffect(() => {
    useStore.getState().takePendingAction();
    void scan();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    let off: (() => void) | undefined;
    void on<CleanupProgress>("cleanup://progress", (p) => {
      setProgress((prev) => {
        const done = new Set(prev.done);
        if (p.phase === "done") done.add(p.id);
        return { current: p.phase === "start" ? p.id : prev.current, done };
      });
    }).then((u) => (off = u));
    return () => off?.();
  }, []);

  const selectedSize = useMemo(() => cats.filter((c) => selected.has(c.id)).reduce((s, c) => s + c.size, 0), [cats, selected]);
  const total = useMemo(() => cats.reduce((s, c) => s + c.size, 0), [cats]);

  const clean = async () => {
    if (selected.size === 0) {
      notify.info(t.cleanup.selectAtLeastOne);
      return;
    }
    setPhase("cleaning");
    setProgress({ current: null, done: new Set() });
    try {
      const res = await api.cleanupRun([...selected]);
      invalidate("cleanupScan");
      if (!alive.current) return;
      setResult(res);
      setPhase("done");
    } catch (e) {
      if (!alive.current) return;
      notify.err(e);
      setPhase("ready");
    }
  };

  const toggle = (id: CleanupId, v: boolean) =>
    setSelected((s) => {
      const n = new Set(s);
      if (v) n.add(id);
      else n.delete(id);
      return n;
    });

  const selectable = cats.filter((c) => c.available && c.size > 0 && (!c.needsAdmin || isAdmin));
  const ringValue =
    phase === "cleaning" ? (selected.size ? (progress.done.size / selected.size) * 100 : 0) : phase === "done" ? 100 : total > 0 ? (selectedSize / total) * 100 : 0;

  return (
    <>
      <PageHeader
        title={t.cleanup.title}
        sub={t.cleanup.sub}
        right={
          phase === "ready" || phase === "done" || phase === "error" ? (
            <button className="btn btn-secondary" onClick={() => void scan()}>
              <RotateCw size={15} />
              {t.cleanup.rescan}
            </button>
          ) : null
        }
      />

      {phase === "error" && error ? (
        <div className="card">
          <ErrorState error={error} onRetry={() => void scan()} />
        </div>
      ) : (
        <div style={{ display: "grid", gridTemplateColumns: "minmax(300px, 380px) 1fr", gap: 16, alignItems: "start" }}>
          {/* Ring panel */}
          <section className="card" style={{ display: "grid", justifyItems: "center", gap: 28, padding: "40px 28px" }}>
            <Ring value={ringValue} spinning={phase === "scanning"} size={230}>
              <AnimatePresence mode="wait" initial={false}>
                <motion.div
                  key={phase === "done" ? "done" : phase === "scanning" ? "scan" : "size"}
                  initial={calm ? { opacity: 0 } : { opacity: 0, filter: "blur(4px)" }}
                  animate={{ opacity: 1, filter: "blur(0px)" }}
                  exit={calm ? { opacity: 0 } : { opacity: 0, filter: "blur(4px)" }}
                  transition={{ duration: 0.18 }}
                  style={{ display: "grid", justifyItems: "center", gap: 4, padding: "0 20px" }}
                >
                  {phase === "scanning" || phase === "idle" ? (
                    <span className="muted">{t.cleanup.scanning}</span>
                  ) : phase === "done" && result ? (
                    <>
                      <Check size={28} />
                      <span className="num" style={{ fontSize: 30, fontWeight: 300, letterSpacing: "-0.03em" }}>
                        {formatBytes(result.freed, lang)}
                      </span>
                      <span className="muted small">{t.common.done}</span>
                    </>
                  ) : (
                    <>
                      <span className="num" style={{ fontSize: 38, fontWeight: 300, letterSpacing: "-0.03em" }}>
                        {formatBytes(phase === "cleaning" ? selectedSize : total, lang)}
                      </span>
                      <span className="muted small">{phase === "cleaning" ? t.cleanup.cleaning : total > 0 ? t.cleanup.canClean : t.cleanup.nothing}</span>
                    </>
                  )}
                </motion.div>
              </AnimatePresence>
            </Ring>

            {phase === "done" && result ? (
              <div style={{ textAlign: "center", display: "grid", gap: 6 }}>
                <div style={{ fontWeight: 600 }}>{fmt(t.cleanup.freed, { size: formatBytes(result.freed, lang) })}</div>
                <div className="small muted">{fmt(t.cleanup.freedSub, { n: countOf(result.deleted, t.common.files, lang) })}</div>
                {result.skipped > 0 ? <div className="small faint">{fmt(t.cleanup.skipped, { n: countOf(result.skipped, t.common.files, lang) })}</div> : null}
              </div>
            ) : (
              <button
                className="btn btn-primary lg"
                style={{ width: "100%" }}
                disabled={phase !== "ready" || selectedSize === 0}
                onClick={() => void clean()}
              >
                {phase === "cleaning" ? <Spinner /> : <Sparkles size={18} />}
                {phase === "cleaning" ? t.cleanup.cleaning : fmt(t.cleanup.clean, { size: formatBytes(selectedSize, lang) })}
              </button>
            )}
          </section>

          {/* Categories */}
          <section className="card flush">
            <div className="list-head">
              <span style={{ flex: 1 }}>{t.cleanup.title}</span>
              {phase === "ready" && selectable.length > 0 ? (
                <button
                  className="btn btn-ghost sm"
                  style={{ textTransform: "none", letterSpacing: 0 }}
                  onClick={() => setSelected(selected.size === selectable.length ? new Set() : new Set(selectable.map((c) => c.id)))}
                >
                  {selected.size === selectable.length ? t.cleanup.selectNone : t.cleanup.selectAll}
                </button>
              ) : null}
            </div>
            <div className="list">
              {phase === "scanning" || phase === "idle"
                ? Array.from({ length: 8 }, (_, i) => (
                    <div className="list-row" key={i}>
                      <Skeleton h={18} w={18} r={5} />
                      <div className="cell-main" style={{ display: "grid", gap: 8 }}>
                        <Skeleton h={12} w="35%" />
                        <Skeleton h={10} w="55%" />
                      </div>
                      <Skeleton h={12} w={60} />
                    </div>
                  ))
                : cats.map((c) => {
                    const info = t.cleanup.cats[c.id];
                    const locked = !c.available || (c.needsAdmin && !isAdmin);
                    const isDone = progress.done.has(c.id);
                    const isCurrent = phase === "cleaning" && progress.current === c.id && !isDone;
                    return (
                      <div className="list-row" key={c.id} style={{ opacity: locked || c.size === 0 ? 0.55 : 1 }}>
                        {phase === "cleaning" || phase === "done" ? (
                          <span style={{ width: 18, display: "grid", placeItems: "center" }}>
                            {isCurrent ? <Spinner size={15} /> : isDone ? <Check size={16} /> : <span className="dot muted" />}
                          </span>
                        ) : locked || c.size === 0 ? (
                          <span className="check" style={{ opacity: 0.4 }} />
                        ) : (
                          <Checkbox checked={selected.has(c.id)} onChange={(v) => toggle(c.id, v)} label={info.title} />
                        )}
                        <div className="cell-main">
                          <div className="cell-title">{info.title}</div>
                          <div className="cell-sub">{locked && c.needsAdmin && !isAdmin ? t.cleanup.needsAdmin : info.desc}</div>
                        </div>
                        <div className="num" style={{ fontWeight: 500, whiteSpace: "nowrap" }}>
                          {formatBytes(c.size, lang)}
                        </div>
                      </div>
                    );
                  })}
            </div>
          </section>
        </div>
      )}
    </>
  );
}
