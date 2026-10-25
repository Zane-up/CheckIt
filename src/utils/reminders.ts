/**
 * 提醒预设：提前量以秒为单位集中定义。
 * 与 Rust 侧 reminders/engine.rs 中的语义一致（0 = 截止时提醒）。
 */

export interface ReminderPreset {
  /** 提前秒数；0 表示截止时刻 */
  offsetSeconds: number;
  label: string;
}

export const REMINDER_PRESETS: readonly ReminderPreset[] = [
  { offsetSeconds: 86400, label: "提前 1 天" },
  { offsetSeconds: 10800, label: "提前 3 小时" },
  { offsetSeconds: 3600, label: "提前 1 小时" },
  { offsetSeconds: 1800, label: "提前 30 分钟" },
  { offsetSeconds: 600, label: "提前 10 分钟" },
  { offsetSeconds: 0, label: "截止时" },
] as const;

export const CUSTOM_REMINDER_MINUTES = [5, 15, 120, 240, 720] as const;

export function offsetLabel(offsetSeconds: number): string {
  const preset = REMINDER_PRESETS.find((p) => p.offsetSeconds === offsetSeconds);
  if (preset) return preset.label;
  if (offsetSeconds % 86400 === 0) return `提前 ${offsetSeconds / 86400} 天`;
  if (offsetSeconds % 3600 === 0) return `提前 ${offsetSeconds / 3600} 小时`;
  if (offsetSeconds % 60 === 0) return `提前 ${offsetSeconds / 60} 分钟`;
  return `提前 ${offsetSeconds} 秒`;
}

/** 供表单使用：规范化用户勾选的提醒集合（去重、升序） */
export function normalizeOffsets(offsets: number[]): number[] {
  return [...new Set(offsets)].filter((o) => Number.isInteger(o) && o >= 0 && o <= 60 * 86400).sort((a, b) => a - b);
}
