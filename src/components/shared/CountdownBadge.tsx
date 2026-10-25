/**
 * 倒计时徽章：根据剩余时间展示不同文本与紧急颜色。
 * 由父组件传入 now（全局共享时钟），本组件自身不建定时器。
 */

import { computeCountdown } from "@/utils/countdown";
import type { Urgency } from "@/types";

interface CountdownBadgeProps {
  dueAt: string | null;
  now: number;
  completed: boolean;
}

const URGENCY_COLOR: Record<Urgency, string> = {
  none: "var(--text-muted)",
  normal: "var(--text-secondary)",
  warning: "var(--warning)",
  urgent: "var(--urgent)",
  overdue: "var(--urgent)",
  done: "var(--text-muted)",
};

export default function CountdownBadge({ dueAt, now, completed }: CountdownBadgeProps) {
  if (!dueAt) {
    return <span className="text-[10px] text-[var(--text-muted)]">无截止</span>;
  }
  const cd = computeCountdown(dueAt, now);
  if (!cd) return null;

  if (completed) {
    return <span className="text-[10px] text-[var(--text-muted)]">已完成</span>;
  }

  const color = URGENCY_COLOR[cd.urgency];
  const isHot = cd.urgency === "urgent" || cd.urgency === "overdue";

  return (
    <span
      className={`text-[10px] tabular-nums ${isHot ? "font-medium" : ""}`}
      style={{ color }}
    >
      {cd.text}
    </span>
  );
}
