import { useI18n } from "@/i18n";
/**
 * 悬浮窗底部：今日完成进度 + 打开管理窗口入口。
 */

import { ExternalLink } from "lucide-react";
import type { TaskStats } from "@/utils/tasks";
import { t } from "@/i18n";
import { api } from "@/services/api";

interface WidgetFooterProps {
  stats: TaskStats;
}

export default function WidgetFooter({ stats }: WidgetFooterProps) {
  useI18n();
  const done = stats.completedToday;
  const total = stats.dueTotalToday + done;
  const pct = total > 0 ? Math.round((done / total) * 100) : 0;

  return (
    <div className="flex h-[40px] shrink-0 items-center gap-3 border-t border-[var(--glass-border)] px-3">
      <div className="flex min-w-0 flex-1 items-center gap-2">
        <span className="shrink-0 text-[11.5px] font-medium text-[var(--text-secondary)]">{t.widget.todayProgress}</span>
        <div className="h-1.5 min-w-0 flex-1 overflow-hidden rounded-full bg-[var(--surface-active)]">
          <div
            className="h-full rounded-full bg-gradient-to-r from-[var(--ok)] to-[#6ee7b7] transition-all duration-500"
            style={{ width: `${pct}%` }}
          />
        </div>
        <span className="shrink-0 text-[12px] font-medium tabular-nums text-[var(--text-primary)]">
          {done}/{total}
        </span>
      </div>
      <button
        type="button"
        onClick={() => void api.openManager()}
        className="flex shrink-0 items-center gap-1 rounded-lg px-2 py-1 text-[12px] font-medium text-[var(--text-secondary)] transition-colors hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
        title={t.widget.openManager}
      >
        {t.widget.allTasks}
        <ExternalLink size={11} />
      </button>
    </div>
  );
}
