import { useI18n } from "@/i18n";
/**
 * 状态概览：待办 / 今日到期 / 已逾期 紧凑一行。
 * 标签与数字分层：数字更大更重，标签 12px 中等字重且使用稳定的次文字色，
 * 数值为 0 时也保持可读。
 */

import type { TaskStats } from "@/utils/tasks";
import { t } from "@/i18n";

interface StatsOverviewProps {
  stats: TaskStats;
}

function StatItem({ value, label, color }: { value: number; label: string; color?: string }) {
  useI18n();
  return (
    <div className="glass-subtle flex min-h-[52px] min-w-0 flex-1 flex-col items-center justify-center rounded-xl px-1.5 py-2">
      <span
        className="text-[20px] font-semibold leading-none tabular-nums"
        style={color ? { color } : undefined}
      >
        {value}
      </span>
      <span className="mt-1.5 whitespace-nowrap text-[12px] font-medium leading-none text-[var(--text-secondary)]">
        {label}
      </span>
    </div>
  );
}

export default function StatsOverview({ stats }: StatsOverviewProps) {
  useI18n();
  return (
    <div className="flex shrink-0 gap-2 px-3 pb-2">
      <StatItem value={stats.pending} label={t.widget.stats.pending} />
      <StatItem
        value={stats.dueToday}
        label={t.widget.stats.dueToday}
        color={stats.dueToday > 0 ? "var(--warning-text)" : undefined}
      />
      <StatItem
        value={stats.overdue}
        label={t.widget.stats.overdue}
        color={stats.overdue > 0 ? "var(--urgent-text)" : undefined}
      />
    </div>
  );
}
