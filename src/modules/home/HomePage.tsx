import { motion } from "motion/react";
import { ArrowRight, Brush, Layers, Power, ShieldCheck } from "lucide-react";
import { useCallback, useMemo } from "react";
import { Bar, CountUp, notify, Sparkline, stagger, useCalm } from "../../components/ui";
import { countOf, fmt, useLang, useT } from "../../i18n";
import { api } from "../../lib/api";
import { cached } from "../../lib/cache";
import { formatBytes, formatDecimal, greetingKey, pct } from "../../lib/format";
import { useAsync, useHistory, usePoll } from "../../lib/hooks";
import { useStore } from "../../lib/store";
import type { DiskInfo } from "../../lib/types";

function pickSystemDisk(disks: DiskInfo[]): DiskInfo | null {
  return disks.find((d) => d.mount.toUpperCase().startsWith("C:")) ?? disks[0] ?? null;
}

export function HomePage() {
  const t = useT();
  const lang = useLang();
  const app = useStore((s) => s.app);
  const navigate = useStore((s) => s.navigate);
  const calm = useCalm();

  const { data: snap, error: snapError } = usePoll(api.snapshot, 2000);
  const junk = useAsync(() => cached("cleanupScan", 5 * 60_000, api.cleanupScan));
  const prot = useAsync(() => cached("protection", 60_000, api.protection));
  const startup = useAsync(() => cached("startup", 60_000, api.startupList));
  const history = useAsync(api.history);

  const disk = snap ? pickSystemDisk(snap.disks) : null;
  const diskUsed = disk ? disk.total - disk.available : 0;
  const diskPct = disk ? pct(diskUsed, disk.total) : 0;
  const memPct = snap ? pct(snap.memUsed, snap.memTotal) : 0;
  const cpuPct = snap ? Math.round(snap.cpu) : 0;

  const cpuHist = useHistory(snap ? snap.cpu : null, snap);
  const memHist = useHistory(snap ? memPct : null, snap);
  const diskHist = useHistory(disk ? diskPct : null, snap);

  const junkTotal = junk.data?.reduce((s, c) => s + c.size, 0) ?? 0;
  const driveLetter = disk ? disk.mount.slice(0, 1) : "C";
  const almostFull = diskPct >= 85;

  const name = app?.userName ? `, ${app.userName}` : "";
  const greeting = t.home.greet[greetingKey(new Date().getHours())] + name;

  const sub =
    snapError && !snap
      ? t.common.loadFailed
      : junk.loading
        ? t.home.checking
        : junk.error
          ? t.common.loadFailed
          : junkTotal > 50 * 1024 * 1024
            ? fmt(t.home.canFree, { size: formatBytes(junkTotal, lang) })
            : t.home.allGood;

  const defender = prot.data?.defender;
  const protActive = defender ? defender.available && defender.realtime : (prot.data?.otherAntivirus.length ?? 0) > 0;

  const tips = useMemo(() => {
    const list: { id: string; text: string; action: () => void }[] = [];
    const enabledStartup = startup.data?.filter((s) => s.enabled).length ?? 0;
    if (enabledStartup >= 8)
      list.push({ id: "startup", text: fmt(t.home.tipStartup, { n: countOf(enabledStartup, t.common.programs, lang) }), action: () => navigate("system", "startup") });
    if (defender && defender.available && !defender.realtime && (prot.data?.otherAntivirus.length ?? 0) === 0)
      list.push({ id: "def", text: t.home.tipDefender, action: () => api.openTool("defender").catch(notify.err) });
    if (defender && defender.available && defender.signatureAgeDays >= 7)
      list.push({ id: "sig", text: fmt(t.home.tipSignatures, { n: countOf(defender.signatureAgeDays, t.common.days, lang) }), action: () => navigate("protection") });
    if (prot.data && prot.data.firewall.some((f) => !f.enabled))
      list.push({ id: "fw", text: t.home.tipFirewall, action: () => navigate("protection") });
    if (memPct >= 90) list.push({ id: "mem", text: fmt(t.home.tipMemory, { p: memPct }), action: () => navigate("system", "processes") });
    return list;
  }, [startup.data, defender, prot.data, memPct, t, lang, navigate]);

  const formatPct = useCallback((v: number) => `${Math.round(v)}%`, []);

  const quick = [
    { id: "clean", label: t.home.quickClean, icon: Brush, run: () => navigate("cleanup", null, "scan") },
    { id: "startup", label: t.home.quickStartup, icon: Power, run: () => navigate("system", "startup") },
    { id: "scan", label: t.home.quickScan, icon: ShieldCheck, run: () => navigate("protection", null, "scan") },
    {
      id: "dns",
      label: t.home.quickDns,
      icon: Layers,
      run: () => api.dnsFlush().then(() => notify.ok(t.home.dnsFlushed), notify.err),
    },
  ];

  return (
    <>
      <div>
        <h1 className="page-title" style={{ fontSize: 52, fontWeight: 700, letterSpacing: "-0.035em" }}>
          <span className="truncate" style={{ display: "block" }}>
            {greeting}
          </span>
        </h1>
        <p className="page-sub">{sub}</p>
      </div>

      {/* Hero: system disk */}
      <section className="card" style={{ display: "flex", alignItems: "center", gap: 40, padding: "36px 40px" }}>
        <div className="num" style={{ fontSize: "clamp(56px, 5.4vw, 76px)", fontWeight: 200, letterSpacing: "-0.04em", lineHeight: 1, minWidth: 150 }}>
          {disk ? <CountUp value={diskPct} format={formatPct} /> : "—"}
        </div>
        <div style={{ width: 1, alignSelf: "stretch", background: "var(--border-strong)", margin: "6px 0" }} />
        <div style={{ flex: 1, minWidth: 0, display: "grid", gap: 8 }}>
          {(() => {
            const headline = almostFull ? fmt(t.home.diskAlmostFull, { d: driveLetter }) : fmt(t.home.diskStatus, { d: driveLetter });
            return (
              <div style={{ fontSize: "clamp(20px, 1.9vw, 26px)", fontWeight: 500, letterSpacing: "-0.02em" }} className="truncate" title={headline}>
                {headline}
              </div>
            );
          })()}
          <div className="muted num">
            {disk
              ? fmt(t.home.usedOf, { used: formatBytes(diskUsed, lang), total: formatBytes(disk.total, lang) })
              : snapError
                ? t.common.loadFailed
                : snap
                  ? t.common.noData
                  : "…"}
          </div>
          <div style={{ marginTop: 12, maxWidth: 560 }}>
            <Bar value={diskPct} />
          </div>
        </div>
        <button className="btn btn-primary lg" style={{ flexShrink: 0 }} onClick={() => navigate("cleanup", null, "scan")}>
          {t.home.cleanUp}
        </button>
      </section>

      {/* Stat cards */}
      <motion.div className="grid-4" variants={stagger.container} initial={calm ? false : "hidden"} animate="show">
        <motion.button className="card" variants={stagger.item} onClick={() => navigate("disk")}>
          <div className="card-label">{t.home.disk}</div>
          <div className="card-value num">{disk ? `${diskPct}%` : "—"}</div>
          <div className="card-meta num truncate">{disk ? `${formatBytes(diskUsed, lang)} / ${formatBytes(disk.total, lang)}` : snap ? t.common.noData : "…"}</div>
          <div style={{ marginTop: 18 }}>
            <Sparkline values={diskHist} max={100} />
          </div>
        </motion.button>
        <motion.button className="card" variants={stagger.item} onClick={() => navigate("system", "processes")}>
          <div className="card-label">{t.home.memory}</div>
          <div className="card-value num">{snap ? `${memPct}%` : "—"}</div>
          <div className="card-meta num truncate">{snap ? `${formatBytes(snap.memUsed, lang)} / ${formatBytes(snap.memTotal, lang)}` : "…"}</div>
          <div style={{ marginTop: 18 }}>
            <Sparkline values={memHist} max={100} />
          </div>
        </motion.button>
        <motion.button className="card" variants={stagger.item} onClick={() => navigate("system", "processes")}>
          <div className="card-label">{t.home.cpu}</div>
          <div className="card-value num">{snap ? `${cpuPct}%` : "—"}</div>
          <div className="card-meta num truncate">
            {snap && snap.cpuFreqMhz > 0 ? `${formatDecimal(snap.cpuFreqMhz / 1000, 1, lang)} ${t.home.ghz}` : snap ? "—" : "…"}
          </div>
          <div style={{ marginTop: 18 }}>
            <Sparkline values={cpuHist} max={100} />
          </div>
        </motion.button>
        <motion.div className="card" variants={stagger.item}>
          <div className="card-label">{t.home.protection}</div>
          <div className="row" style={{ marginTop: 10, gap: 12, minHeight: 50 }}>
            <span className={`dot ${prot.data ? (protActive ? "" : "warn") : "muted"}`} style={{ width: 12, height: 12 }} />
            <span style={{ fontSize: 30, fontWeight: 400, letterSpacing: "-0.02em" }} className="truncate">
              {prot.loading ? "…" : prot.data ? (protActive ? t.home.active : t.home.inactive) : t.home.unknown}
            </span>
          </div>
          <button className="link" style={{ marginTop: 16, textAlign: "left" }} onClick={() => api.openTool("defender").catch(notify.err)}>
            {t.home.openSecurity}
          </button>
        </motion.div>
      </motion.div>

      {/* Quick actions */}
      <div className="grid-4">
        {quick.map((q) => {
          const Icon = q.icon;
          return (
            <button key={q.id} className="card" style={{ display: "flex", alignItems: "center", gap: 18, padding: "26px 30px" }} onClick={q.run}>
              <Icon size={24} strokeWidth={1.6} />
              <span style={{ fontSize: 18, fontWeight: 500 }} className="truncate">
                {q.label}
              </span>
            </button>
          );
        })}
      </div>

      {/* Tips */}
      {tips.length > 0 ? (
        <section className="card flush">
          <div className="list-head">{t.home.tips}</div>
          <div className="list">
            {tips.map((tip) => (
              <div key={tip.id} className="list-row">
                <span className="dot warn" />
                <span className="cell-main truncate">{tip.text}</span>
                <button className="btn btn-secondary sm" onClick={tip.action}>
                  {t.home.fix}
                  <ArrowRight size={14} />
                </button>
              </div>
            ))}
          </div>
        </section>
      ) : null}

      {history.data && history.data.totalFreed > 0 ? (
        <div className="small faint num">{fmt(t.home.totalFreed, { size: formatBytes(history.data.totalFreed, lang) })}</div>
      ) : null}
    </>
  );
}
