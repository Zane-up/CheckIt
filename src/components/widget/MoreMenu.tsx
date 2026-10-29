import { useI18n } from "@/i18n";
/**
 * 更多菜单：打开管理窗口 / 设置 / 置顶切换 / 退出。
 */

import { Layers, Pin, Settings, LogOut } from "lucide-react";
import { useEffect, useRef } from "react";
import { t } from "@/i18n";
import { api } from "@/services/api";

interface MoreMenuProps {
  onClose: () => void;
}

export default function MoreMenu({ onClose }: MoreMenuProps) {
  useI18n();
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  const togglePin = () => {
    void api.toggleAlwaysOnTop().catch((e) => console.error("切换置顶失败", e));
  };

  const items = [
    {
      icon: Layers,
      label: t.widget.openManager,
      action: () => void api.openManager(),
    },
    {
      icon: Pin,
      label: t.widget.alwaysOnTop,
      action: togglePin,
    },
    {
      icon: Settings,
      label: t.widget.settings,
      action: () => void api.openSettings(),
    },
  ];

  return (
    <div
      ref={ref}
      className="anim-pop absolute right-2 top-12 z-40 flex w-44 flex-col gap-0.5 rounded-xl border border-[var(--glass-border)] bg-[var(--surface-overlay)] p-1.5 shadow-xl"
    >
      {items.map(({ icon: Icon, label, action }) => (
        <button
          key={label}
          type="button"
          onClick={() => {
            action();
            onClose();
          }}
          className="flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-[12.5px] text-[var(--text-primary)] transition-colors hover:bg-[var(--surface-hover)]"
        >
          <Icon size={14} className="text-[var(--text-secondary)]" />
          {label}
        </button>
      ))}
      <div className="my-0.5 h-px bg-[var(--glass-border)]" />
      <button
        type="button"
        onClick={() => void api.quitApp()}
        className="flex items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-[12.5px] text-[var(--urgent-text)] transition-colors hover:bg-[rgba(248,113,113,0.12)]"
      >
        <LogOut size={14} />
        {t.widget.quit}
      </button>
    </div>
  );
}
