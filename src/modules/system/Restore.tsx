import { History, ShieldPlus } from "lucide-react";
import { useState } from "react";
import { Empty, ErrorState, SkeletonRows, Spinner, notify } from "../../components/ui";
import { useT } from "../../i18n";
import { api } from "../../lib/api";
import { useAsync } from "../../lib/hooks";
import { useStore } from "../../lib/store";

export function Restore() {
  const t = useT();
  const st = useAsync(api.restoreStatus);
  const [busy, setBusy] = useState(false);
  const isAdmin = useStore((s) => s.app?.isAdmin ?? true);

  const create = async () => {
    setBusy(true);
    try {
      const res = await api.restoreCreate(t.system.restoreDescription);
      if (res === "skipped") notify.info(t.system.restoreSkipped);
      else notify.ok(t.system.restoreCreated);
      void st.reload();
    } catch (e) {
      notify.err(e);
    } finally {
      setBusy(false);
    }
  };

  const enable = async () => {
    setBusy(true);
    try {
      await api.restoreEnable();
      void st.reload();
    } catch (e) {
      notify.err(e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <section className="card row" style={{ gap: 24, alignItems: "center" }}>
        <ShieldPlus size={28} strokeWidth={1.5} />
        <div className="cell-main">
          <div className="section-title">{t.system.restoreTitle}</div>
          <div className="small muted" style={{ marginTop: 4, whiteSpace: "normal" }}>{t.system.restoreText}</div>
        </div>
        <button className="btn btn-secondary" onClick={() => api.openTool("rstrui").catch(notify.err)}>
          {t.system.restoreOpen}
        </button>
        {st.data && !st.data.enabled ? (
          <button className="btn btn-primary" disabled={busy} onClick={() => void enable()}>
            {busy ? <Spinner /> : null}
            {t.system.restoreEnable}
          </button>
        ) : (
          <button className="btn btn-primary" disabled={busy || !st.data || !isAdmin} onClick={() => void create()}>
            {busy ? <Spinner /> : null}
            {busy ? t.system.restoreCreating : t.system.restoreCreate}
          </button>
        )}
      </section>

      {st.data && !st.data.enabled ? <div className="notice warn">{t.system.restoreDisabled}</div> : null}

      <section className="card flush">
        {st.error ? (
          <ErrorState error={st.error} onRetry={() => void st.reload()} />
        ) : !st.data ? (
          <SkeletonRows rows={3} />
        ) : st.data.points.length === 0 ? (
          <Empty icon={<History size={26} />} text={t.system.restoreNone} />
        ) : (
          <div className="list">
            {[...st.data.points].reverse().map((p) => (
              <div key={p.sequence} className="list-row">
                <History size={18} strokeWidth={1.6} className="faint" />
                <div className="cell-main">
                  <div className="cell-title">{p.description}</div>
                </div>
                <div className="num small muted">{p.createdAt}</div>
              </div>
            ))}
          </div>
        )}
      </section>
    </>
  );
}
