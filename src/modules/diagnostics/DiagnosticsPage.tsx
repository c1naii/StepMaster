import { motion } from "motion/react";
import {
  Check,
  ChevronDown,
  CircleCheck,
  Cpu,
  ExternalLink,
  Flame,
  HardDrive,
  MonitorX,
  RotateCw,
  ShieldAlert,
  Stethoscope,
  Wand2,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { PageHeader, Ring, Spinner, Toggle, notify, stagger, useCalm, StopButton } from "../../components/ui";
import { fmt, useLang, useT } from "../../i18n";
import { api, errorText, on } from "../../lib/api";
import { formatDate } from "../../lib/format";
import { ask, useStore, type ModuleId } from "../../lib/store";
import type { DiagCategory, DiagIssue, DiagProgress, DiagReport } from "../../lib/types";

const CAT_ICON: Record<DiagCategory, typeof Cpu> = {
  performance: Cpu,
  thermal: Flame,
  hardware: HardDrive,
  system: MonitorX,
  security: ShieldAlert,
};

/**
 * "Fix all" runs only quick fixes that can't hurt anything the user set up on purpose.
 * Everything else (threats, hosts, startup, proxy, Windows repair, scans) is done one by one.
 */
const FIX_ALL = new Set(["cleanup_safe", "power_balanced", "update_signatures", "enable_firewall", "enable_uac"]);
/** Fixes that change something the user may have set up themselves: ask first, show the list. */
const CONFIRM_FIRST = new Set(["remove_threats", "restore_hosts", "disable_startup", "disable_proxy", "enable_uac", "repair_system"]);

type FixState = { state: "idle" | "busy" | "done" | "error"; msg?: string };

/** Numbers come from the backend as "18.2"; Russian uses a decimal comma. */
function localize(params: Record<string, string>, lang: string): Record<string, string> {
  if (lang !== "ru") return params;
  return Object.fromEntries(Object.entries(params).map(([k, v]) => [k, /^\d+\.\d+$/.test(v) ? v.replace(".", ",") : v]));
}

// The last report survives page switches (and is also stored by the backend).
let lastReport: DiagReport | null = null;
let running = false;

function useOpen() {
  const navigate = useStore((s) => s.navigate);
  return (target: string) => {
    const [kind, a, b] = target.split(":");
    if (kind === "nav") navigate(a as ModuleId, b ?? null);
    else if (kind === "tool") api.openTool(a).catch(notify.err);
  };
}

function IssueCard({ issue, fix, onFix, locked }: { issue: DiagIssue; fix: FixState; onFix: () => void; locked?: boolean }) {
  const t = useT();
  const lang = useLang();
  const open = useOpen();
  const isAdmin = useStore((s) => s.app?.isAdmin ?? true);
  const [showDetails, setShowDetails] = useState(false);
  const text = t.diag.issues[issue.code] ?? { title: issue.code, why: "", fix: "" };
  const params = localize(issue.params, lang);
  const Icon = CAT_ICON[issue.category] ?? Cpu;
  const fixLabel = issue.fix ? (t.diag.fixes as Record<string, string>)[issue.fix] ?? issue.fix : null;
  const fixNote = issue.fix ? (t.diag.fixNotes as Record<string, string>)[issue.fix] : undefined;
  const openLabel = issue.open ? (t.diag.opens as Record<string, string>)[issue.open] : undefined;

  return (
    <div className={`card issue-card sev-${issue.severity} ${fix.state === "done" ? "is-fixed" : ""}`}>
      <div className="row" style={{ gap: 12, alignItems: "flex-start" }}>
        <Icon size={18} strokeWidth={1.7} className="faint" style={{ marginTop: 3, flexShrink: 0 }} />
        <div className="cell-main">
          <div style={{ fontSize: 17, fontWeight: 600, letterSpacing: "-0.01em" }}>{fmt(text.title, params)}</div>
          <div className="small faint" style={{ marginTop: 2 }}>
            {t.diag.catNames[issue.category]}
            {issue.params.last ? ` · ${issue.params.last}` : ""}
          </div>
        </div>
        {fix.state === "done" ? (
          <span className="badge solid">
            <Check size={12} style={{ marginRight: 4 }} />
            {t.diag.fixed}
          </span>
        ) : (
          <span className={`sev-badge sev-${issue.severity}`}>{t.diag.sev[issue.severity]}</span>
        )}
      </div>

      <div className="issue-cols">
        <div>
          <div className="issue-label">{t.diag.why}</div>
          <div className="issue-text">{fmt(text.why, params)}</div>
        </div>
        <div>
          <div className="issue-label">{t.diag.how}</div>
          <div className="issue-text">{fmt(text.fix, params)}</div>
        </div>
      </div>

      {issue.details.length > 0 ? (
        <>
          <button className="btn btn-ghost sm" style={{ marginTop: 10, paddingLeft: 0 }} onClick={() => setShowDetails((v) => !v)}>
            <ChevronDown size={14} style={{ transform: showDetails ? "rotate(180deg)" : undefined, transition: "transform 160ms ease" }} />
            {t.diag.details} ({issue.details.length})
          </button>
          {showDetails ? (
            <div className="issue-details">
              {issue.details.map((d, i) => (
                <div key={i}>{d}</div>
              ))}
            </div>
          ) : null}
        </>
      ) : null}

      {(fixLabel || openLabel) && fix.state !== "done" ? (
        <div className="row wrap" style={{ marginTop: 16 }}>
          {fixLabel ? (
            <button
              className="btn btn-primary sm"
              disabled={fix.state === "busy" || !isAdmin || locked}
              title={!isAdmin ? t.common.needsAdmin : fixNote}
              onClick={onFix}
            >
              {fix.state === "busy" ? <Spinner size={14} /> : <Wand2 size={14} />}
              {fix.state === "busy" ? t.diag.fixing : fixLabel}
            </button>
          ) : null}
          {openLabel && issue.open ? (
            <button className="btn btn-secondary sm" onClick={() => open(issue.open!)}>
              {openLabel}
              <ExternalLink size={13} />
            </button>
          ) : null}
          {fix.state === "busy" && fixNote ? <span className="small faint">{fixNote}</span> : null}
          {fix.state === "error" ? <span className="small" style={{ color: "var(--danger)" }}>{fmt(t.diag.fixFailed, { e: fix.msg ?? "" })}</span> : null}
        </div>
      ) : null}
      {fix.state === "done" && fix.msg === "reboot" ? <div className="small muted" style={{ marginTop: 12 }}>{t.diag.needReboot}</div> : null}
      {fix.state === "done" && fix.msg?.startsWith("backup:") ? (
        <div className="small muted" style={{ marginTop: 12, userSelect: "text", overflowWrap: "anywhere" }}>
          {fmt(t.diag.hostsBackup, { path: fix.msg.slice(7) })}
        </div>
      ) : null}
    </div>
  );
}

/** Pages mounted while a run finishes (the user may leave and come back during a run). */
const doneListeners = new Set<() => void>();
/** "Stop" was pressed for the current run (kept here: the page may remount meanwhile). */
let stopRequested = false;

export function DiagnosticsPage() {
  const t = useT();
  const lang = useLang();
  const calm = useCalm();
  const isAdmin = useStore((s) => s.app?.isAdmin ?? true);
  const [report, setReport] = useState<DiagReport | null>(lastReport);
  const [busy, setBusy] = useState(running);
  const [progress, setProgress] = useState<DiagProgress | null>(null);
  const [withDefender, setWithDefender] = useState(true);
  const [fixes, setFixes] = useState<Record<number, FixState>>({});
  const [fixingAll, setFixingAll] = useState(false);
  const [stopping, setStopping] = useState(stopRequested);
  const stop = () => {
    stopRequested = true;
    setStopping(true);
    api.opCancel("diag").catch(notify.err);
  };
  const [showPassed, setShowPassed] = useState(false);
  const alive = useRef(true);

  useEffect(() => {
    alive.current = true;
    if (!lastReport) {
      api
        .diagLast()
        .then((r) => {
          if (r && alive.current && !lastReport) {
            lastReport = r;
            setReport(r);
          }
        })
        .catch(() => undefined);
    }
    const onDone = () => {
      setReport(lastReport);
      setBusy(false);
      setStopping(false);
    };
    doneListeners.add(onDone);
    let off: (() => void) | undefined;
    void on<DiagProgress>("diag://progress", (p) => alive.current && setProgress(p)).then((u) => {
      if (alive.current) off = u;
      else u();
    });
    return () => {
      alive.current = false;
      doneListeners.delete(onDone);
      off?.();
    };
  }, []);

  const run = async () => {
    if (running) return;
    running = true;
    setBusy(true);
    setProgress(null);
    setFixes({});
    try {
      lastReport = await api.diagRun(withDefender);
    } catch (e) {
      if (stopRequested) notify.info(t.common.stopped);
      else notify.err(e);
    } finally {
      running = false;
      stopRequested = false;
      setStopping(false);
      doneListeners.forEach((f) => f());
    }
  };

  const fixOne = async (i: number, issue: DiagIssue, confirmed = false): Promise<boolean> => {
    if (!issue.fix) return false;
    if (!confirmed && CONFIRM_FIRST.has(issue.fix)) {
      const text = t.diag.issues[issue.code];
      const ok = await ask({
        title: (t.diag.fixes as Record<string, string>)[issue.fix] ?? issue.fix,
        text: text ? fmt(text.fix, localize(issue.params, lang)) : undefined,
        details: issue.details,
        confirmLabel: t.diag.fixAllConfirm,
        cancelLabel: t.common.cancel,
        danger: true, // focus starts on "Cancel": Enter never applies it by accident
      });
      if (!ok) return false;
    }
    setFixes((f) => ({ ...f, [i]: { state: "busy" } }));
    try {
      const r = await api.diagFix(issue.fix, issue.fixArgs);
      const msg = r.note === "reboot" ? "reboot" : issue.fix === "restore_hosts" && r.note ? `backup:${r.note}` : undefined;
      if (alive.current) setFixes((f) => ({ ...f, [i]: { state: "done", msg } }));
      return true;
    } catch (e) {
      if (alive.current) setFixes((f) => ({ ...f, [i]: { state: "error", msg: errorText(e) } }));
      return false;
    }
  };

  const autoFixable = useMemo(
    () => (report?.issues ?? []).map((iss, i) => ({ iss, i })).filter(({ iss, i }) => iss.fix && FIX_ALL.has(iss.fix) && fixes[i]?.state !== "done"),
    [report, fixes],
  );

  const fixAll = async () => {
    const ok = await ask({
      title: t.diag.fixAllTitle,
      text: fmt(t.diag.fixAllText, { n: autoFixable.length }),
      details: autoFixable.map(({ iss }) => fmt((t.diag.issues[iss.code] ?? { title: iss.code }).title, localize(iss.params, lang))),
      confirmLabel: t.diag.fixAllConfirm,
      cancelLabel: t.common.cancel,
    });
    if (!ok) return;
    setFixingAll(true);
    try {
      // Safety net first; "skipped" (one point per day) is fine.
      await api.restoreCreate(t.diag.introTitle).catch(() => undefined);
      let done = 0;
      // Already confirmed once for the whole list above.
      for (const { iss, i } of autoFixable) {
        if (await fixOne(i, iss, true)) done++;
      }
      notify.ok(fmt(t.diag.fixedAll, { n: done, total: autoFixable.length }));
    } finally {
      if (alive.current) setFixingAll(false);
    }
  };

  const stepLabel = (s?: DiagProgress["step"]) =>
    s === "defender" ? t.diag.stepDefender : s === "system" ? t.diag.stepSystem : s === "performance" ? t.diag.stepPerformance : s === "evaluate" ? t.diag.stepEvaluate : s === "done" ? t.diag.stepDone : t.diag.running;

  const steps: DiagProgress["step"][] = withDefender ? ["defender", "system", "performance", "evaluate"] : ["system", "performance", "evaluate"];
  const pctDone = progress ? Math.round((progress.done / Math.max(1, progress.total)) * 100) : 0;

  const passed = report?.checks.filter((c) => c.status === "ok") ?? [];
  const skipped = report?.checks.filter((c) => c.status === "skipped") ?? [];
  const score = report?.score ?? 0;
  const verdict = score >= 85 ? t.diag.scoreGood : score >= 60 ? t.diag.scoreFair : t.diag.scoreBad;
  const checkName = (code: string) => (t.diag.checks as Record<string, string>)[code] ?? code;

  return (
    <>
      <PageHeader
        title={t.diag.title}
        sub={t.diag.sub}
        right={
          busy ? (
            <StopButton onClick={stop} stopping={stopping} />
          ) : report ? (
            <button className="btn btn-secondary" onClick={() => void run()}>
              <RotateCw size={15} />
              {t.diag.again}
            </button>
          ) : null
        }
      />
      {!isAdmin ? <div className="notice warn">{t.diag.adminHint}</div> : null}

      {busy ? (
        /* ---------- Running ---------- */
        <section className="card" style={{ display: "flex", alignItems: "center", gap: 40, padding: "36px 40px" }}>
          <Ring value={pctDone} size={180} stroke={9}>
            <div style={{ display: "grid", justifyItems: "center" }}>
              <span className="num" style={{ fontSize: 34, fontWeight: 300 }}>{pctDone}%</span>
            </div>
          </Ring>
          <div className="cell-main" style={{ display: "grid", gap: 14 }}>
            <div style={{ fontSize: 22, fontWeight: 600, letterSpacing: "-0.02em" }}>{t.diag.running}</div>
            {steps.map((s, i) => {
              const doneStep = (progress?.done ?? 0) > i || progress?.step === "done";
              const current = progress?.step === s;
              return (
                <div key={s} className={`step-row ${current ? "is-current" : ""} ${doneStep ? "is-done" : ""}`}>
                  <span style={{ width: 18, display: "grid", placeItems: "center" }}>
                    {doneStep ? <Check size={16} /> : current ? <Spinner size={15} /> : <span className="dot muted" />}
                  </span>
                  {stepLabel(s)}
                </div>
              );
            })}
          </div>
        </section>
      ) : !report ? (
        /* ---------- Intro ---------- */
        <section className="card" style={{ padding: "36px 40px", display: "grid", gap: 26 }}>
          <div className="row" style={{ gap: 24, alignItems: "flex-start" }}>
            <div
              style={{
                width: 72,
                height: 72,
                borderRadius: 20,
                display: "grid",
                placeItems: "center",
                background: "var(--panel-raised)",
                border: "1px solid var(--border-strong)",
                flexShrink: 0,
              }}
            >
              <Stethoscope size={32} strokeWidth={1.5} />
            </div>
            <div className="cell-main">
              <div style={{ fontSize: 26, fontWeight: 600, letterSpacing: "-0.02em" }}>{t.diag.introTitle}</div>
              <div className="muted" style={{ marginTop: 8, maxWidth: 720, lineHeight: 1.55 }}>{t.diag.introText}</div>
            </div>
          </div>
          <div className="row wrap" style={{ gap: 8 }}>
            {(Object.keys(CAT_ICON) as DiagCategory[]).map((c) => {
              const Icon = CAT_ICON[c];
              return (
                <span key={c} className="chip">
                  <Icon size={15} strokeWidth={1.7} />
                  {t.diag.catNames[c]}
                </span>
              );
            })}
          </div>
          <div className="row" style={{ gap: 16, flexWrap: "wrap" }}>
            <div className="row" style={{ gap: 12, flex: 1, minWidth: 260 }}>
              <Toggle checked={withDefender} onChange={setWithDefender} label={t.diag.defenderToggle} />
              <div>
                <div style={{ fontWeight: 500 }}>{t.diag.defenderToggle}</div>
                <div className="small faint">{t.diag.defenderToggleDesc}</div>
              </div>
            </div>
            <button className="btn btn-primary lg" onClick={() => void run()}>
              <Stethoscope size={18} />
              {t.diag.start}
            </button>
          </div>
        </section>
      ) : (
        /* ---------- Result ---------- */
        <>
          <section className="card" style={{ display: "flex", alignItems: "center", gap: 36, padding: "32px 40px", flexWrap: "wrap" }}>
            <Ring value={score} size={170} stroke={9}>
              <div style={{ display: "grid", justifyItems: "center" }}>
                <span className="num" style={{ fontSize: 46, fontWeight: 200, letterSpacing: "-0.03em", lineHeight: 1 }}>{score}</span>
                <span className="small faint" style={{ marginTop: 4 }}>{t.diag.score}</span>
              </div>
            </Ring>
            <div className="cell-main" style={{ display: "grid", gap: 8, minWidth: 260 }}>
              <div style={{ fontSize: 26, fontWeight: 600, letterSpacing: "-0.02em" }}>{verdict}</div>
              <div className="muted">{report.issues.length > 0 ? fmt(t.diag.found, { n: report.issues.length }) : t.diag.noIssues}</div>
              <div className="small faint">
                {fmt(t.diag.lastRun, { date: formatDate(report.at, lang) })} · {fmt(t.diag.duration, { s: Math.max(1, Math.round(report.durationMs / 1000)) })}
                {report.defenderScanned ? ` · ${t.diag.scanned}` : ""}
              </div>
              <div className="row wrap" style={{ marginTop: 10 }}>
                {autoFixable.length > 0 ? (
                  <button className="btn btn-primary" disabled={fixingAll || !isAdmin} title={!isAdmin ? t.common.needsAdmin : undefined} onClick={() => void fixAll()}>
                    {fixingAll ? <Spinner /> : <Wand2 size={16} />}
                    {fixingAll ? t.diag.fixing : fmt(t.diag.fixAll, { n: autoFixable.length })}
                  </button>
                ) : null}
                <div className="row" style={{ gap: 10 }}>
                  <Toggle checked={withDefender} onChange={setWithDefender} label={t.diag.defenderToggle} />
                  <span className="small muted">{t.diag.defenderToggle}</span>
                </div>
              </div>
            </div>
          </section>

          {report.issues.length === 0 ? (
            <section className="card" style={{ display: "grid", justifyItems: "center", gap: 10, padding: 36 }}>
              <CircleCheck size={30} strokeWidth={1.5} />
              <div style={{ fontSize: 18, fontWeight: 500 }}>{t.diag.noIssues}</div>
            </section>
          ) : (
            <motion.div style={{ display: "grid", gap: 12 }} variants={stagger.container} initial={calm ? false : "hidden"} animate="show">
              {report.issues.map((iss, i) => (
                <motion.div key={`${iss.code}-${i}`} variants={stagger.item}>
                  <IssueCard issue={iss} fix={fixes[i] ?? { state: "idle" }} onFix={() => void fixOne(i, iss)} locked={fixingAll} />
                </motion.div>
              ))}
            </motion.div>
          )}

          <section className="card flush">
            <button className="list-head" style={{ width: "100%", textTransform: "none", letterSpacing: 0, fontSize: 13, cursor: "pointer" }} onClick={() => setShowPassed((v) => !v)}>
              <CircleCheck size={15} />
              <span style={{ flex: 1, textAlign: "left" }}>
                {fmt(t.diag.passed, { n: passed.length })}
                {skipped.length > 0 ? ` · ${fmt(t.diag.skipped, { n: skipped.length })}` : ""}
              </span>
              <ChevronDown size={15} style={{ transform: showPassed ? "rotate(180deg)" : undefined, transition: "transform 160ms ease" }} />
            </button>
            {showPassed ? (
              <div style={{ padding: "8px 20px 16px", display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(240px, 1fr))", gap: "6px 20px" }}>
                {passed.map((c) => (
                  <div key={`ok-${c.code}`} className="row small" style={{ gap: 8 }}>
                    <Check size={14} />
                    <span className="truncate">{checkName(c.code)}</span>
                  </div>
                ))}
                {skipped.map((c) => (
                  <div key={`sk-${c.code}`} className="row small faint" style={{ gap: 8 }} title={t.diag.skippedHint}>
                    <span style={{ width: 14, textAlign: "center" }}>–</span>
                    <span className="truncate">{checkName(c.code)}</span>
                  </div>
                ))}
              </div>
            ) : null}
          </section>
        </>
      )}
    </>
  );
}
