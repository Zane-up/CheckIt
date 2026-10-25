/**
 * 状态概览：待办 / 今日到期 / 即将到期 紧凑一行。
 */

import type { TaskStats } from "@/utils/tasks";
import { t } from "@/i18n/zh";

interface StatsOverviewProps {
  stats: TaskStats;
}

function StatItem({ value, label, color }: { value: number; label: string; color?: string }) {
  return (
    <div className="glass-subtle flex flex-1 flex-col items-center rounded-xl px-1 py-1.5">
      <span
        className="text-[15px] font-semibold leading-none"
        style={color ? { color } : undefined}
      >
        {value}
      </span>
      <span className="mt-0.5 text-[10px] text-[var(--text-muted)]">{label}</span>
    </div>
  );
}

export default function StatsOverview({ stats }: StatsOverviewProps) {
  return (
    <div className="flex shrink-0 gap-1.5 px-3 pb-1.5">
      <StatItem value={stats.pending} label={t.widget.stats.pending} />
      <StatItem value={stats.dueToday} label={t.widget.stats.dueToday} color={stats.dueToday > 0 ? "var(--warning)" : undefined} />
      <StatItem value={stats.overdue} label={t.widget.stats.overdue} color={stats.overdue > 0 ? "var(--urgent)" : undefined} />
    </div>
  );
}
