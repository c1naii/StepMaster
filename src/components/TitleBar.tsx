import { Minus, Search, Square, X } from "lucide-react";
import { useT } from "../i18n";
import { win } from "../lib/api";
import { useStore } from "../lib/store";

export function TitleBar() {
  const t = useT();
  const setPalette = useStore((s) => s.setPalette);

  return (
    <header className="titlebar">
      <button className="titlebar-search" onClick={() => setPalette(true)}>
        <Search size={18} strokeWidth={1.8} />
        <span>{t.shell.searchHint}</span>
        <span className="kbd">Ctrl K</span>
      </button>
      <div className="titlebar-drag" data-tauri-drag-region />
      <div className="window-controls">
        <button className="window-btn" aria-label={t.shell.minimize} onClick={() => void win.minimize()}>
          <Minus size={16} strokeWidth={1.6} />
        </button>
        <button className="window-btn" aria-label={t.shell.maximize} onClick={() => void win.toggleMaximize()}>
          <Square size={13} strokeWidth={1.6} />
        </button>
        <button className="window-btn close" aria-label={t.shell.closeWindow} onClick={() => void win.close()}>
          <X size={17} strokeWidth={1.6} />
        </button>
      </div>
    </header>
  );
}
