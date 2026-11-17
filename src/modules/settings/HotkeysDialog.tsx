import { AnimatePresence, motion } from "motion/react";
import { Fragment, useRef } from "react";
import { createPortal } from "react-dom";
import { modalMotion, useModalKeys } from "../../components/Dialogs";
import { useCalm } from "../../components/ui";
import { useT } from "../../i18n";
import { useStore } from "../../lib/store";

/** Keys as separate caps: "Ctrl+Shift+S" -> Ctrl, Shift, S. "/" separates alternatives. */
function Keys({ combo }: { combo: string }) {
  return (
    <span className="hotkey-keys">
      {combo.split(" / ").map((alt, i) => (
        <Fragment key={i}>
          {i > 0 ? <span className="hotkey-or">/</span> : null}
          {alt.split("+").map((k, j) => (
            <kbd key={j} className="kbd">
              {k}
            </kbd>
          ))}
        </Fragment>
      ))}
    </span>
  );
}

/** Every keyboard shortcut StepMaster has, in one list. */
export function HotkeysDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const t = useT();
  const calm = useCalm();
  const hotkey = useStore((s) => s.settings.hotkey);
  const boxRef = useRef<HTMLDivElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  useModalKeys(open, boxRef, closeRef, onClose);
  const h = t.settings.hk;

  const groups: { title: string; rows: { keys: string; text: string }[] }[] = [
    {
      title: h.gGlobal,
      rows: [{ keys: hotkey || "—", text: hotkey ? h.global : `${h.global} (${t.settings.hotkeyOff.toLowerCase()})` }],
    },
    {
      title: h.gApp,
      rows: [
        { keys: "Ctrl+K", text: h.palette },
        { keys: "Ctrl+B", text: h.sidebar },
        { keys: "Esc", text: h.close },
        { keys: "Tab / Shift+Tab", text: h.tab },
      ],
    },
    {
      title: h.gPalette,
      rows: [
        { keys: "↑ / ↓", text: h.move },
        { keys: "Enter", text: h.run },
      ],
    },
    {
      title: h.gWallpapers,
      rows: [
        { keys: "← / →", text: h.wallNav },
        { keys: "Enter", text: h.rename },
        { keys: "Esc", text: h.renameCancel },
      ],
    },
  ];

  // Rendered into <body>: the page itself is transformed (enter animation), which would pin a
  // fixed overlay to the page instead of the window.
  return createPortal(
    <AnimatePresence>
      {open ? (
        <motion.div className="modal-backdrop" key="hotkeys" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ duration: 0.2 }} onClick={onClose}>
          <motion.div
            ref={boxRef}
            className="modal hotkeys-modal"
            role="dialog"
            aria-modal="true"
            aria-label={h.title}
            onClick={(e) => e.stopPropagation()}
            {...modalMotion(calm)}
          >
            <h2 className="modal-title">{h.title}</h2>
            <div className="hotkeys-list">
              {groups.map((g) => (
                <div key={g.title} className="hotkeys-group">
                  <div className="update-section-title">{g.title}</div>
                  {g.rows.map((r, i) => (
                    <div key={i} className="hotkey-row">
                      <span>{r.text}</span>
                      <Keys combo={r.keys} />
                    </div>
                  ))}
                </div>
              ))}
            </div>
            <div className="modal-actions">
              <button ref={closeRef} className="btn btn-secondary" onClick={onClose}>
                {t.common.close}
              </button>
            </div>
          </motion.div>
        </motion.div>
      ) : null}
    </AnimatePresence>,
    document.body,
  );
}
