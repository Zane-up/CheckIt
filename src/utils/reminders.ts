import { tr } from "@/i18n";
/**
 * 提醒预设与自定义提前量：提前量以秒为单位集中定义。
 * 与 Rust 侧 reminders/engine.rs 中的语义一致（0 = 截止时提醒）。
 */

export interface ReminderPreset {
  /** 提前秒数；0 表示截止时刻 */
  offsetSeconds: number;
  label: string;
}

export const REMINDER_PRESETS: readonly ReminderPreset[] = [
  { offsetSeconds: 86400, get label() { return tr("提前 1 天"); } },
  { offsetSeconds: 10800, get label() { return tr("提前 3 小时"); } },
  { offsetSeconds: 3600, get label() { return tr("提前 1 小时"); } },
  { offsetSeconds: 1800, get label() { return tr("提前 30 分钟"); } },
  { offsetSeconds: 600, get label() { return tr("提前 10 分钟"); } },
  { offsetSeconds: 0, get label() { return tr("截止时"); } },
] as const;

/** 自定义提前量的输入单位 */
export type ReminderUnit = "minute" | "hour" | "day";

export const REMINDER_UNIT_SECONDS: Record<ReminderUnit, number> = {
  minute: 60,
  hour: 3600,
  day: 86400,
};

/** 提前量上限：60 天，与 Rust 侧 normalize_reminder_offsets 一致 */
export const MAX_REMINDER_OFFSET_SECONDS = 60 * 86400;

/** 自定义输入校验失败的原因码（文案由界面层映射） */
export type ReminderParseError = "required" | "invalid" | "zero" | "tooLarge";

export type ReminderParseResult =
  | { ok: true; offsetSeconds: number }
  | { ok: false; error: ReminderParseError };

/**
 * 校验自定义提前量输入：必须是正整数字符串；按单位换算为秒后
 * 不得为 0（0 走"截止时"预设）、不得超过 60 天、不得超过安全整数。
 * 校验失败时调用方须就地提示并保留输入，不得静默丢弃。
 */
export function parseReminderAmount(raw: string, unit: ReminderUnit): ReminderParseResult {
  const s = raw.trim();
  if (s === "") return { ok: false, error: "required" };
  // 只认纯数字串：天然拒绝负数、小数点、科学计数法、空白与其他字符
  if (!/^\d+$/.test(s)) return { ok: false, error: "invalid" };
  const amount = Number(s);
  if (!Number.isSafeInteger(amount)) return { ok: false, error: "invalid" };
  if (amount === 0) return { ok: false, error: "zero" };
  const offsetSeconds = amount * REMINDER_UNIT_SECONDS[unit];
  if (!Number.isSafeInteger(offsetSeconds) || offsetSeconds > MAX_REMINDER_OFFSET_SECONDS) {
    return { ok: false, error: "tooLarge" };
  }
  return { ok: true, offsetSeconds };
}

export function offsetLabel(offsetSeconds: number): string {
  const preset = REMINDER_PRESETS.find((p) => p.offsetSeconds === offsetSeconds);
  if (preset) return preset.label;
  if (offsetSeconds % 86400 === 0) return tr("提前 {0} 天", offsetSeconds / 86400);
  if (offsetSeconds % 3600 === 0) return tr("提前 {0} 小时", offsetSeconds / 3600);
  if (offsetSeconds % 60 === 0) return tr("提前 {0} 分钟", offsetSeconds / 60);
  return tr("提前 {0} 秒", offsetSeconds);
}

/** 供表单使用：规范化用户勾选的提醒集合（去重、升序） */
export function normalizeOffsets(offsets: number[]): number[] {
  return [...new Set(offsets)]
    .filter((o) => Number.isInteger(o) && o >= 0 && o <= 60 * 86400)
    .sort((a, b) => a - b);
}

/** 展示排序：提前量从大到小（与预设顺序一致），标签位置保持稳定 */
export function sortOffsetsForDisplay(offsets: number[]): number[] {
  return [...offsets].sort((a, b) => b - a);
}
