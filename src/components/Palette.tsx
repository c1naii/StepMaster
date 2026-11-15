import { CornerDownLeft, Search } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useT } from "../i18n";
import { api } from "../lib/api";
import { useStore } from "../lib/store";
import { MODULES } from "../modules/registry";
import { notify } from "./ui";

interface Item {
  id: string;
  group: "sections" | "actions";
  label: string;
  keywords: string;
  icon?: ReactNode;
  run: () => void;
}

/** Command palette (Ctrl+K / global hotkey). No open/close animation on purpose: it's used constantly. */
export function Palette() {
  const t = useT();
  const open = useStore((s) => s.paletteOpen);
  const setOpen = useStore((s) => s.setPalette);
  const navigate = useStore((s) => s.navigate);
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);

  const items = useMemo<Item[]>(() => {
    const sections: Item[] = MODULES.map((m) => {
      const Icon = m.icon;
      return {
        id: `nav:${m.id}`,
        group: "sections",
        label: t.nav[m.id],
        keywords: m.id,
        icon: <Icon size={17} strokeWidth={1.7} />,
        run: () => navigate(m.id),
      };
    });
    const p = t.palette;
    const actions: Item[] = [
      { id: "a:diag", group: "actions", label: p.aDiag, keywords: "diagnostics check health virus диагностика проверка здоровье вирусы перегрев ошибки", run: () => navigate("diagnostics") },
      { id: "a:downloads", group: "actions", label: p.aDownloads, keywords: "downloads загрузки скачанные установщики архивы", run: () => navigate("disk", "downloads") },
      { id: "a:clean", group: "actions", label: p.aClean, keywords: "clean cleanup очистка мусор кэш temp", run: () => navigate("cleanup", null, "scan") },
      {
        id: "a:dns",
        group: "actions",
        label: p.aDns,
        keywords: "dns flush сброс",
        run: () => {
          setOpen(false);
          api.dnsFlush().then(() => notify.ok(t.network.flushed), notify.err);
        },
      },
      { id: "a:scan", group: "actions", label: p.aScan, keywords: "defender antivirus scan антивирус проверка", run: () => navigate("protection", null, "scan") },
      { id: "a:restore", group: "actions", label: p.aRestore, keywords: "restore point точка восстановление", run: () => navigate("system", "restore") },
      { id: "a:game", group: "actions", label: p.aGame, keywords: "game boost fps игры ускорение", run: () => navigate("games") },
      { id: "a:speed", group: "actions", label: p.aSpeed, keywords: "speed test internet скорость интернет", run: () => navigate("network", "speed") },
      { id: "a:large", group: "actions", label: p.aLarge, keywords: "large files big большие файлы место", run: () => navigate("disk", "large") },
      { id: "a:startup", group: "actions", label: p.aStartup, keywords: "startup autorun автозагрузка", run: () => navigate("system", "startup") },
      { id: "a:proc", group: "actions", label: p.aProcesses, keywords: "process task процессы", run: () => navigate("system", "processes") },
      { id: "a:svc", group: "actions", label: p.aServices, keywords: "services службы", run: () => navigate("system", "services") },
      { id: "a:wifi", group: "actions", label: p.aWifi, keywords: "wifi password пароль вайфай", run: () => navigate("network", "wifi") },
      {
        id: "a:taskmgr",
        group: "actions",
        label: p.aTaskmgr,
        keywords: "taskmgr task manager диспетчер",
        run: () => {
          setOpen(false);
          api.openTool("taskmgr").catch(notify.err);
        },
      },
      {
        id: "a:updates",
        group: "actions",
        label: p.aUpdates,
        keywords: "windows update обновления",
        run: () => {
          setOpen(false);
          api.openTool("updates").catch(notify.err);
        },
      },
    ];
    return [...sections, ...actions];
  }, [t, navigate, setOpen]);

  const filtered = useMemo(() => {
    const needle = q.trim().toLowerCase();
    if (!needle) return items;
    const words = needle.split(/\s+/);
    return items.filter((it) => {
      const hay = `${it.label} ${it.keywords}`.toLowerCase();
      return words.every((w) => hay.includes(w));
    });
  }, [items, q]);

  useEffect(() => {
    if (open) {
      setQ("");
      setSel(0);
    }
  }, [open]);

  useEffect(() => setSel(0), [q]);

  useEffect(() => {
    const el = listRef.current?.querySelector<HTMLElement>(`[data-idx="${sel}"]`);
    el?.scrollIntoView({ block: "nearest" });
  }, [sel]);

  if (!open) return null;

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      setOpen(false);
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      setSel((s) => (filtered.length ? (s + 1) % filtered.length : 0));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSel((s) => (filtered.length ? (s - 1 + filtered.length) % filtered.length : 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      filtered[sel]?.run();
    }
  };

  let lastGroup: string | null = null;

  return (
    <div
      className="palette-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) setOpen(false);
      }}
    >
      <div className="palette" role="dialog" aria-modal="true" aria-label={t.shell.searchHint} onKeyDown={onKey}>
        <div style={{ position: "relative" }}>
          <Search size={18} style={{ position: "absolute", left: 20, top: 19, color: "var(--text-3)" }} />
          <input
            className="palette-input"
            autoFocus
            spellCheck={false}
            placeholder={t.palette.placeholder}
            value={q}
            onChange={(e) => setQ(e.target.value)}
          />
        </div>
        <div className="palette-list" ref={listRef} role="listbox">
          {filtered.length === 0 ? <div className="empty small">{t.palette.empty}</div> : null}
          {filtered.map((it, i) => {
            const header = it.group !== lastGroup ? (it.group === "sections" ? t.palette.sections : t.palette.actions) : null;
            lastGroup = it.group;
            return (
              <div key={it.id}>
                {header ? <div className="palette-group">{header}</div> : null}
                <button
                  data-idx={i}
                  role="option"
                  aria-selected={i === sel}
                  className={`palette-item ${i === sel ? "selected" : ""}`}
                  onMouseMove={() => setSel(i)}
                  onClick={() => it.run()}
                >
                  {it.icon ?? <CornerDownLeft size={15} strokeWidth={1.7} />}
                  <span className="truncate">{it.label}</span>
                </button>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
