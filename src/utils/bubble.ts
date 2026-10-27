/**
 * 贴边圆圈计数格式化。
 *
 * 规则（与 Rust `dock::format_bubble_count` 保持一致）：
 * - 0 显示 "0"
 * - 1..=99 显示原值
 * - 大于 99 显示 "99+"
 */

export const BUBBLE_COUNT_CAP = 99;

export function formatBubbleCount(count: number): string {
  if (!Number.isFinite(count) || count <= 0) return "0";
  const n = Math.floor(count);
  return n > BUBBLE_COUNT_CAP ? `${BUBBLE_COUNT_CAP}+` : String(n);
}

/** 圆圈 tooltip：显示完整数量（不做 99+ 截断） */
export function bubbleTooltip(count: number): string {
  const n = Number.isFinite(count) && count > 0 ? Math.floor(count) : 0;
  return n === 0 ? "暂无待办任务" : `${n} 个待办任务`;
}
