import { X } from "lucide-react";
import { useEffect, useState } from "react";
import { fmt, useT } from "../i18n";
import { api, errorText, subscribe } from "../lib/api";
import { useStore } from "../lib/store";
import type { UpdateNotes } from "../lib/types";
import { Spinner } from "./ui";

/** How many "what's new" lines fit in the popup; the rest becomes "N more changes". */
const SHOWN = 2;

/** The small update popup in the bottom-right corner (its own window, see notify.rs). */
export function UpdateToast() {
  const t = useT();
  const setSettings = useStore((s) => s.setSettings);
  const [notes, setNotes] = useState<UpdateNotes | null>(null);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    // The window is transparent: only the rounded card is visible.
    document.documentElement.style.background = "transparent";
    document.body.style.background = "transparent";
    let alive = true;
    Promise.all([api.settingsGet().catch(() => null), api.updateNotes().catch(() => null)]).then(([s, n]) => {
      if (!alive) return;
      if (s) setSettings(s);
      setNotes(n ?? { sections: [], image: null });
      // Shown only once drawn, so it never flashes empty.
      requestAnimationFrame(() => void api.toastReady().catch(() => undefined));
    });
    const off = subscribe<{ percent: number }>("update://progress", (p) => setProgress(p.percent));
    return () => {
      alive = false;
      off();
    };
  }, [setSettings]);

  // Short form of each line: the part before ":" or " — " ("Monitor refresh rate: …" → "Monitor refresh rate").
  const items = (notes?.sections ?? []).flatMap((s) => s.items).map((it) => it.split(/: | — /)[0]);
  const more = items.length - SHOWN;

  const update = async () => {
    setBusy(true);
    setError(null);
    setProgress(0);
    try {
      await api.updateInstall(); // the backend restarts StepMaster on success
    } catch (e) {
      setError(errorText(e));
      setBusy(false);
      setProgress(null);
    }
  };

  const close = () => void api.toastClose().catch(() => undefined);

  if (!notes) return null;
  return (
    <div className="update-toast" role="dialog" aria-label={t.update.title}>
      <div className="update-toast-head">
        <img src="/logo-mark.png" alt="" className="update-toast-logo" />
        <span className="update-toast-app">StepMaster</span>
        {!busy ? (
          <button className="icon-btn" aria-label={t.common.close} onClick={close}>
            <X size={16} />
          </button>
        ) : null}
      </div>
      <div className="update-toast-title">{t.update.title}</div>
      <div className="update-toast-text">{busy ? t.toast.updating : t.toast.text}</div>
      {items.length > 0 && !busy ? (
        <ul className="update-toast-list">
          {items.slice(0, SHOWN).map((it) => (
            <li key={it} className="truncate" title={it}>
              {it}
            </li>
          ))}
          {more > 0 ? <li className="update-toast-more">{fmt(t.toast.more, { n: more })}</li> : null}
        </ul>
      ) : null}
      {busy && progress !== null ? (
        <div className="update-toast-bar" aria-hidden>
          <span style={{ width: `${Math.round(progress)}%` }} />
        </div>
      ) : null}
      {error ? <div className="update-toast-error">{fmt(t.update.failed, { e: error })}</div> : null}
      <div className="update-toast-actions">
        {!busy ? (
          <button className="btn btn-secondary sm" onClick={close}>
            {t.update.later}
          </button>
        ) : null}
        <button className="btn btn-primary sm" disabled={busy} onClick={() => void update()}>
          {busy ? <Spinner /> : null}
          {busy ? `${Math.round(progress ?? 0)}%` : t.update.update}
        </button>
      </div>
    </div>
  );
}
