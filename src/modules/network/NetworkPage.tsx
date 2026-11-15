import { Cable, Copy, Eye, EyeOff, Gauge, Globe, Layers, RotateCcw, Wifi } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { Empty, ErrorState, PageHeader, Ring, SearchInput, SkeletonRows, Spinner, Tabs, notify } from "../../components/ui";
import { useLang, useT } from "../../i18n";
import { api, on } from "../../lib/api";
import { formatDecimal } from "../../lib/format";
import { useAsync } from "../../lib/hooks";
import { ask, useStore } from "../../lib/store";
import type { DnsPreset, SpeedProgress, SpeedResult } from "../../lib/types";

type Tab = "overview" | "speed" | "connections" | "wifi";
const TABS: Tab[] = ["overview", "speed", "connections", "wifi"];

function Overview() {
  const t = useT();
  const list = useAsync(api.adapters);
  const [busy, setBusy] = useState<string | null>(null);
  const [preset, setPreset] = useState<DnsPreset>("auto");
  const isAdmin = useStore((s) => s.app?.isAdmin ?? true);

  const run = async (key: string, fn: () => Promise<void>, ok: string) => {
    setBusy(key);
    try {
      await fn();
      notify.ok(ok);
      void list.reload();
    } catch (e) {
      notify.err(e);
    } finally {
      setBusy(null);
    }
  };

  const presets: { id: DnsPreset; label: string }[] = [
    { id: "auto", label: t.network.dnsAuto },
    { id: "cloudflare", label: t.network.dnsCloudflare },
    { id: "google", label: t.network.dnsGoogle },
    { id: "quad9", label: t.network.dnsQuad9 },
    { id: "adguard", label: t.network.dnsAdguard },
  ];

  return (
    <>
      <div className="section-title">{t.network.adapters}</div>
      {list.error ? (
        <div className="card"><ErrorState error={list.error} onRetry={() => void list.reload()} /></div>
      ) : !list.data ? (
        <div className="card flush"><SkeletonRows rows={2} /></div>
      ) : (
        <div className="grid-2">
          {list.data.map((a) => {
            const up = a.status.toLowerCase() === "up";
            const Icon = a.kind === "wifi" ? Wifi : a.kind === "ethernet" ? Cable : Globe;
            return (
              <div key={a.name} className="card" style={{ opacity: up ? 1 : 0.6 }}>
                <div className="row">
                  <Icon size={20} strokeWidth={1.6} />
                  <div className="cell-main">
                    <div className="cell-title">{a.name}</div>
                    <div className="cell-sub" title={a.description}>{a.description}</div>
                  </div>
                  <span className={`badge ${up ? "solid" : ""}`}>{up ? t.network.connected : t.network.disconnected}</span>
                </div>
                {up ? (
                  <dl className="kv" style={{ marginTop: 18, gridTemplateColumns: "130px 1fr", userSelect: "text" }}>
                    <dt>{t.network.ip}</dt>
                    <dd className="num">{a.ipv4.join(", ") || "—"}</dd>
                    <dt>{t.network.gateway}</dt>
                    <dd className="num">{a.gateway || "—"}</dd>
                    <dt>{t.network.dns}</dt>
                    <dd className="num">{a.dns.join(", ") || "—"}</dd>
                    <dt>{t.network.mac}</dt>
                    <dd className="num">{a.mac || "—"}</dd>
                    <dt>{t.network.link}</dt>
                    <dd className="num">{a.linkSpeed || "—"}</dd>
                  </dl>
                ) : null}
              </div>
            );
          })}
        </div>
      )}

      <div className="section-title" style={{ marginTop: 8 }}>{t.network.tools}</div>
      <section className="card flush">
        <div className="setting-row">
          <Layers size={20} strokeWidth={1.6} />
          <div className="setting-text">
            <div className="setting-title">{t.network.flush}</div>
            <div className="setting-desc">{t.network.flushDesc}</div>
          </div>
          <button className="btn btn-secondary sm" disabled={busy !== null} onClick={() => void run("flush", api.dnsFlush, t.network.flushed)}>
            {busy === "flush" ? <Spinner size={14} /> : null}
            {t.network.flush}
          </button>
        </div>
        <div className="setting-row">
          <Globe size={20} strokeWidth={1.6} />
          <div className="setting-text">
            <div className="setting-title">{t.network.dnsTitle}</div>
            <div className="setting-desc">{t.network.dnsDesc}</div>
          </div>
          <select className="select" style={{ height: 32, fontSize: 13 }} value={preset} onChange={(e) => setPreset(e.target.value as DnsPreset)} aria-label={t.network.dnsTitle}>
            {presets.map((p) => (
              <option key={p.id} value={p.id}>{p.label}</option>
            ))}
          </select>
          <button
            className="btn btn-primary sm"
            title={!isAdmin ? t.common.needsAdmin : undefined}
            disabled={busy !== null || !isAdmin}
            onClick={() => void run("dns", () => api.dnsSet(preset), t.network.dnsApplied)}
          >
            {busy === "dns" ? <Spinner size={14} /> : null}
            {t.common.apply}
          </button>
        </div>
        <div className="setting-row">
          <RotateCcw size={20} strokeWidth={1.6} />
          <div className="setting-text">
            <div className="setting-title">{t.network.reset}</div>
            <div className="setting-desc">{t.network.resetDesc}</div>
          </div>
          <button
            className="btn btn-danger sm"
            title={!isAdmin ? t.common.needsAdmin : undefined}
            disabled={busy !== null || !isAdmin}
            onClick={async () => {
              const ok = await ask({ title: t.network.resetTitle, text: t.network.resetText, confirmLabel: t.network.reset, cancelLabel: t.common.cancel, danger: true });
              if (ok) void run("reset", api.networkReset, t.network.resetDone);
            }}
          >
            {busy === "reset" ? <Spinner size={14} /> : null}
            {t.network.reset}
          </button>
        </div>
      </section>
    </>
  );
}

