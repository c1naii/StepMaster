import { animate, motion, useMotionValue, useReducedMotion, useSpring, useTransform, useVelocity } from "motion/react";
import { AlertCircle, Check, Loader2, Search } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { toast } from "sonner";
import { errorText } from "../lib/api";
import { clamp } from "../lib/format";
import { useStore } from "../lib/store";
import { useT } from "../i18n";

/** True when the user turned animations down (app setting or OS setting). */
export function useCalm(): boolean {
  const motionSetting = useStore((s) => s.settings.motion);
  const os = useReducedMotion();
  return motionSetting !== "full" || Boolean(os);
}

export const notify = {
  ok: (msg: string) => toast.success(msg),
  info: (msg: string) => toast(msg),
  err: (e: unknown) => toast.error(errorText(e)),
};

/* ---------- Page wrapper ---------- */
export function PageHeader({ title, sub, right }: { title: string; sub?: ReactNode; right?: ReactNode }) {
  return (
    <div className="page-head">
      <div style={{ minWidth: 0 }}>
        <h1 className="page-title">{title}</h1>
        {sub ? <p className="page-sub">{sub}</p> : null}
      </div>
      {right ? <div className="row">{right}</div> : null}
    </div>
  );
}

/* ---------- Controls ---------- */
export function Toggle({
  checked,
  onChange,
  disabled,
  label,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
  label: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      className="toggle"
      disabled={disabled}
      onClick={() => onChange(!checked)}
    />
  );
}

export function Checkbox({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button type="button" role="checkbox" aria-checked={checked} aria-label={label} className="check" onClick={() => onChange(!checked)}>
      {checked ? <Check size={13} strokeWidth={3} /> : null}
    </button>
  );
}

/** Soft resistance past an edge instead of a hard stop (Apple's rubber-band curve). */
export function rubberband(overshoot: number, dimension: number, k = 0.55) {
  return (overshoot * dimension * k) / (dimension + k * Math.abs(overshoot));
}

/**
 * Segmented tabs with a "liquid glass" indicator, same feel as the sidebar.
 * Click: the indicator glides to the tab. Press and drag sideways (or hold, then drag):
 * it lifts into a glass lens that follows the pointer, stretches with speed and morphs to the
 * width of the tab under it. Release: that tab is selected. Used for every tab bar in the app.
 */
