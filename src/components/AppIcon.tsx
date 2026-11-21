import { useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { initials } from "../lib/format";

// Icons are fetched lazily (only rows on screen), batched into one backend call per frame-ish,
// and cached for the whole session.
const cache = new Map<string, string | null>();
const waiters = new Map<string, ((v: string | null) => void)[]>();
let queue = new Set<string>();
let timer: ReturnType<typeof setTimeout> | null = null;

function flush() {
  timer = null;
  const batch = [...queue];
  queue = new Set();
  for (let i = 0; i < batch.length; i += 40) {
    const part = batch.slice(i, i + 40);
    api
      .icons(part)
      .then((res) => {
        for (const spec of part) {
          const v = res[spec] ?? null;
          // "Not found" isn't kept: the program may still be installing.
          if (v) cache.set(spec, v);
          waiters.get(spec)?.forEach((f) => f(v));
          waiters.delete(spec);
        }
      })
      .catch(() => {
        for (const spec of part) {
          waiters.get(spec)?.forEach((f) => f(null));
          waiters.delete(spec);
        }
      });
  }
}

export function loadIcon(spec: string): Promise<string | null> {
  if (cache.has(spec)) return Promise.resolve(cache.get(spec) ?? null);
  return new Promise((resolve) => {
    const list = waiters.get(spec) ?? [];
    list.push(resolve);
    waiters.set(spec, list);
    queue.add(spec);
    if (!timer) timer = setTimeout(flush, 30);
  });
}

/** Real program icon (from its exe / Store logo); falls back to initials while loading or if none. */
export function AppIcon({ spec, name, size = 36 }: { spec?: string | null; name: string; size?: number }) {
  const [src, setSrc] = useState<string | null>(() => (spec ? cache.get(spec) ?? null : null));
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!spec) {
      setSrc(null);
      return;
    }
    if (cache.has(spec)) {
      setSrc(cache.get(spec) ?? null);
      return;
    }
    let alive = true;
    const el = ref.current;
    const start = () => {
      void loadIcon(spec).then((v) => {
        if (alive) setSrc(v);
      });
    };
    if (!el || typeof IntersectionObserver === "undefined") {
      start();
      return () => {
        alive = false;
      };
    }
    let started = false;
    const go = () => {
      if (started) return;
      started = true;
      io.disconnect();
      clearTimeout(fallback);
      start();
    };
    const io = new IntersectionObserver((entries) => entries.some((e) => e.isIntersecting) && go(), { rootMargin: "200px" });
    io.observe(el);
    // Safety net: if the observer can't report (window not painted yet), check the position directly.
    const fallback = setTimeout(() => {
      const r = el.getBoundingClientRect();
      if (r.bottom > -200 && r.top < window.innerHeight + 200) go();
    }, 600);
    return () => {
      alive = false;
      io.disconnect();
      clearTimeout(fallback);
    };
  }, [spec]);

  const inner = Math.round(size * 0.72);
  return (
    <div ref={ref} className={`avatar ${src ? "has-icon" : ""}`} style={{ width: size, height: size }} aria-hidden>
      {src ? <img src={src} width={inner} height={inner} alt="" draggable={false} /> : initials(name)}
    </div>
  );
}