// Speed test result survives tab switches.
let lastSpeed: SpeedResult | null = null;

function Speed() {
  const t = useT();
  const lang = useLang();
  const [result, setResult] = useState<SpeedResult | null>(lastSpeed);
  const [live, setLive] = useState<SpeedProgress | null>(null);
  const [running, setRunning] = useState(false);
  const alive = useRef(true);

  useEffect(() => {
    alive.current = true;
    let off: (() => void) | undefined;
    void on<SpeedProgress>("speed://progress", (p) => alive.current && setLive(p)).then((u) => (off = u));
    return () => {
      alive.current = false;
      off?.();
    };
  }, []);

  const start = async () => {
    setRunning(true);
    setLive(null);
    setResult(null);
    try {
      const r = await api.speedTest();
      lastSpeed = r;
      if (alive.current) setResult(r);
    } catch (e) {
      notify.err(e);
    } finally {
      if (alive.current) {
        setRunning(false);
        setLive(null);
      }
    }
  };

  const phaseLabel = live?.phase === "ping" ? t.network.phasePing : live?.phase === "download" ? t.network.phaseDown : live?.phase === "upload" ? t.network.phaseUp : "";
  const d1 = (v: number) => formatDecimal(v, 1, lang);
  const center = running ? (live && live.phase !== "ping" ? d1(live.value) : "…") : result ? d1(result.downMbps) : "—";

  return (
    <section className="card" style={{ display: "grid", justifyItems: "center", gap: 28, padding: "40px 28px" }}>
      <Ring value={running ? 0 : result ? 100 : 0} spinning={running} size={220}>
        <div style={{ display: "grid", justifyItems: "center", gap: 2 }}>
          <span className="num" style={{ fontSize: 44, fontWeight: 300, letterSpacing: "-0.03em" }}>{center}</span>
          <span className="muted small">{running ? phaseLabel : t.network.mbps}</span>
        </div>
      </Ring>
      <div className="grid-3" style={{ width: "min(560px, 100%)" }}>
        {[
          { label: t.network.ping, value: result ? `${Math.round(result.pingMs)} ${t.network.ms}` : "—" },
          { label: t.network.down, value: result ? `${d1(result.downMbps)} ${t.network.mbps}` : "—" },
          { label: t.network.up, value: result ? `${d1(result.upMbps)} ${t.network.mbps}` : "—" },
        ].map((m) => (
          <div key={m.label} style={{ textAlign: "center" }}>
            <div className="card-label">{m.label}</div>
            <div className="num" style={{ fontSize: 20, fontWeight: 500, marginTop: 6 }}>{m.value}</div>
          </div>
        ))}
      </div>
      <button className="btn btn-primary lg" disabled={running} onClick={() => void start()}>
        {running ? <Spinner /> : <Gauge size={18} />}
        {result ? t.network.speedAgain : t.network.speedStart}
      </button>
      <div className="small faint">{t.network.speedNote}</div>
    </section>
  );
}

