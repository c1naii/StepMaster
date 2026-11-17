import { notify } from "../../components/ui";
import { countOf, fmt } from "../../i18n";
import type { Dict } from "../../i18n/ru";
import { api } from "../../lib/api";
import { ask } from "../../lib/store";
import type { Lang } from "../../lib/types";

/** Confirm with the exact list of paths, then move them to the Recycle Bin. Returns recycled paths. */
export async function recycle(paths: string[], t: Dict, lang: Lang): Promise<string[]> {
  if (paths.length === 0) return [];
  const ok = await ask({
    title: t.disk.recycleTitle,
    text: fmt(t.disk.recycleText, { n: countOf(paths.length, t.common.items, lang) }),
    details: paths, // the full list: the user must see everything that will be moved
    confirmLabel: t.common.toRecycle,
    cancelLabel: t.common.cancel,
    danger: true,
  });
  if (!ok) return [];
  try {
    const r = await api.recycle(paths);
    if (r.ok > 0) notify.ok(fmt(t.disk.recycled, { n: countOf(r.ok, t.common.items, lang) }));
    if (r.failed.length > 0) notify.err(fmt(t.disk.recycleFailed, { n: r.failed.length }) + "\n" + r.failed.slice(0, 3).join("\n"));
    const failed = new Set(r.failed.map((f) => f.split(" — ")[0]));
    return paths.filter((p) => !failed.has(p));
  } catch (e) {
    notify.err(e);
    return [];
  }
}
