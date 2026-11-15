import { Battery, Cpu, ExternalLink, MemoryStick, Monitor, MonitorSmartphone } from "lucide-react";
import { ErrorState, Skeleton, notify } from "../../components/ui";
import { fmt, useLang, useT } from "../../i18n";
import { api } from "../../lib/api";
import { cached } from "../../lib/cache";
import { formatBytes, formatDuration, pct } from "../../lib/format";
import { useAsync } from "../../lib/hooks";

export function Overview() {
  const t = useT();
  const lang = useLang();
  const ov = useAsync(() => cached("overview", 5 * 60_000, api.overview));

  const tools: { id: string; label: string }[] = [
    { id: "taskmgr", label: t.system.toolTaskmgr },
    { id: "devmgmt", label: t.system.toolDevmgmt },
    { id: "control", label: t.system.toolControl },
    { id: "msinfo", label: t.system.toolMsinfo },
    { id: "diskmgmt", label: t.system.toolDiskmgmt },
    { id: "services", label: t.system.toolServices },
  ];

  if (ov.error) return <div className="card"><ErrorState error={ov.error} onRetry={() => void ov.reload()} /></div>;
  const d = ov.data;
  const health = d?.battery?.designCapacity && d.battery.fullCapacity ? pct(d.battery.fullCapacity, d.battery.designCapacity) : null;

  const cards = [
    { icon: Cpu, label: t.system.cpu, value: d?.cpuName, meta: d ? fmt(t.system.cores, { c: d.cores, t: d.threads }) : "" },
    { icon: Monitor, label: t.system.gpu, value: d ? (d.gpus[0] ?? "—") : undefined, meta: d && d.gpus.length > 1 ? d.gpus.slice(1).join(", ") : "" },
    { icon: MemoryStick, label: t.system.memory, value: d ? formatBytes(d.memTotal, lang) : undefined, meta: "" },
    { icon: MonitorSmartphone, label: t.system.os, value: d ? d.osName : undefined, meta: d ? `${d.osVersion} · ${t.system.build} ${d.osBuild}` : "" },
  ];

  return (
    <>
      <div className="grid-2">
        {cards.map((c) => {
          const Icon = c.icon;
          return (
            <div key={c.label} className="card">
              <div className="row card-label">
                <Icon size={18} strokeWidth={1.6} />
                {c.label}
              </div>
              {c.value === undefined ? (
                <div style={{ marginTop: 14, display: "grid", gap: 8 }}>
                  <Skeleton h={20} w="70%" />
                  <Skeleton h={12} w="40%" />
                </div>
              ) : (
                <>
                  <div style={{ fontSize: 20, fontWeight: 500, marginTop: 10, letterSpacing: "-0.01em", userSelect: "text" }} className="truncate" title={c.value}>
                    {c.value}
                  </div>
                  <div className="card-meta truncate">{c.meta || " "}</div>
                </>
              )}
            </div>
          );
        })}
      </div>

      <div className="card">
        {d ? (
          <dl className="kv">
            <dt>{t.system.host}</dt>
            <dd style={{ userSelect: "text" }}>{d.hostName}</dd>
            <dt>{t.system.board}</dt>
            <dd style={{ userSelect: "text" }}>{d.board || "—"}</dd>
            <dt>{t.system.uptime}</dt>
            <dd>{formatDuration(d.uptimeSec, lang)}</dd>
            {d.battery ? (
              <>
                <dt className="row" style={{ gap: 6 }}>
                  <Battery size={15} /> {t.system.battery}
                </dt>
                <dd>
                  {d.battery.percent}% · {d.battery.charging ? t.system.charging : t.system.onBattery}
                </dd>
                {health !== null ? (
                  <>
                    <dt>{t.system.batteryHealth}</dt>
                    <dd>{health}%</dd>
                  </>
                ) : null}
              </>
            ) : null}
          </dl>
        ) : (
          <div style={{ display: "grid", gap: 12 }}>
            <Skeleton h={14} w="50%" />
            <Skeleton h={14} w="60%" />
            <Skeleton h={14} w="30%" />
          </div>
        )}
      </div>

      <div className="section-title" style={{ marginTop: 8 }}>{t.system.tools}</div>
      <div className="grid-3">
        {tools.map((tool) => (
          <button key={tool.id} className="card tight row" onClick={() => api.openTool(tool.id).catch(notify.err)}>
            <span className="cell-main truncate" style={{ fontWeight: 500 }}>{tool.label}</span>
            <ExternalLink size={15} className="faint" />
          </button>
        ))}
      </div>
    </>
  );
}
