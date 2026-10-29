import { tr } from "@/i18n";
/**
 * 倒计时核心模块（纯函数，可测试）。
 *
 * 关键规则：
 * 1. 每次都使用 dueAt - now 的真实差值计算，绝不递减变量。
 * 2. 阈值集中配置，避免业务逻辑分散。
 * 3. 展示粒度：
 *    - > 24 小时：X天 X小时
 *    - < 24 小时：X小时 X分钟
 *    - < 1 小时：X分钟 X秒
 *    - 已过期：逾期 X（使用相同粒度规则）
 */

import type { Task, Urgency } from "@/types";

/** 紧急程度阈值配置（毫秒）。集中管理，避免散落各组件。 */
export const URGENCY_THRESHOLDS = {
  /** 低于该剩余时间显示"紧急"（红色）：1 小时 */
  urgentMs: 60 * 60 * 1000,
  /** 低于该剩余时间显示"即将到期"（琥珀色）：24 小时 */
  warningMs: 24 * 60 * 60 * 1000,
} as const;

export interface CountdownResult {
  /** 剩余毫秒数；负数表示已逾期 */
  totalMs: number;
  overdue: boolean;
  /** 展示文本，如 "3天 4小时"、"逾期 2小时 5分钟" */
  text: string;
  urgency: Urgency;
}

/** 按毫秒差值生成人类可读的剩余时间文本（不含"逾期"前缀） */
export function formatRemaining(totalMs: number): string {
  const abs = Math.abs(totalMs);
  const totalSeconds = Math.floor(abs / 1000);

  if (totalSeconds < 60) {
    return tr("{0}秒", totalSeconds);
  }

  const totalMinutes = Math.floor(totalSeconds / 60);
  if (totalMinutes < 60) {
    const secs = totalSeconds % 60;
    return tr("{0}分钟 {1}秒", totalMinutes, secs);
  }

  const totalHours = Math.floor(totalMinutes / 60);
  if (totalHours < 24) {
    const mins = totalMinutes % 60;
    return tr("{0}小时 {1}分钟", totalHours, mins);
  }

  const days = Math.floor(totalHours / 24);
  const hours = totalHours % 24;
  return tr("{0}天 {1}小时", days, hours);
}

/**
 * 计算任务截止倒计时。
 * @param dueAt RFC3339 UTC 截止时间
 * @param now 当前真实时间戳（毫秒）
 */
export function computeCountdown(dueAt: string | null, now: number): CountdownResult | null {
  if (!dueAt) return null;
  const due = new Date(dueAt).getTime();
  if (Number.isNaN(due)) return null;

  const totalMs = due - now;
  const overdue = totalMs < 0;

  let urgency: Urgency;
  if (overdue) {
    urgency = "overdue";
  } else if (totalMs <= URGENCY_THRESHOLDS.urgentMs) {
    urgency = "urgent";
  } else if (totalMs <= URGENCY_THRESHOLDS.warningMs) {
    urgency = "warning";
  } else {
    urgency = "normal";
  }

  return {
    totalMs,
    overdue,
    text: overdue ? tr("逾期 {0}", formatRemaining(totalMs)) : formatRemaining(totalMs),
    urgency,
  };
}

/** 任务的展示紧急程度（考虑完成状态：已完成不再显示紧急倒计时） */
export function taskUrgency(task: Task, now: number): Urgency {
  if (task.status === "completed") return "done";
  const cd = computeCountdown(task.due_at, now);
  if (!cd) return "none";
  return cd.urgency;
}

/** 用于测试的固定时钟辅助：从 UTC 时间的 Y/M/D H:M 构造时间戳 */
export function utcTimestamp(
  year: number,
  month: number,
  day: number,
  hour = 0,
  minute = 0,
  second = 0,
): number {
  return Date.UTC(year, month - 1, day, hour, minute, second);
}
