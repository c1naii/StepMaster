import { useCallback, useEffect, useRef, useState } from "react";
import { errorText } from "./api";

export interface AsyncState<T> {
  data: T | null;
  error: string | null;
  loading: boolean;
  reload: () => Promise<void>;
  setData: (d: T | null) => void;
}

/** Loads data once on mount (and on reload). Ignores results after unmount or a newer reload. */
export function useAsync<T>(fn: () => Promise<T>, deps: unknown[] = []): AsyncState<T> {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const seq = useRef(0);
  const fnRef = useRef(fn);
  fnRef.current = fn;

  const reload = useCallback(async () => {
    const my = ++seq.current;
    setLoading(true);
    setError(null);
    try {
      const res = await fnRef.current();
      if (my === seq.current) setData(res);
    } catch (e) {
      if (my === seq.current) setError(errorText(e));
    } finally {
      if (my === seq.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    void reload();
    return () => {
      seq.current++;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);

  return { data, error, loading, reload, setData };
}

/** Polls fn every `ms` while mounted and the window is visible. Keeps the last good value. */
export function usePoll<T>(fn: () => Promise<T>, ms: number): { data: T | null; error: string | null } {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const fnRef = useRef(fn);
  fnRef.current = fn;

  useEffect(() => {
    let alive = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let running = false;
    const tick = async () => {
      if (running) return;
      running = true;
      try {
        if (document.visibilityState === "visible") {
          const res = await fnRef.current();
          if (alive) {
            setData(res);
            setError(null);
          }
        }
      } catch (e) {
        if (alive) setError(errorText(e));
      } finally {
        running = false;
        if (alive) timer = setTimeout(tick, ms);
      }
    };
    void tick();
    return () => {
      alive = false;
      if (timer) clearTimeout(timer);
    };
  }, [ms]);

  return { data, error };
}

/**
 * Keeps the last `size` values for sparklines. A sample is added every time `tick` changes
 * (e.g. each new snapshot object), even if the value itself stayed the same.
 */
export function useHistory(value: number | null | undefined, tick: unknown, size = 30): number[] {
  const [hist, setHist] = useState<number[]>([]);
  const valueRef = useRef(value);
  valueRef.current = value;
  useEffect(() => {
    const v = valueRef.current;
    if (v === null || v === undefined || !Number.isFinite(v)) return;
    setHist((h) => {
      const next = [...h, v];
      return next.length > size ? next.slice(next.length - size) : next;
    });
  }, [tick, size]);
  return hist;
}

/** Runs an async action and tracks its busy state; prevents double-runs. */
export function useAction() {
  const [busy, setBusy] = useState<string | null>(null);
  const busyRef = useRef<string | null>(null);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const run = useCallback(async <R,>(key: string, fn: () => Promise<R>): Promise<R | undefined> => {
    if (busyRef.current) return undefined;
    busyRef.current = key;
    setBusy(key);
    try {
      return await fn();
    } finally {
      busyRef.current = null;
      if (mounted.current) setBusy(null);
    }
  }, []);
  return { busy, run };
}
