import {
  Check,
  ChevronLeft,
  ChevronRight,
  ImagePlus,
  Pencil,
  Plus,
  RotateCcw,
  Shuffle,
  Trash2,
  X,
} from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { modalMotion, useModalKeys } from "../../components/Dialogs";
import {
  ErrorState,
  PageHeader,
  Skeleton,
  Spinner,
  Tabs,
  Toggle,
  notify,
  useCalm,
} from "../../components/ui";
import { fmt, useT } from "../../i18n";
import { api, isTauri, on, pickImages } from "../../lib/api";
import { useAsync } from "../../lib/hooks";
import type { WallItem, WallOptions, WallState } from "../../lib/types";

const EXT = ["jpg", "jpeg", "jfif", "png", "bmp", "webp", "gif"];
const extOk = (p: string) =>
  EXT.includes(p.split(".").pop()?.toLowerCase() ?? "");

export function WallpapersPage() {
  const t = useT();
  const w = t.wallpapers;
  const st = useAsync(api.wallpaperState);
  const data = st.data;
  const [busy, setBusy] = useState<string | null>(null);
  const [view, setView] = useState<{ list: WallItem[]; index: number } | null>(
    null,
  );
  const [dragOver, setDragOver] = useState(false);
  const optSeq = useRef(0);
  const reloadRef = useRef(st.reload);
  reloadRef.current = st.reload;

  // The tray rotation changed the wallpaper: refresh the "current" mark.
  useEffect(() => {
    let off: (() => void) | undefined;
    let dead = false;
    void on("wallpaper://changed", () => void reloadRef.current()).then((f) =>
      dead ? f() : (off = f),
    );
    return () => {
      dead = true;
      off?.();
    };
  }, []);

  const run = async (
    key: string,
    fn: () => Promise<WallState>,
    ok?: string,
  ) => {
    if (busy) return;
    setBusy(key);
    try {
      st.setData(await fn());
      if (ok) notify.ok(ok);
    } catch (e) {
      notify.err(e);
      void st.reload();
    } finally {
      setBusy(null);
    }
  };

  const setWall = (it: WallItem) =>
    run(it.id, () => api.wallpaperSet(it.id), w.setDone);

  const setOpt = (patch: Partial<WallOptions>) => {
    if (!data) return;
    const options = { ...data.options, ...patch };
    st.setData({ ...data, options });
    const seq = ++optSeq.current;
    api
      .wallpaperOptions(options)
      .then((s) => seq === optSeq.current && st.setData(s))
      .catch((e) => {
        notify.err(e);
        void st.reload();
      });
  };

  const addPaths = async (paths: string[]) => {
    const list = paths.filter(extOk);
    if (!list.length) {
      notify.err(w.notImage);
      return;
    }
    if (busy) return;
    setBusy("add");
    try {
      const r = await api.wallpaperAdd(list);
      st.setData(r.state);
      if (r.added) notify.ok(fmt(w.added, { n: r.added }));
      if (r.failed.length)
        notify.err(fmt(w.addFailed, { list: r.failed.slice(0, 3).join(", ") }));
    } catch (e) {
      notify.err(e);
    } finally {
      setBusy(null);
    }
  };
  const addRef = useRef(addPaths);
  addRef.current = addPaths;

  const pick = async () => {
    try {
      const paths = await pickImages(EXT);
      if (paths.length) void addPaths(paths);
    } catch (e) {
      notify.err(e);
    }
  };

  // Drop pictures from Explorer onto the window.
  useEffect(() => {
    if (!isTauri) return;
    let off: (() => void) | undefined;
    let dead = false;
    void import("@tauri-apps/api/webview")
      .then(({ getCurrentWebview }) =>
        getCurrentWebview().onDragDropEvent((e) => {
          const p = e.payload;
          if (p.type === "enter" || p.type === "over") setDragOver(true);
          else if (p.type === "leave") setDragOver(false);
          else if (p.type === "drop") {
            setDragOver(false);
            void addRef.current(p.paths);
          }
        }),
      )
      .then((f) => (dead ? f() : (off = f)));
    return () => {
      dead = true;
      off?.();
    };
  }, []);

  // Inline rename in "My wallpapers": Enter / leaving the field saves, Esc cancels.
  const [edit, setEditState] = useState<{ id: string; text: string } | null>(null);
  const editRef = useRef<{ id: string; text: string } | null>(null);
  const setEdit = (v: { id: string; text: string } | null) => {
    editRef.current = v;
    setEditState(v);
  };
  // Runs once per edit: Enter, Esc or leaving the field, whichever comes first.
  const finishEdit = async (save: boolean) => {
    const e = editRef.current;
    if (!e) return;
    setEdit(null);
    const name = e.text.trim();
    const old = data?.mine.find((x) => x.id === e.id)?.name;
    if (!save || !name || name === old) return;
    try {
      st.setData(await api.wallpaperRename(e.id, name));
    } catch (err) {
      notify.err(err);
    }
  };

  const card = (it: WallItem, list: WallItem[], i: number) => {
    const active = data?.current === it.id;
    const setting = busy === it.id;
    return (
      <div key={it.id} className={`wp-card${active ? " active" : ""}`}>
        <button
          className="wp-thumb"
          onClick={() => setView({ list, index: i })}
          aria-label={`${w.view}: ${it.name}`}
        >
          <img
            src={it.thumb}
            alt=""
            loading="lazy"
            draggable={false}
          />
        </button>
        {edit?.id === it.id ? (
          <div className="wp-meta editing">
            <input
              className="wp-name-input"
              autoFocus
              maxLength={60}
              value={edit.text}
              aria-label={w.rename}
              onFocus={(e) => e.currentTarget.select()}
              onChange={(e) => setEdit({ id: it.id, text: e.target.value })}
              onKeyDown={(e) => {
                if (e.key === "Enter") void finishEdit(true);
                else if (e.key === "Escape") {
                  e.stopPropagation();
                  void finishEdit(false);
                }
              }}
              onBlur={() => void finishEdit(true)}
            />
          </div>
        ) : (
          <div className="wp-meta">
            <span className="wp-name">{it.name}</span>
            {active ? <span className="wp-badge">{w.active}</span> : null}
          </div>
        )}
        {!it.builtin && edit?.id !== it.id ? (
          <button
            className="wp-edit"
            title={w.rename}
            aria-label={w.rename}
            disabled={!!busy}
            onClick={() => setEdit({ id: it.id, text: it.name })}
          >
            <Pencil size={13} />
          </button>
        ) : null}
        {!it.builtin ? (
          <button
            className="wp-del"
            title={w.remove}
            aria-label={w.remove}
            disabled={!!busy}
            onClick={() =>
              run("del:" + it.id, () => api.wallpaperRemove(it.id), w.removed)
            }
          >
            <Trash2 size={14} />
          </button>
        ) : null}
        <button
          className={`wp-plus${active ? " on" : ""}`}
          title={active ? w.active : w.setNow}
          aria-label={active ? w.active : w.setNow}
          disabled={!!busy && !setting}
          onClick={() => !active && setWall(it)}
        >
          {setting ? (
            <Spinner size={16} />
          ) : active ? (
            <Check size={18} strokeWidth={2.5} />
          ) : (
            <Plus size={18} strokeWidth={2.5} />
          )}
        </button>
      </div>
    );
  };

  const o = data?.options;
  const mine = data?.mine ?? [];
  const builtin = data?.builtin ?? [];

  return (
    <>
      <PageHeader
        title={w.title}
        sub={w.sub}
        right={
          <div style={{ display: "flex", gap: 8 }}>
            {data?.canRestore ? (
              <button
                className="btn btn-ghost"
                disabled={!!busy}
                onClick={() => run("restore", api.wallpaperRestore, w.restored)}
              >
                {busy === "restore" ? (
                  <Spinner size={16} />
                ) : (
                  <RotateCcw size={16} />
                )}
                {w.restore}
              </button>
            ) : null}
            <button
              className="btn btn-secondary"
              disabled={!!busy || !data}
              onClick={() => run("next", api.wallpaperNext)}
            >
              {busy === "next" ? <Spinner size={16} /> : <Shuffle size={16} />}
              {w.next}
            </button>
          </div>
        }
      />

      {st.error && !data ? (
        <ErrorState error={st.error} onRetry={() => void st.reload()} />
      ) : (
        <>
          <section className="card wp-settings">
            {o ? (
              <>
                <div className="wp-row">
                  <div className="wp-label">{w.fit}</div>
                  <Tabs
                    value={o.fit === "span" ? "fill" : o.fit}
                    onChange={(fit) => setOpt({ fit })}
                    items={[
                      { id: "fill", label: w.fitFill },
                      { id: "fit", label: w.fitFit },
                      { id: "stretch", label: w.fitStretch },
                      { id: "center", label: w.fitCenter },
                    ]}
                  />
                </div>
                <div className="wp-row">
                  <div className="cell-main">
                    <div className="wp-label">{w.auto}</div>
                    <div className="wp-hint">{w.autoHint}</div>
                  </div>
                  <Toggle
                    checked={o.auto}
                    onChange={(auto) => setOpt({ auto })}
                    label={w.auto}
                  />
                </div>
                <AnimatePresence initial={false}>
                  {o.auto ? (
                    <motion.div
                      className="wp-auto"
                      initial={{ height: 0, opacity: 0 }}
                      animate={{ height: "auto", opacity: 1 }}
                      exit={{ height: 0, opacity: 0 }}
                      transition={{ duration: 0.22, ease: [0.23, 1, 0.32, 1] }}
                    >
                      <div className="wp-auto-inner">
                        <div className="wp-row">
                          <div className="wp-label">{w.every}</div>
                          <Tabs
                            value={String(o.interval)}
                            onChange={(v) => setOpt({ interval: Number(v) })}
                            items={[
                              { id: "15", label: w.i15 },
                              { id: "60", label: w.i60 },
                              { id: "360", label: w.i360 },
                              { id: "1440", label: w.i1440 },
                              { id: "0", label: w.iLogin },
                            ]}
                          />
                        </div>
                        <div className="wp-row">
                          <div className="wp-label">{w.from}</div>
                          <Tabs
                            value={o.source}
                            onChange={(source) => setOpt({ source })}
                            items={[
                              { id: "all", label: w.srcAll },
                              { id: "mine", label: w.srcMine },
                              { id: "builtin", label: w.srcBuiltin },
                            ]}
                          />
                        </div>
                        <div className="wp-row">
                          <div className="wp-label">{w.shuffle}</div>
                          <Toggle
                            checked={o.shuffle}
                            onChange={(shuffle) => setOpt({ shuffle })}
                            label={w.shuffle}
                          />
                        </div>
                      </div>
                    </motion.div>
                  ) : null}
                </AnimatePresence>
              </>
            ) : (
              <>
                <Skeleton h={34} />
                <Skeleton h={34} />
              </>
            )}
          </section>

          <div className="wp-head">
            <h2 className="section-title">
              {w.mine}
              {mine.length ? (
                <span className="wp-count">{mine.length}</span>
              ) : null}
            </h2>
            {mine.length ? (
              <button
                className="btn btn-secondary sm"
                onClick={pick}
                disabled={!!busy}
              >
                {busy === "add" ? <Spinner size={14} /> : <Plus size={15} />}
                {w.add}
              </button>
            ) : null}
          </div>
          {!data ? (
            <Skeleton h={150} r={16} />
          ) : mine.length === 0 ? (
            <button className="wp-empty" onClick={pick} disabled={!!busy}>
              {busy === "add" ? (
                <Spinner size={26} />
              ) : (
                <ImagePlus size={28} strokeWidth={1.5} />
              )}
              <span>{w.emptyMine}</span>
              <small>{w.formats}</small>
            </button>
          ) : (
            <div className="wp-grid">
              {mine.map((it, i) => card(it, mine, i))}
            </div>
          )}

          <div className="wp-head">
            <h2 className="section-title">{w.builtin}</h2>
          </div>
          <div className="wp-grid">
            {data
              ? builtin.map((it, i) => card(it, builtin, i))
              : Array.from({ length: 6 }, (_, i) => (
                  <Skeleton key={i} h={150} r={16} />
                ))}
          </div>
        </>
      )}

      {createPortal(
        <>
          <AnimatePresence>
            {view && data ? (
              <Viewer
                key="viewer"
                list={view.list}
                index={view.index}
                current={data.current}
                busy={busy}
                onIndex={(index) => setView({ ...view, index })}
                onClose={() => setView(null)}
                onSet={(it) => void setWall(it)}
              />
            ) : null}
          </AnimatePresence>

          <AnimatePresence>
            {dragOver ? (
              <motion.div
                className="wp-drop"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
                transition={{ duration: 0.15 }}
              >
                <div className="wp-drop-box">
                  <ImagePlus size={34} strokeWidth={1.5} />
                  <span>{w.drop}</span>
                </div>
              </motion.div>
            ) : null}
          </AnimatePresence>
        </>,
        document.body,
      )}
    </>
  );
}