function Connections() {
  const t = useT();
  const list = useAsync(api.connections);
  const [q, setQ] = useState("");
  const rows = useMemo(() => {
    const n = q.trim().toLowerCase();
    return (list.data ?? []).filter((c) => !n || `${c.process} ${c.remote} ${c.local}`.toLowerCase().includes(n));
  }, [list.data, q]);

  return (
    <>
      <div className="row">
        <SearchInput value={q} onChange={setQ} placeholder={t.network.connSearch} width={340} />
        <span className="spacer" />
        <button className="btn btn-secondary sm" disabled={list.loading} onClick={() => void list.reload()}>{t.common.refresh}</button>
      </div>
      <section className="card flush">
        <div className="list-head">
          <span style={{ flex: 1 }}>{t.network.colProcess}</span>
          <span style={{ width: 190 }}>{t.network.colLocal}</span>
          <span style={{ width: 210 }}>{t.network.colRemote}</span>
          <span style={{ width: 110 }}>{t.network.colState}</span>
        </div>
        {list.error ? (
          <ErrorState error={list.error} onRetry={() => void list.reload()} />
        ) : !list.data ? (
          <SkeletonRows rows={6} />
        ) : rows.length === 0 ? (
          <Empty text={t.network.connEmpty} />
        ) : (
          <div className="list">
            {rows.slice(0, 300).map((c, i) => (
              <div key={`${c.pid}-${c.local}-${c.remote}-${i}`} className="list-row" style={{ minHeight: 44, fontSize: 13 }}>
                <span className="cell-main truncate" style={{ fontWeight: 500 }}>{c.process}</span>
                <span className="num muted truncate" style={{ width: 190, userSelect: "text" }}>{c.local}</span>
                <span className="num truncate" style={{ width: 210, userSelect: "text" }}>{c.remote}</span>
                <span className="muted truncate" style={{ width: 110 }}>
                  {c.state.toLowerCase() === "established" ? t.network.stEstablished : c.state}
                </span>
              </div>
            ))}
          </div>
        )}
      </section>
    </>
  );
}

function WifiPasswords() {
  const t = useT();
  const list = useAsync(api.wifiProfiles);
  const [shown, setShown] = useState<Set<string>>(new Set());

  return (
    <>
      <div className="notice">{t.network.wifiHint}</div>
      <section className="card flush">
        {list.error ? (
          <ErrorState error={list.error} onRetry={() => void list.reload()} />
        ) : !list.data ? (
          <SkeletonRows rows={3} />
        ) : list.data.length === 0 ? (
          <Empty icon={<Wifi size={26} />} text={t.network.wifiNone} />
        ) : (
          <div className="list">
            {list.data.map((w) => {
              const visible = shown.has(w.ssid);
              return (
                <div key={w.ssid} className="list-row">
                  <Wifi size={18} strokeWidth={1.6} />
                  <div className="cell-main">
                    <div className="cell-title">{w.ssid}</div>
                    <div className="cell-sub">{w.auth}</div>
                  </div>
                  {w.password ? (
                    <>
                      <span className="num" style={{ fontFamily: visible ? "inherit" : undefined, userSelect: "text", minWidth: 120, textAlign: "right" }}>
                        {visible ? w.password : "••••••••••"}
                      </span>
                      <button
                        className="icon-btn"
                        aria-label={visible ? t.common.hide : t.network.showPassword}
                        onClick={() =>
                          setShown((s) => {
                            const n = new Set(s);
                            if (n.has(w.ssid)) n.delete(w.ssid);
                            else n.add(w.ssid);
                            return n;
                          })
                        }
                      >
                        {visible ? <EyeOff size={16} /> : <Eye size={16} />}
                      </button>
                      <button
                        className="icon-btn"
                        aria-label={t.common.copy}
                        onClick={() => navigator.clipboard.writeText(w.password ?? "").then(() => notify.ok(t.common.copied), notify.err)}
                      >
                        <Copy size={16} />
                      </button>
                    </>
                  ) : (
                    <span className="small muted">{t.network.wifiOpen}</span>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </section>
    </>
  );
}

export function NetworkPage() {
  const t = useT();
  const [tab, setTab] = useState<Tab>(() => {
    const p = useStore.getState().takePendingTab();
    return TABS.includes(p as Tab) ? (p as Tab) : "overview";
  });

  return (
    <>
      <PageHeader title={t.network.title} sub={t.network.sub} />
      <Tabs
        value={tab}
        onChange={setTab}
        items={[
          { id: "overview", label: t.network.tabOverview },
          { id: "speed", label: t.network.tabSpeed },
          { id: "connections", label: t.network.tabConnections },
          { id: "wifi", label: t.network.tabWifi },
        ]}
      />
      {tab === "overview" ? <Overview /> : null}
      {tab === "speed" ? <Speed /> : null}
      {tab === "connections" ? <Connections /> : null}
      {tab === "wifi" ? <WifiPasswords /> : null}
    </>
  );
}
