import { BrickWall, Download, ExternalLink, RefreshCcw, ScanSearch, Shield } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Bar, ErrorState, PageHeader, Skeleton, Spinner, notify } from "../../components/ui";
import { fmt, useT } from "../../i18n";
import { api } from "../../lib/api";
import { cached, invalidate } from "../../lib/cache";
import { useAsync } from "../../lib/hooks";
import { ask, useStore } from "../../lib/store";

// Quick scan can outlive the page; keep its state at module level so returning shows progress.
let scanRunning = false;
const scanListeners = new Set<(v: boolean) => void>();
function setScanRunning(v: boolean) {
  scanRunning = v;
  scanListeners.forEach((f) => f(v));
}

export function ProtectionPage() {
  const t = useT();
  const st = useAsync(() => cached("protection", 30_000, api.protection));
  const fixes = useAsync(api.hotfixes);
  const [scanning, setScanning] = useState(scanRunning);
  const [busy, setBusy] = useState<string | null>(null);
  const autoStarted = useRef(false);
  const isAdmin = useStore((s) => s.app?.isAdmin ?? true);

  useEffect(() => {
    scanListeners.add(setScanning);
    return () => void scanListeners.delete(setScanning);
  }, []);

  const reload = () => {
    invalidate("protection");
    void st.reload();
  };

  const quickScan = async () => {
    if (scanRunning) return;
    setScanRunning(true);
    notify.info(t.home.scanStarted);
    try {
      await api.defenderQuickScan();
      notify.ok(t.protection.scanDone);
      invalidate("protection");
    } catch (e) {
      notify.err(e);
    } finally {
      setScanRunning(false);
    }
  };

  useEffect(() => {
    if (autoStarted.current) return;
    autoStarted.current = true;
    if (useStore.getState().takePendingAction() === "scan") void quickScan();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const run = async (key: string, fn: () => Promise<void>, okMsg: string) => {
    setBusy(key);
    try {
      await fn();
      notify.ok(okMsg);
      reload();
    } catch (e) {
      notify.err(e);
    } finally {
      setBusy(null);
    }
  };

  const d = st.data?.defender;
  const fwAllOn = st.data ? st.data.firewall.every((f) => f.enabled) : true;
  const profName = (n: string) =>
    n.toLowerCase().startsWith("dom") ? t.protection.profDomain : n.toLowerCase().startsWith("priv") ? t.protection.profPrivate : t.protection.profPublic;

  return (
    <>
      <PageHeader title={t.protection.title} sub={t.protection.sub} />

      {st.error ? (
        <div className="card">
          <ErrorState error={st.error} onRetry={reload} />
        </div>
      ) : null}

      {/* Defender */}
      <section className="card" style={{ display: "grid", gap: 24 }}>
        <div className="row" style={{ gap: 16 }}>
          <Shield size={26} strokeWidth={1.5} />
          <div className="cell-main">
            <div className="section-title">{t.protection.defender}</div>
            <div className="small muted">{t.protection.openSecurityDesc}</div>
          </div>
          <button className="btn btn-primary" onClick={() => api.openTool("defender").catch(notify.err)}>
            {t.protection.openSecurity}
            <ExternalLink size={15} />
          </button>
        </div>

        {st.data && st.data.otherAntivirus.length > 0 ? (
          <div className="notice">{fmt(t.protection.otherAv, { name: st.data.otherAntivirus.join(", ") })}</div>
        ) : null}

        {!st.data && !st.loading ? null : !st.data ? (
          <div className="grid-4">
            {[0, 1, 2, 3].map((i) => (
              <div key={i} style={{ display: "grid", gap: 10 }}>
                <Skeleton h={12} w="50%" />
                <Skeleton h={22} w="70%" />
              </div>
            ))}
          </div>
        ) : d && d.available ? (
          <div className="grid-4">
            <div>
              <div className="card-label">{t.protection.realtime}</div>
              <div className="row" style={{ marginTop: 8, gap: 10 }}>
                <span className={`dot ${d.realtime ? "" : "warn"}`} />
                <span style={{ fontSize: 20, fontWeight: 500 }}>{d.realtime ? t.protection.on : t.protection.off}</span>
              </div>
            </div>
            <div>
              <div className="card-label">{t.protection.signatures}</div>
              <div style={{ fontSize: 15, fontWeight: 500, marginTop: 10 }} className="truncate">
                {d.signatureUpdated ? fmt(t.protection.signaturesUpdated, { date: d.signatureUpdated }) : "—"}
              </div>
            </div>
            <div>
              <div className="card-label">{t.protection.lastScan}</div>
              <div style={{ fontSize: 15, fontWeight: 500, marginTop: 10 }} className="truncate">
                {d.lastQuickScan || "—"}
              </div>
            </div>
            <div>
              <div className="card-label">{t.protection.threats}</div>
              <div style={{ fontSize: 20, fontWeight: 500, marginTop: 8, color: d.threats > 0 ? "var(--warn)" : undefined }}>
                {d.threats > 0 ? d.threats : t.protection.noThreats}
              </div>
            </div>
          </div>
        ) : (
          <div className="notice">{t.protection.unavailable}</div>
        )}

        {scanning ? (
          <div style={{ display: "grid", gap: 10 }}>
            <div className="small muted">{t.protection.scanning}</div>
            <Bar value={0} thin indeterminate />
          </div>
        ) : null}

        <div className="row wrap">
          <button className="btn btn-secondary" disabled={scanning || !d?.available} onClick={() => void quickScan()}>
            {scanning ? <Spinner /> : <ScanSearch size={16} />}
            {t.protection.quickScan}
          </button>
          <button
            className="btn btn-secondary"
            disabled={busy !== null || !d?.available}
            onClick={() => void run("sigs", api.defenderUpdate, t.protection.sigsUpdated)}
          >
            {busy === "sigs" ? <Spinner /> : <Download size={16} />}
            {busy === "sigs" ? t.protection.updatingSigs : t.protection.updateSigs}
          </button>
        </div>
      </section>

      <div className="grid-2" style={{ alignItems: "start" }}>
        {/* Firewall (hidden if its status couldn't be read) */}
        <section className="card flush" style={!st.data && !st.loading ? { display: "none" } : undefined}>
          <div className="row" style={{ padding: "20px 24px", gap: 14 }}>
            <BrickWall size={22} strokeWidth={1.5} />
            <div className="section-title cell-main">{t.protection.firewall}</div>
            <button className="btn btn-ghost sm" onClick={() => api.openTool("firewall").catch(notify.err)}>
              {t.protection.fwAdvanced}
            </button>
          </div>
          <div className="list" style={{ borderTop: "1px solid var(--divider)" }}>
            {(st.data?.firewall ?? []).map((f) => (
              <div key={f.name} className="list-row">
                <span className={`dot ${f.enabled ? "" : "warn"}`} />
                <span className="cell-main">{profName(f.name)}</span>
                <span className="small muted">{f.enabled ? t.protection.on : t.protection.off}</span>
              </div>
            ))}
          </div>
          <div className="row" style={{ padding: "16px 24px", borderTop: "1px solid var(--divider)" }}>
            {fwAllOn ? (
              <button
                className="btn btn-danger sm"
                title={!isAdmin ? t.common.needsAdmin : undefined}
                disabled={busy !== null || !st.data || !isAdmin}
                onClick={async () => {
                  const ok = await ask({
                    title: t.protection.fwOffTitle,
                    text: t.protection.fwOffText,
                    confirmLabel: t.protection.fwOff,
                    cancelLabel: t.common.cancel,
                    danger: true,
                  });
                  if (ok) void run("fw", () => api.firewallSet(false), t.protection.fwChanged);
                }}
              >
                {busy === "fw" ? <Spinner size={14} /> : null}
                {t.protection.fwOff}
              </button>
            ) : (
              <button
                className="btn btn-primary sm"
                title={!isAdmin ? t.common.needsAdmin : undefined}
                disabled={busy !== null || !isAdmin}
                onClick={() => void run("fw", () => api.firewallSet(true), t.protection.fwChanged)}
              >
                {busy === "fw" ? <Spinner size={14} /> : null}
                {t.protection.fwOnAll}
              </button>
            )}
          </div>
        </section>

        {/* Updates */}
        <section className="card flush">
          <div className="row" style={{ padding: "20px 24px", gap: 14 }}>
            <RefreshCcw size={22} strokeWidth={1.5} />
            <div className="section-title cell-main truncate" title={t.protection.updates}>{t.protection.updates}</div>
            <button className="btn btn-secondary sm" onClick={() => api.openTool("updates").catch(notify.err)}>
              {t.protection.openUpdates}
            </button>
          </div>
          <div className="list-head" style={{ borderTop: "1px solid var(--divider)" }}>{t.protection.updatesHistory}</div>
          <div className="list" style={{ maxHeight: 260, overflowY: "auto" }}>
            {fixes.error ? (
              <ErrorState error={fixes.error} onRetry={() => void fixes.reload()} />
            ) : !fixes.data ? (
              <div style={{ padding: 20, display: "grid", gap: 10 }}>
                <Skeleton h={12} />
                <Skeleton h={12} w="80%" />
              </div>
            ) : fixes.data.length === 0 ? (
              <div className="empty small">{t.protection.noUpdates}</div>
            ) : (
              fixes.data.slice(0, 20).map((f) => (
                <div key={f.id + f.installedOn} className="list-row" style={{ minHeight: 44 }}>
                  <span className="cell-main truncate" style={{ fontWeight: 500 }}>{f.id}</span>
                  <span className="small muted truncate" style={{ maxWidth: 160 }}>{f.description}</span>
                  <span className="num small faint">{f.installedOn}</span>
                </div>
              ))
            )}
          </div>
        </section>
      </div>
    </>
  );
}
