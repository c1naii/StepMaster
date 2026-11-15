import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { fmt, useT } from "../i18n";
import { api, errorText, on } from "../lib/api";
import { useStore } from "../lib/store";
import { Bar, useCalm } from "./ui";

const modalMotion = (calm: boolean) => ({
  initial: calm ? { opacity: 0 } : { opacity: 0, transform: "scale(0.96)" },
  animate: calm ? { opacity: 1 } : { opacity: 1, transform: "scale(1)" },
  exit: calm ? { opacity: 0 } : { opacity: 0, transform: "scale(0.98)" },
  transition: { duration: 0.2, ease: [0.23, 1, 0.32, 1] as const },
});

/**
 * Keyboard handling for modal dialogs: Tab stays inside, Escape calls onEscape,
 * initial focus goes to `initial`, and focus returns to the opener on close.
 */
function useModalKeys(open: boolean, boxRef: React.RefObject<HTMLElement | null>, initial: React.RefObject<HTMLElement | null>, onEscape: () => void) {
  const escRef = useRef(onEscape);
  escRef.current = onEscape;
  useEffect(() => {
    if (!open) return;
    const opener = document.activeElement as HTMLElement | null;
    const t = setTimeout(() => initial.current?.focus(), 30);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        escRef.current();
        return;
      }
      if (e.key !== "Tab" || !boxRef.current) return;
      const items = Array.from(boxRef.current.querySelectorAll<HTMLElement>("button:not(:disabled), [tabindex]:not([tabindex='-1'])"));
      if (items.length === 0) return;
      const first = items[0];
      const last = items[items.length - 1];
      const active = document.activeElement as HTMLElement | null;
      if (e.shiftKey && (active === first || !boxRef.current.contains(active))) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && (active === last || !boxRef.current.contains(active))) {
        e.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => {
      clearTimeout(t);
      window.removeEventListener("keydown", onKey, true);
      if (opener && document.contains(opener)) opener.focus();
    };
  }, [open, boxRef, initial]);
}

/** Global confirmation dialog driven by store.ask(). */
export function ConfirmDialog() {
  const req = useStore((s) => s.confirm);
  const close = useStore((s) => s.closeConfirm);
  const calm = useCalm();
  const okRef = useRef<HTMLButtonElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const boxRef = useRef<HTMLDivElement>(null);
  // Dangerous dialogs start on "Cancel", so Enter never deletes anything by accident.
  const initial = req?.danger ? cancelRef : okRef;
  useModalKeys(Boolean(req), boxRef, initial, () => close(false));

  return (
    <AnimatePresence>
      {req ? (
        <motion.div
          className="modal-backdrop"
          key="confirm"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.16 }}
          onMouseDown={(e) => {
            if (e.target === e.currentTarget) close(false);
          }}
        >
          <motion.div ref={boxRef} className="modal" role="dialog" aria-modal="true" aria-label={req.title} {...modalMotion(calm)}>
            <h2 className="modal-title">{req.title}</h2>
            {req.text ? <p className="modal-text">{req.text}</p> : null}
            {req.details && req.details.length > 0 ? (
              <div
                className="card tight"
                style={{ marginTop: 16, maxHeight: 180, overflowY: "auto", fontSize: 13, color: "var(--text-2)", userSelect: "text" }}
              >
                {req.details.map((d) => (
                  <div key={d} style={{ overflowWrap: "anywhere", padding: "3px 0" }}>
                    {d}
                  </div>
                ))}
              </div>
            ) : null}
            <div className="modal-actions">
              <button ref={cancelRef} className="btn btn-secondary" onClick={() => close(false)}>
                {req.cancelLabel}
              </button>
              <button ref={okRef} className={`btn ${req.danger ? "btn-danger" : "btn-primary"}`} onClick={() => close(true)}>
                {req.confirmLabel}
              </button>
            </div>
          </motion.div>
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}

/** "Нужно обновить программу" — shown once per launch when GitHub has a newer build. */
export function UpdateModal() {
  const t = useT();
  const calm = useCalm();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const boxRef = useRef<HTMLDivElement>(null);
  const updateRef = useRef<HTMLButtonElement>(null);
  useModalKeys(open, boxRef, updateRef, () => {
    if (!busy) setOpen(false);
  });

  useEffect(() => {
    let alive = true;
    // Small delay so startup stays smooth; failures (offline, no repo) are silent by design.
    const timer = setTimeout(() => {
      api
        .updateCheck()
        .then((r) => {
          if (alive && r.available) setOpen(true);
        })
        .catch(() => undefined);
    }, 2500);
    const manual = () => setOpen(true);
    window.addEventListener("sm:update-available", manual);
    return () => {
      alive = false;
      clearTimeout(timer);
      window.removeEventListener("sm:update-available", manual);
    };
  }, []);

  useEffect(() => {
    let off: (() => void) | undefined;
    void on<{ percent: number }>("update://progress", (p) => setProgress(p.percent)).then((u) => (off = u));
    return () => off?.();
  }, []);

  const start = async () => {
    setBusy(true);
    setError(null);
    setProgress(0);
    try {
      await api.updateInstall(); // backend restarts the app on success
    } catch (e) {
      setError(errorText(e));
      setBusy(false);
      setProgress(null);
    }
  };

  return (
    <AnimatePresence>
      {open ? (
        <motion.div className="modal-backdrop" key="update" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ duration: 0.2 }}>
          <motion.div ref={boxRef} className="modal" role="dialog" aria-modal="true" aria-label={t.update.title} {...modalMotion(calm)}>
            <img src="/logo-mark.png" alt="" width={36} height={36} style={{ marginBottom: 20 }} />
            <h2 className="modal-title">{t.update.title}</h2>
            <p className="modal-text">{t.update.text}</p>
            {busy ? (
              <div style={{ marginTop: 22, display: "grid", gap: 10 }}>
                <div className="small muted">{t.update.downloading}</div>
                <Bar value={progress ?? 0} thin indeterminate={progress === null} />
              </div>
            ) : null}
            {error ? (
              <div className="notice warn" style={{ marginTop: 18 }}>
                {fmt(t.update.failed, { e: error })}
              </div>
            ) : null}
            <div className="modal-actions">
              <button className="btn btn-secondary" disabled={busy} onClick={() => setOpen(false)}>
                {t.update.later}
              </button>
              <button ref={updateRef} className="btn btn-primary" disabled={busy} onClick={() => void start()}>
                {t.update.update}
              </button>
            </div>
          </motion.div>
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}
