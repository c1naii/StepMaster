import { Power } from "lucide-react";
import { useState } from "react";
import { Empty, ErrorState, SkeletonRows, Toggle, notify } from "../../components/ui";
import { fmt, useT } from "../../i18n";
import { api } from "../../lib/api";
import { invalidate } from "../../lib/cache";
import { AppIcon } from "../../components/AppIcon";
import { useAsync } from "../../lib/hooks";

export function Startup() {
  const t = useT();
  const list = useAsync(api.startupList);
  const [pending, setPending] = useState<string | null>(null);

  const set = async (id: string, name: string, enabled: boolean) => {
    setPending(id);
    // Optimistic update, rolled back on error.
    list.setData((list.data ?? []).map((s) => (s.id === id ? { ...s, enabled } : s)));
    try {
      await api.startupSet(id, enabled);
      invalidate("startup");
      notify.ok(fmt(enabled ? t.system.startupOn : t.system.startupOff, { name }));
    } catch (e) {
      notify.err(e);
      void list.reload();
    } finally {
      setPending(null);
    }
  };

  return (
    <>
      <div className="notice">{t.system.startupHint}</div>
      <section className="card flush">
        {list.error ? (
          <ErrorState error={list.error} onRetry={() => void list.reload()} />
        ) : !list.data ? (
          <SkeletonRows rows={5} />
        ) : list.data.length === 0 ? (
          <Empty icon={<Power size={26} />} text={t.system.startupEmpty} />
        ) : (
          <div className="list">
            {list.data.map((s) => (
              <div key={s.id} className="list-row">
                <AppIcon spec={s.exePath} name={s.name} />
                <div className="cell-main">
                  <div className="cell-title">{s.name}</div>
                  <div className="cell-sub" title={s.command}>{s.command}</div>
                </div>
                <span className="badge">{s.location}</span>
                <Toggle checked={s.enabled} disabled={pending !== null} label={s.name} onChange={(v) => void set(s.id, s.name, v)} />
              </div>
            ))}
          </div>
        )}
      </section>
    </>
  );
}