function Viewer({
  list,
  index,
  current,
  busy,
  onIndex,
  onClose,
  onSet,
}: {
  list: WallItem[];
  index: number;
  current: string | null;
  busy: string | null;
  onIndex: (i: number) => void;
  onClose: () => void;
  onSet: (it: WallItem) => void;
}) {
  const t = useT();
  const w = t.wallpapers;
  const calm = useCalm();
  const boxRef = useRef<HTMLDivElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const [loaded, setLoaded] = useState<string | null>(null);
  useModalKeys(true, boxRef, closeRef, onClose);

  const it = list[Math.min(index, list.length - 1)];
  const n = list.length;
  const go = (d: number) => onIndex((index + d + n) % n);

  // Full picture comes from the backend (the page cannot read local files); cached per id.
  const [full, setFull] = useState<Record<string, string>>({});
  const id = it?.id;
  useEffect(() => {
    if (!id || full[id]) return;
    let dead = false;
    api
      .wallpaperView(id)
      .then((src) => !dead && src && setFull((f) => ({ ...f, [id]: src })))
      .catch(() => undefined);
    return () => {
      dead = true;
    };
  }, [id, full]);

  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      if (n < 2) return;
      if (e.key === "ArrowLeft") onIndex((index - 1 + n) % n);
      else if (e.key === "ArrowRight") onIndex((index + 1) % n);
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [index, n, onIndex]);

  if (!it) return null;
  const active = current === it.id;
  const ar = it.width > 0 && it.height > 0 ? it.width / it.height : 16 / 9;
  const ready = loaded === it.id;

  return (
    <motion.div
      className="modal-backdrop"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.2 }}
      onMouseDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <motion.div
        ref={boxRef}
        className="wp-viewer"
        role="dialog"
        aria-modal="true"
        aria-label={it.name}
        {...modalMotion(calm)}
      >
        <div
          className="wp-stage"
          style={{
            aspectRatio: String(ar),
            width: `min(1100px, 86vw, calc(72vh * ${ar.toFixed(4)}))`,
          }}
        >
          <img
            className="wp-stage-img wp-stage-thumb"
            src={it.thumb}
            alt=""
            draggable={false}
          />
          {full[it.id] ? (
            <img
              key={it.id}
              className="wp-stage-img"
              style={{ opacity: ready ? 1 : 0 }}
              src={full[it.id]}
              alt={it.name}
              draggable={false}
              onLoad={() => setLoaded(it.id)}
            />
          ) : null}
          {n > 1 ? (
            <>
              <button
                className="wp-nav left"
                onClick={() => go(-1)}
                aria-label={w.prev}
              >
                <ChevronLeft size={22} />
              </button>
              <button
                className="wp-nav right"
                onClick={() => go(1)}
                aria-label={w.nextOne}
              >
                <ChevronRight size={22} />
              </button>
            </>
          ) : null}
          <button
            ref={closeRef}
            className="wp-close"
            onClick={onClose}
            aria-label={t.common.close}
          >
            <X size={18} />
          </button>
        </div>
        <div className="wp-viewer-bar">
          <div className="cell-main">
            <div className="wp-viewer-name">{it.name}</div>
            {it.width > 0 ? (
              <div className="wp-hint">
                {it.width} × {it.height}
              </div>
            ) : null}
          </div>
          <button
            className="btn btn-primary"
            disabled={active || !!busy}
            onClick={() => onSet(it)}
          >
            {busy === it.id ? (
              <Spinner size={16} />
            ) : active ? (
              <Check size={16} />
            ) : (
              <Plus size={16} />
            )}
            {active ? w.active : w.setNow}
          </button>
        </div>
      </motion.div>
    </motion.div>
  );
}
