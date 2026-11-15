// Tiny in-memory cache for slow backend calls (PowerShell/WMI) shared between pages.
const store = new Map<string, { at: number; value: unknown }>();
const inflight = new Map<string, Promise<unknown>>();

export async function cached<T>(key: string, ttlMs: number, fn: () => Promise<T>): Promise<T> {
  const hit = store.get(key);
  if (hit && Date.now() - hit.at < ttlMs) return hit.value as T;
  const running = inflight.get(key);
  if (running) return running as Promise<T>;
  const p = fn()
    .then((v) => {
      store.set(key, { at: Date.now(), value: v });
      return v;
    })
    .finally(() => inflight.delete(key));
  inflight.set(key, p);
  return p;
}

export function invalidate(...keys: string[]) {
  for (const k of keys) store.delete(k);
}

export function peek<T>(key: string): T | null {
  return (store.get(key)?.value as T) ?? null;
}
