/**
 * 悬浮窗标题栏：Logo + 日期 + 快捷操作。
 * 整个标题栏是拖动区域（data-tauri-drag-region）。
 */

import { CheckSquare, ChevronDown, ChevronUp, MoreHorizontal, PanelTop, Plus } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useState } from "react";
import { IconButton } from "@/components/ui/Buttons";
import { t } from "@/i18n/zh";
import MoreMenu from "@/components/widget/MoreMenu";

interface WidgetHeaderProps {
  dateText: string;
  onAdd: () => void;
  onToggleCollapse: () => void;
  collapsed: boolean;
}

export default function WidgetHeader({ dateText, onAdd, onToggleCollapse, collapsed }: WidgetHeaderProps) {
  const [menuOpen, setMenuOpen] = useState(false);

  return (
    <div
      data-tauri-drag-region
      className="relative flex h-[52px] shrink-0 items-center gap-2 px-3"
    >
      <div data-tauri-drag-region className="flex min-w-0 flex-1 items-center gap-2">
        <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-gradient-to-br from-[#6366f1] to-[#8b5cf6] shadow-sm">
          <CheckSquare size={15} className="text-white" strokeWidth={2.4} />
        </div>
        <div data-tauri-drag-region className="min-w-0 leading-tight">
          <div className="truncate text-[13px] font-semibold tracking-wide">CheckIt</div>
          <div className="text-[10px] text-[var(--text-muted)]">{dateText}</div>
        </div>
      </div>

      <div className="flex items-center gap-0.5">
        <IconButton
          title={t.widget.alwaysOnTop}
          onClick={async () => {
            const win = getCurrentWindow();
            await win.setAlwaysOnTop(!(await win.isAlwaysOnTop()));
          }}
        >
          <PanelTop size={15} />
        </IconButton>
        <IconButton title={collapsed ? t.widget.expand : t.widget.collapse} onClick={onToggleCollapse}>
          {collapsed ? <ChevronUp size={15} /> : <ChevronDown size={15} />}
        </IconButton>
        <IconButton title={t.widget.settings} onClick={() => setMenuOpen((v) => !v)}>
          <MoreHorizontal size={15} />
        </IconButton>
        <button
          type="button"
          title={t.widget.addTask}
          onClick={onAdd}
          className="ml-0.5 flex h-7 w-7 items-center justify-center rounded-lg bg-[var(--accent)] text-white shadow-sm transition-all duration-150 hover:bg-[var(--accent-hover)] active:scale-90"
        >
          <Plus size={16} strokeWidth={2.5} />
        </button>
      </div>

      {menuOpen && <MoreMenu onClose={() => setMenuOpen(false)} />}
    </div>
  );
}
