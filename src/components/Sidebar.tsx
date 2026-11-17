import { AnimatePresence, animate, motion, useMotionValue, useSpring, useTransform, useVelocity } from "motion/react";
import { PanelLeftClose, PanelLeftOpen } from "lucide-react";
import { Fragment, useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useT } from "../i18n";
import { useStore, type ModuleId } from "../lib/store";
import { MODULES } from "../modules/registry";
import { rubberband, useCalm } from "./ui";

/** Movement (px) before a press turns into a drag; a plain click stays a click. */
const DRAG_THRESHOLD = 5;
/** Holding still this long also "lifts" the lens, like a long-press. */
const HOLD_MS = 180;
/** Narrower windows than this get the icon-only menu by themselves. */
const NARROW_PX = 1200;
const PREF_KEY = "sm-sidebar-collapsed";

function readPref(): boolean {
  try {
    return localStorage.getItem(PREF_KEY) === "1";
  } catch {
    return false;
  }
}

/**
 * Icon-only menu: the user's own choice (remembered), or automatically on a narrow window.
 * On a narrow window "Expand" opens it just for now; widening the window resets that.
 */
export function useSidebarCollapsed() {
  const [pinned, setPinned] = useState(readPref);
  const [narrow, setNarrow] = useState(() => window.innerWidth < NARROW_PX);
  const [peek, setPeek] = useState(false);
  useEffect(() => {
    const mq = window.matchMedia(`(max-width: ${NARROW_PX - 1}px)`);
    const on = () => {
      setNarrow(mq.matches);
      setPeek(false);
    };
    mq.addEventListener("change", on);
    return () => mq.removeEventListener("change", on);
  }, []);
  const collapsed = pinned || (narrow && !peek);
  const save = (v: boolean) => {
    setPinned(v);
    try {
      localStorage.setItem(PREF_KEY, v ? "1" : "0");
    } catch {
      /* storage unavailable: the choice lasts for this session */
    }
  };
  const toggle = () => {
    if (collapsed) {
      if (pinned) save(false);
      if (narrow) setPeek(true);
    } else if (narrow && peek) {
      setPeek(false);
    } else {
      save(true);
    }
  };
  return { collapsed, toggle };
}


/**
 * Sidebar with a single "liquid glass" indicator.
 * Click: the indicator glides to the item. Press and drag (or hold, then drag): the indicator
 * lifts into a glass lens that follows the pointer with spring inertia and stretches with speed.
 * Release: the section under the lens opens.
 */
