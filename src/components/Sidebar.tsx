import { animate, motion, useMotionValue, useSpring, useTransform, useVelocity } from "motion/react";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useT } from "../i18n";
import { useStore, type ModuleId } from "../lib/store";
import { MODULES } from "../modules/registry";
import { useCalm } from "./ui";

/** Movement (px) before a press turns into a drag; a plain click stays a click. */
const DRAG_THRESHOLD = 5;
/** Holding still this long also "lifts" the lens, like a long-press. */
const HOLD_MS = 180;

/** Soft resistance past the first/last item instead of a hard stop. */
function rubberband(overshoot: number, dimension: number, k = 0.55) {
  return (overshoot * dimension * k) / (dimension + k * Math.abs(overshoot));
}

/**
 * Sidebar with a single "liquid glass" indicator.
 * Click: the indicator glides to the item. Press and drag (or hold, then drag): the indicator
 * lifts into a glass lens that follows the pointer with spring inertia and stretches with speed.
 * Release: the section under the lens opens.
 */
export function Sidebar() {
  const t = useT();
  const route = useStore((s) => s.route);
  const navigate = useStore((s) => s.navigate);
  const app = useStore((s) => s.app);
  const calm = useCalm();

  const navRef = useRef<HTMLElement>(null);
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const [lifted, setLifted] = useState(false);
  const [hover, setHover] = useState<number | null>(null);
  const suppressClick = useRef(false);
  const press = useRef<{ id: number; startY: number; grab: number; index: number; dragging: boolean; timer: number } | null>(null);

  // Indicator position: raw target -> spring (inertia) -> velocity-based stretch.
  const targetY = useMotionValue(0);
  const y = useSpring(targetY, calm ? { stiffness: 2000, damping: 200 } : { stiffness: 520, damping: 42, mass: 0.9 });
  const vel = useVelocity(y);
  const stretchY = useTransform(vel, [-1800, 0, 1800], calm ? [1, 1, 1] : [1.14, 1, 1.14]);
  const stretchX = useTransform(vel, [-1800, 0, 1800], calm ? [1, 1, 1] : [0.96, 1, 0.96]);
  const lift = useSpring(1, { stiffness: 600, damping: 32 });
  const scaleY = useTransform([lift, stretchY], ([l, s]: number[]) => l * s);
  const scaleX = useTransform([lift, stretchX], ([l, s]: number[]) => l * s);

  const activeIndex = Math.max(0, MODULES.findIndex((m) => m.id === route));
  const topOf = useCallback((i: number) => itemRefs.current[i]?.offsetTop ?? 0, []);
  const itemHeight = () => itemRefs.current[0]?.offsetHeight ?? 44;

  // Place the indicator on the active item without animating on first paint.
  const placed = useRef(false);
  useLayoutEffect(() => {
    const top = topOf(activeIndex);
    if (!placed.current) {
      placed.current = true;
      targetY.jump(top);
      y.jump(top);
      return;
    }
    if (!press.current?.dragging) targetY.set(top);
  }, [activeIndex, topOf, targetY, y]);

  useEffect(() => {
    lift.set(lifted && !calm ? 1.045 : 1);
  }, [lifted, calm, lift]);

  const indexAt = (center: number) => {
    let best = 0;
    let dist = Infinity;
    itemRefs.current.forEach((el, i) => {
      if (!el) return;
      const d = Math.abs(el.offsetTop + el.offsetHeight / 2 - center);
      if (d < dist) {
        dist = d;
        best = i;
      }
    });
    return best;
  };

  const pointerY = (e: React.PointerEvent) => {
    const nav = navRef.current!;
    return e.clientY - nav.getBoundingClientRect().top + nav.scrollTop;
  };

  const startDrag = (e?: React.PointerEvent) => {
    const p = press.current;
    if (!p || p.dragging) return;
    p.dragging = true;
    window.clearTimeout(p.timer);
    if (e) {
      try {
        navRef.current?.setPointerCapture(e.pointerId);
      } catch {
        /* pointer no longer active */
      }
    }
    setLifted(true);
    setHover(p.index);
    targetY.set(topOf(p.index));
  };

  const onPointerDown = (e: React.PointerEvent, index: number) => {
    if (e.button !== 0) return;
    suppressClick.current = false;
    const grab = pointerY(e) - topOf(index);
    press.current = { id: e.pointerId, startY: e.clientY, grab, index, dragging: false, timer: 0 };
    // Long-press lifts the lens even before the pointer moves.
    press.current.timer = window.setTimeout(() => {
      if (press.current && !press.current.dragging) {
        press.current.dragging = true;
        try {
          navRef.current?.setPointerCapture(press.current.id);
        } catch {
          /* pointer already released */
        }
        setLifted(true);
        setHover(index);
      }
    }, HOLD_MS);
  };

  const onPointerMove = (e: React.PointerEvent) => {
    const p = press.current;
    if (!p) return;
    if (!p.dragging) {
      if (Math.abs(e.clientY - p.startY) < DRAG_THRESHOLD) return;
      startDrag(e);
    }
    const first = topOf(0);
    const last = topOf(MODULES.length - 1);
    let next = pointerY(e) - p.grab;
    if (next < first) next = first + rubberband(next - first, itemHeight() * 2);
    if (next > last) next = last + rubberband(next - last, itemHeight() * 2);
    targetY.set(next);
    setHover(indexAt(next + itemHeight() / 2));
  };

  const finish = (commit: boolean) => {
    const p = press.current;
    press.current = null;
    if (!p) return;
    window.clearTimeout(p.timer);
    if (!p.dragging) return; // plain click: handled by onClick
    // Swallow only the click the browser may fire right after this release.
    suppressClick.current = true;
    window.setTimeout(() => {
      suppressClick.current = false;
    }, 0);
    const target = indexAt(targetY.get() + itemHeight() / 2);
    setLifted(false);
    setHover(null);
    if (commit && MODULES[target]) {
      targetY.set(topOf(target));
      if (MODULES[target].id !== route) navigate(MODULES[target].id as ModuleId);
    } else {
      targetY.set(topOf(activeIndex));
    }
  };

  // Small "settle" when a section is opened by a plain click.
  const nudge = () => {
    if (calm) return;
    void animate(lift, [0.97, 1], { duration: 0.28, ease: [0.23, 1, 0.32, 1] });
  };

  return (
    <aside className="sidebar">
      <div className="sidebar-brand" data-tauri-drag-region>
        <img src="/logo-mark.png" alt="" draggable={false} />
        <span data-tauri-drag-region>StepMaster</span>
      </div>
      <nav
        ref={navRef}
        className={`nav ${lifted ? "lifted" : ""}`}
        aria-label="Main"
        onPointerMove={onPointerMove}
        onPointerUp={() => finish(true)}
        onPointerCancel={() => finish(false)}
        onLostPointerCapture={() => press.current?.dragging && finish(true)}
      >
        <motion.div
          className={`nav-glass ${lifted ? "is-lifted" : ""}`}
          aria-hidden
          style={{ y, scaleX, scaleY, height: itemRefs.current[0]?.offsetHeight ?? 44 }}
        />
        {MODULES.map((m, i) => {
          const active = lifted ? hover === i : route === m.id;
          const Icon = m.icon;
          return (
            <button
              key={m.id}
              ref={(el) => {
                itemRefs.current[i] = el;
              }}
              className={`nav-item ${active ? "active" : ""}`}
              aria-current={route === m.id ? "page" : undefined}
              onPointerDown={(e) => onPointerDown(e, i)}
              onClick={() => {
                if (suppressClick.current) {
                  suppressClick.current = false;
                  return;
                }
                if (m.id !== route) nudge();
                navigate(m.id);
              }}
            >
              <Icon size={20} strokeWidth={1.6} />
              <span className="nav-label">{t.nav[m.id]}</span>
            </button>
          );
        })}
      </nav>
      <div className="sidebar-footer">
        <span className={`dot ${app && !app.isAdmin ? "warn" : ""}`} />
        <span className="truncate">{app ? (app.isAdmin ? t.shell.admin : t.shell.notAdmin) : "…"}</span>
      </div>
    </aside>
  );
}