export function Tabs<T extends string>({
  value,
  onChange,
  items,
}: {
  value: T;
  onChange: (v: T) => void;
  items: { id: T; label: string }[];
}) {
  const calm = useCalm();
  const listRef = useRef<HTMLDivElement>(null);
  const tabRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const [lifted, setLifted] = useState(false);
  const [hover, setHover] = useState<number | null>(null);
  const suppressClick = useRef(false);
  const press = useRef<{ id: number; startX: number; grab: number; index: number; dragging: boolean; timer: number } | null>(null);

  const spring = calm ? { stiffness: 2000, damping: 200 } : { stiffness: 560, damping: 44, mass: 0.85 };
  const targetX = useMotionValue(0);
  const targetW = useMotionValue(0);
  const x = useSpring(targetX, spring);
  const w = useSpring(targetW, spring);
  const vel = useVelocity(x);
  const stretchX = useTransform(vel, [-2000, 0, 2000], calm ? [1, 1, 1] : [1.1, 1, 1.1]);
  const stretchY = useTransform(vel, [-2000, 0, 2000], calm ? [1, 1, 1] : [0.94, 1, 0.94]);
  const lift = useSpring(1, { stiffness: 600, damping: 32 });
  const scaleX = useTransform([lift, stretchX], ([l, s]: number[]) => l * s);
  const scaleY = useTransform([lift, stretchY], ([l, s]: number[]) => l * s);

  const activeIndex = Math.max(0, items.findIndex((it) => it.id === value));
  const leftOf = (i: number) => tabRefs.current[i]?.offsetLeft ?? 0;
  const widthOf = (i: number) => tabRefs.current[i]?.offsetWidth ?? 0;

  // Place on mount, follow the selected tab, and re-measure when labels change size (language switch).
  const placed = useRef(false);
  useLayoutEffect(() => {
    const place = (animateIt: boolean) => {
      if (press.current?.dragging) return;
      const l = leftOf(activeIndex);
      const wd = widthOf(activeIndex);
      if (!animateIt) {
        targetX.jump(l);
        x.jump(l);
        targetW.jump(wd);
        w.jump(wd);
      } else {
        targetX.set(l);
        targetW.set(wd);
      }
    };
    place(placed.current);
    placed.current = true;
    const el = listRef.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => place(false));
    ro.observe(el);
    return () => ro.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeIndex, items.map((i) => i.label).join("|")]);

  useEffect(() => {
    lift.set(lifted && !calm ? 1.06 : 1);
  }, [lifted, calm, lift]);

  const pointerX = (e: React.PointerEvent) => e.clientX - (listRef.current?.getBoundingClientRect().left ?? 0);

  const indexAt = (center: number) => {
    let best = 0;
    let dist = Infinity;
    tabRefs.current.forEach((el, i) => {
      if (!el) return;
      const d = Math.abs(el.offsetLeft + el.offsetWidth / 2 - center);
      if (d < dist) {
        dist = d;
        best = i;
      }
    });
    return best;
  };

  const lifting = (p: NonNullable<typeof press.current>, pointerId?: number) => {
    p.dragging = true;
    window.clearTimeout(p.timer);
    try {
      if (pointerId !== undefined) listRef.current?.setPointerCapture(pointerId);
    } catch {
      /* pointer no longer active */
    }
    setLifted(true);
    setHover(p.index);
  };

  const onPointerDown = (e: React.PointerEvent, index: number) => {
    if (e.button !== 0) return;
    suppressClick.current = false;
    const p = { id: e.pointerId, startX: e.clientX, grab: pointerX(e) - leftOf(index), index, dragging: false, timer: 0 };
    press.current = p;
    p.timer = window.setTimeout(() => {
      if (press.current === p && !p.dragging) lifting(p, p.id);
    }, 180);
  };

  const onPointerMove = (e: React.PointerEvent) => {
    const p = press.current;
    if (!p) return;
    if (!p.dragging) {
      if (Math.abs(e.clientX - p.startX) < 5) return;
      lifting(p, e.pointerId);
    }
    const n = items.length - 1;
    const minX = leftOf(0);
    const maxX = leftOf(n) + widthOf(n);
    const curW = targetW.get() || widthOf(p.index);
    let next = pointerX(e) - p.grab;
    if (next < minX) next = minX + rubberband(next - minX, curW);
    if (next + curW > maxX) next = maxX - curW + rubberband(next + curW - maxX, curW);
    const over = indexAt(next + curW / 2);
    setHover(over);
    targetX.set(next);
    targetW.set(widthOf(over)); // the lens morphs to the width of the tab under it
  };

  const finish = (commit: boolean) => {
    const p = press.current;
    press.current = null;
    if (!p) return;
    window.clearTimeout(p.timer);
    if (!p.dragging) return; // plain click: handled by onClick
    suppressClick.current = true;
    window.setTimeout(() => {
      suppressClick.current = false;
    }, 0);
    const target = commit ? indexAt(targetX.get() + (targetW.get() || 0) / 2) : activeIndex;
    setLifted(false);
    setHover(null);
    targetX.set(leftOf(target));
    targetW.set(widthOf(target));
    if (commit && items[target] && items[target].id !== value) onChange(items[target].id);
  };

  return (
    <div
      ref={listRef}
      className={`tabs ${lifted ? "lifted" : ""}`}
      role="tablist"
      onPointerMove={onPointerMove}
      onPointerUp={() => finish(true)}
      onPointerCancel={() => finish(false)}
      onLostPointerCapture={() => press.current?.dragging && finish(true)}
    >
      <motion.div className={`tab-glass ${lifted ? "is-lifted" : ""}`} aria-hidden style={{ x, width: w, scaleX, scaleY }} />
      {items.map((it, i) => {
        const shown = lifted ? hover === i : value === it.id;
        return (
          <button
            key={it.id}
            ref={(el) => {
              tabRefs.current[i] = el;
            }}
            role="tab"
            aria-selected={value === it.id}
            className={`tab ${shown ? "active" : ""}`}
            onPointerDown={(e) => onPointerDown(e, i)}
            onClick={() => {
              if (suppressClick.current) {
                suppressClick.current = false;
                return;
              }
              onChange(it.id);
            }}
          >
            <span>{it.label}</span>
          </button>
        );
      })}
    </div>
  );
}

export function SearchInput({
  value,
  onChange,
  placeholder,
  autoFocus,
  width = 280,
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder: string;
  autoFocus?: boolean;
  width?: number;
}) {
  return (
    <div className="search-input" style={{ width }}>
      <Search size={16} />
      <input
        className="input"
        value={value}
        placeholder={placeholder}
        autoFocus={autoFocus}
        spellCheck={false}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape" && value) {
            e.stopPropagation();
            onChange("");
          }
        }}
      />
    </div>
  );
}

/* ---------- Data viz ---------- */
export function Bar({ value, thin, indeterminate }: { value: number; thin?: boolean; indeterminate?: boolean }) {
  const v = clamp(value, 0, 100);
  return (
    <div
      className={`bar ${thin ? "thin" : ""} ${indeterminate ? "indeterminate" : ""}`}
      role="progressbar"
      aria-valuenow={indeterminate ? undefined : Math.round(v)}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <div className="bar-fill" style={indeterminate ? undefined : { transform: `scaleX(${v / 100})` }} />
    </div>
  );
}

