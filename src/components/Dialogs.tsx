import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import type { UpdateNotes, WhatsNew } from "../lib/types";
import { fmt, useT } from "../i18n";
import { api, errorText, subscribe } from "../lib/api";
import { useStore } from "../lib/store";
import { Bar, useCalm } from "./ui";

export const modalMotion = (calm: boolean) => ({
  initial: calm ? { opacity: 0 } : { opacity: 0, transform: "scale(0.96)" },
  animate: calm ? { opacity: 1 } : { opacity: 1, transform: "scale(1)" },
  exit: calm ? { opacity: 0 } : { opacity: 0, transform: "scale(0.98)" },
  transition: { duration: 0.2, ease: [0.23, 1, 0.32, 1] as const },
});

/**
 * Keyboard handling for modal dialogs: Tab stays inside, Escape calls onEscape,
 * initial focus goes to `initial`, and focus returns to the opener on close.
 */
export function useModalKeys(open: boolean, boxRef: React.RefObject<HTMLElement | null>, initial: React.RefObject<HTMLElement | null>, onEscape: () => void) {
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
let whatsNewOpen = false;

/** What changed in UpdateModal notes, as a list (shared with "What's new"). */
function NotesList({ sections, label }: { sections: UpdateNotes["sections"]; label: string }) {
  return (
    <div className="update-notes" tabIndex={0} role="region" aria-label={label}>
      {sections.map((s, i) => (
        <div key={i} className="update-section">
          {s.title ? <div className="update-section-title">{s.title}</div> : null}
          <ul>
            {s.items.map((it, j) => (
              <li key={j}>{it}</li>
            ))}
          </ul>
        </div>
      ))}
    </div>
  );
}

/** Shown once after StepMaster was updated: picture + "New / Fixed" of the version now running. */
export function WhatsNewModal() {
  const t = useT();
  const calm = useCalm();
  const [data, setData] = useState<WhatsNew | null>(null);
  const [bannerOk, setBannerOk] = useState(true);
  const boxRef = useRef<HTMLDivElement>(null);
  const okRef = useRef<HTMLButtonElement>(null);
  const open = data !== null;
  const close = () => {
    setData(null);
    whatsNewOpen = false;
    api.whatsNewSeen().catch(() => undefined);
    window.dispatchEvent(new Event("sm:whats-new-closed"));
  };
  useModalKeys(open, boxRef, okRef, close);

  useEffect(() => {
    let alive = true;
    const timer = setTimeout(() => {
      api
        .whatsNew()
        .then((w) => {
          if (!alive || !w) return;
          whatsNewOpen = true;
          setData(w);
        })
        .catch(() => undefined);
    }, 800);
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, []);

  const sections = (data?.notes.sections ?? []).filter((s) => s.items.length > 0);
  const banner = bannerOk ? data?.notes.image : null;

  return (
    <AnimatePresence>
      {open ? (
        <motion.div className="modal-backdrop" key="whats-new" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ duration: 0.2 }}>
          <motion.div
            ref={boxRef}
            className={`modal update-modal ${banner ? "has-banner" : ""}`}
            role="dialog"
            aria-modal="true"
            aria-label={t.whatsNew.title}
            {...modalMotion(calm)}
          >
            {banner ? (
              <div className="update-banner">
                <img src={banner} alt="" onError={() => setBannerOk(false)} />
              </div>
            ) : (
              <img src="/logo-mark.png" alt="" width={36} height={36} style={{ marginBottom: 20 }} />
            )}
            <div className="update-body">
              <h2 className="modal-title">{t.whatsNew.title}</h2>
              <p className="modal-text">{fmt(t.whatsNew.text, { v: data.version })}</p>
              <NotesList sections={sections} label={t.whatsNew.title} />
              <div className="modal-actions">
                <button ref={okRef} className="btn btn-primary" onClick={close}>
                  {t.whatsNew.ok}
                </button>
              </div>
            </div>
          </motion.div>
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}

export function UpdateModal() {
  const t = useT();
  const calm = useCalm();
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notes, setNotes] = useState<UpdateNotes | null>(null);
  const boxRef = useRef<HTMLDivElement>(null);
  const updateRef = useRef<HTMLButtonElement>(null);
  // "What's new" comes with the check; read it whenever the window opens.
  useEffect(() => {
    if (!open) {
      setNotes(null);
      return;
    }
    let alive = true;
    api
      .updateNotes()
      .then((n) => alive && setNotes(n))
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [open]);
  const sections = (notes?.sections ?? []).filter((s) => s.items.length > 0);
  const hasNotes = sections.length > 0;
  const [bannerOk, setBannerOk] = useState(true);
  useEffect(() => setBannerOk(true), [notes?.image]);
  const banner = bannerOk ? notes?.image : null;
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
          if (!alive || !r.available) return;
          // "What's new" is on screen: offer the next update after it's closed.
          if (whatsNewOpen) window.addEventListener("sm:whats-new-closed", () => alive && setOpen(true), { once: true });
          else setOpen(true);
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
    off = subscribe<{ percent: number }>("update://progress", (p) => setProgress(p.percent));
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
          <motion.div
            ref={boxRef}
            className={`modal update-modal ${banner ? "has-banner" : ""}`}
            role="dialog"
            aria-modal="true"
            aria-label={t.update.title}
            {...modalMotion(calm)}
          >
            {banner ? (
              <div className="update-banner">
                <img src={banner} alt="" onError={() => setBannerOk(false)} />
              </div>
            ) : (
              <img src="/logo-mark.png" alt="" width={36} height={36} style={{ marginBottom: 20 }} />
            )}
            <div className="update-body">
            <h2 className="modal-title">{t.update.title}</h2>
            <p className="modal-text">{t.update.text}</p>
            {hasNotes ? (
              <NotesList sections={sections} label={t.update.title} />
            ) : null}
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
            </div>
          </motion.div>
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}