export function Sidebar({ collapsed, onToggle }: { collapsed: boolean; onToggle: () => void }) {
  const t = useT();
  const route = useStore((s) => s.route);
  const navigate = useStore((s) => s.navigate);
  const calm = useCalm();

  const navRef = useRef<HTMLElement>(null);
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const [lifted, setLifted] = useState(false);
  const [hover, setHover] = useState<number | null>(null);
  const suppressClick = useRef(false);
  /** Label shown next to an icon while the menu is collapsed. */
  const [tip, setTip] = useState<{ text: string; top: number } | null>(null);
  useEffect(() => {
    if (!collapsed) setTip(null);
  }, [collapsed]);
  // Ctrl+B: collapse / expand, like in code editors.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && !e.shiftKey && !e.altKey && e.code === "KeyB") {
        e.preventDefault();
        onToggle();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onToggle]);
  const showTip = (el: HTMLElement, text: string) => {
    if (!collapsed || press.current?.dragging) return;
    const r = el.getBoundingClientRect();
    setTip({ text, top: r.top + r.height / 2 });
  };
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

  // The window was resized (dragging its edge, maximize): items at the foot of the menu move,
  // so the indicator snaps to where the active item is now instead of staying behind.
  const activeRef = useRef(activeIndex);
  activeRef.current = activeIndex;
  useEffect(() => {
    const nav = navRef.current;
    if (!nav || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => {
      if (press.current?.dragging) return;
      const top = topOf(activeRef.current);
      targetY.jump(top);
      y.jump(top);
    });
    ro.observe(nav);
    return () => ro.disconnect();
  }, [topOf, targetY, y]);

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
    setTip(null);
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
        targetY.set(topOf(index)); // the lens jumps to the held item, so releasing selects it
      }
    }, HOLD_MS);
    // Released anywhere (even outside the menu or the window): the press must always end,
    // otherwise the lens stays where the pointer left it.
    const p = press.current;
    const end = (ev: Event) => {
      window.removeEventListener("pointerup", end, true);
      window.removeEventListener("pointercancel", end, true);
      window.removeEventListener("blur", end);
      if (press.current !== p) return;
      if (!p.dragging) {
        window.clearTimeout(p.timer);
        press.current = null;
      } else {
        finish(ev.type === "pointerup");
      }
    };
    window.addEventListener("pointerup", end, true);
    window.addEventListener("pointercancel", end, true);
    window.addEventListener("blur", end);
  };

  const onPointerMove = (e: React.PointerEvent) => {
    const p = press.current;
    if (!p) return;
    // The button is no longer held (the release got lost): finish instead of following the pointer.
    if (p.dragging && e.buttons === 0) {
      finish(true);
      return;
    }
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
    <aside className={`sidebar ${collapsed ? "collapsed" : ""}`}>
      <div className="sidebar-brand" data-tauri-drag-region>
        <img src="/logo-mark.png" alt="" draggable={false} />
        <span className="sidebar-brand-text" data-tauri-drag-region>
          StepMaster
        </span>
      </div>
      <nav
        ref={navRef}
        className={`nav ${lifted ? "lifted" : ""}`}
        aria-label="StepMaster"
        onPointerMove={onPointerMove}
        onPointerUp={() => finish(true)}
        onPointerCancel={() => finish(false)}
        onLostPointerCapture={() => press.current?.dragging && finish(true)}
        onScroll={() => setTip(null)}
      >
        <motion.div
          className={`nav-glass ${lifted ? "is-lifted" : ""}`}
          aria-hidden
          style={{ y, scaleX, scaleY, height: itemRefs.current[0]?.offsetHeight ?? 40 }}
        />
        {MODULES.map((m, i) => {
          const active = lifted ? hover === i : route === m.id;
          const Icon = m.icon;
          const label = t.nav[m.id];
          const prev = MODULES[i - 1];
          // A thin line between groups; the "bottom" group is pushed to the foot of the menu.
          const before =
            prev && prev.group !== m.group ? (
              m.group === "bottom" ? <div className="nav-spacer" aria-hidden /> : <div className="nav-sep" aria-hidden />
            ) : null;
          return (
            <Fragment key={m.id}>
              {before}
              <button
                ref={(el) => {
                  itemRefs.current[i] = el;
                }}
                className={`nav-item ${active ? "active" : ""}`}
                aria-current={route === m.id ? "page" : undefined}
                aria-label={collapsed ? label : undefined}
                onPointerDown={(e) => onPointerDown(e, i)}
                onPointerEnter={(e) => showTip(e.currentTarget, label)}
                onPointerLeave={() => setTip(null)}
                onFocus={(e) => showTip(e.currentTarget, label)}
                onBlur={() => setTip(null)}
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
                <span className="nav-label">{label}</span>
              </button>
            </Fragment>
          );
        })}
      </nav>
      <div className="sidebar-footer">
        <button
          className="sidebar-toggle"
          aria-label={collapsed ? t.shell.expandMenu : t.shell.collapseMenu}
          title={collapsed ? undefined : `${t.shell.collapseMenu} · Ctrl+B`}
          onClick={() => {
            setTip(null);
            onToggle();
          }}
          onPointerEnter={(e) => showTip(e.currentTarget, `${t.shell.expandMenu} · Ctrl+B`)}
          onPointerLeave={() => setTip(null)}
        >
          {collapsed ? <PanelLeftOpen size={18} strokeWidth={1.6} /> : <PanelLeftClose size={18} strokeWidth={1.6} />}
        </button>
      </div>
      <AnimatePresence>
        {tip ? (
          <motion.div
            key={tip.text}
            className="nav-tip"
            role="tooltip"
            style={{ top: tip.top }}
            initial={calm ? { opacity: 0 } : { opacity: 0, x: -4 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.12 }}
          >
            {tip.text}
          </motion.div>
        ) : null}
      </AnimatePresence>
    </aside>
  );
}