export function Ring({
  value,
  size = 220,
  stroke = 10,
  spinning,
  children,
}: {
  value: number;
  size?: number;
  stroke?: number;
  spinning?: boolean;
  children?: ReactNode;
}) {
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;
  const v = clamp(value, 0, 100);
  return (
    <div style={{ position: "relative", width: size, height: size, flexShrink: 0 }}>
      <svg width={size} height={size} className={spinning ? "spin" : undefined} style={spinning ? { animationDuration: "1.6s" } : undefined}>
        <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="var(--track)" strokeWidth={stroke} />
        <circle
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          stroke="var(--accent)"
          strokeWidth={stroke}
          strokeLinecap="round"
          strokeDasharray={c}
          strokeDashoffset={spinning ? c * 0.75 : c * (1 - v / 100)}
          transform={`rotate(-90 ${size / 2} ${size / 2})`}
          style={{ transition: "stroke-dashoffset 700ms cubic-bezier(0.23, 1, 0.32, 1)" }}
        />
      </svg>
      <div style={{ position: "absolute", inset: 0, display: "grid", placeItems: "center", textAlign: "center" }}>{children}</div>
    </div>
  );
}

export function Sparkline({ values, height = 28, max }: { values: number[]; height?: number; max?: number }) {
  const w = 240;
  if (values.length < 2) {
    return (
      <svg viewBox={`0 0 ${w} ${height}`} width="100%" height={height} preserveAspectRatio="none" aria-hidden>
        <line x1="0" y1={height - 2} x2={w} y2={height - 2} stroke="var(--track)" strokeWidth="1.5" />
      </svg>
    );
  }
  const top = max ?? Math.max(...values, 1);
  const step = w / (values.length - 1);
  const pts = values.map((v, i) => `${(i * step).toFixed(1)},${(height - 2 - (clamp(v, 0, top) / top) * (height - 6)).toFixed(1)}`);
  return (
    <svg viewBox={`0 0 ${w} ${height}`} width="100%" height={height} preserveAspectRatio="none" aria-hidden>
      <polyline points={pts.join(" ")} fill="none" stroke="#8d8d94" strokeWidth="1.5" strokeLinejoin="round" strokeLinecap="round" vectorEffect="non-scaling-stroke" />
    </svg>
  );
}

/** Counts up to a number once (used for rare hero numbers). */
export function CountUp({ value, format }: { value: number; format: (v: number) => string }) {
  const ref = useRef<HTMLSpanElement>(null);
  const prev = useRef(0);
  const calm = useCalm();
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (calm) {
      el.textContent = format(value);
      prev.current = value;
      return;
    }
    const controls = animate(prev.current, value, {
      duration: 0.7,
      ease: [0.23, 1, 0.32, 1],
      onUpdate: (v) => {
        el.textContent = format(v);
      },
    });
    prev.current = value;
    return () => controls.stop();
  }, [value, format, calm]);
  return <span ref={ref}>{format(value)}</span>;
}

/* ---------- States ---------- */
export function Spinner({ size = 16 }: { size?: number }) {
  return <Loader2 size={size} className="spin" aria-hidden />;
}

export function Empty({ icon, text, action }: { icon?: ReactNode; text: string; action?: ReactNode }) {
  return (
    <div className="empty">
      {icon}
      <div>{text}</div>
      {action}
    </div>
  );
}

export function ErrorState({ error, onRetry }: { error: string; onRetry?: () => void }) {
  const t = useT();
  return (
    <div className="empty">
      <AlertCircle size={28} />
      <div style={{ color: "var(--text)" }}>{t.common.loadFailed}</div>
      <div className="small faint" style={{ maxWidth: 520, overflowWrap: "anywhere", userSelect: "text" }}>
        {error}
      </div>
      {onRetry ? (
        <button className="btn btn-secondary sm" onClick={onRetry}>
          {t.common.retry}
        </button>
      ) : null}
    </div>
  );
}

export function Skeleton({ h = 16, w = "100%", r }: { h?: number; w?: number | string; r?: number }) {
  return <div className="skeleton" style={{ height: h, width: w, borderRadius: r }} />;
}

export function SkeletonRows({ rows = 6 }: { rows?: number }) {
  return (
    <div className="list">
      {Array.from({ length: rows }, (_, i) => (
        <div className="list-row" key={i}>
          <Skeleton h={36} w={36} r={10} />
          <div className="cell-main" style={{ display: "grid", gap: 8 }}>
            <Skeleton h={12} w="40%" />
            <Skeleton h={10} w="25%" />
          </div>
        </div>
      ))}
    </div>
  );
}

/** Stagger container for lists of cards: short, decorative, never blocks input. */
export const stagger = {
  container: { hidden: {}, show: { transition: { staggerChildren: 0.04 } } },
  item: {
    hidden: { opacity: 0, transform: "translateY(6px)" },
    show: { opacity: 1, transform: "translateY(0px)", transition: { duration: 0.26, ease: [0.23, 1, 0.32, 1] as const } },
  },
};
