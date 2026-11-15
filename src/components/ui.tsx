import { animate, motion, useReducedMotion } from "motion/react";
import { AlertCircle, Check, Loader2, Search } from "lucide-react";
import { useEffect, useId, useRef, type ReactNode } from "react";
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

export function Tabs<T extends string>({
  value,
  onChange,
  items,
}: {
  value: T;
  onChange: (v: T) => void;
  items: { id: T; label: string }[];
}) {
  const id = useId();
  const calm = useCalm();
  return (
    <div className="tabs" role="tablist">
      {items.map((it) => (
        <button
          key={it.id}
          role="tab"
          aria-selected={value === it.id}
          className={`tab ${value === it.id ? "active" : ""}`}
          onClick={() => onChange(it.id)}
        >
          {value === it.id ? (
            <motion.div
              layoutId={`tab-pill-${id}`}
              className="tab-pill"
              transition={calm ? { duration: 0 } : { type: "spring", duration: 0.3, bounce: 0.12 }}
            />
          ) : null}
          <span>{it.label}</span>
        </button>
      ))}
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
