/**
 * 时间工具：统一使用真实时间戳（Date.now()）与 UTC 存储策略。
 * 所有展示使用用户本地时区（Date 自动处理）。
 * 不允许字符串比较日期。
 */

/** 将 datetime-local 输入值（本地时区，如 "2026-10-26T18:00"）转为 UTC RFC3339 */
export function localInputToUtcIso(value: string): string | null {
  if (!value) return null;
  // datetime-local 的值被 Date 构造函数按本地时区解释，这正是我们需要的
  const d = new Date(value);
  if (Number.isNaN(d.getTime())) return null;
  return d.toISOString();
}

/** 将 UTC RFC3339 转为 datetime-local 输入值（本地时区） */
export function utcIsoToLocalInput(iso: string | null): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** 本地时区展示：10月26日 18:00 */
export function formatDueLocal(iso: string | null): string {
  if (!iso) return "无截止时间";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "无效时间";
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getMonth() + 1}月${d.getDate()}日 ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** 今天在本地时区的起止时间戳（用于"今日到期"统计） */
export function localDayRange(now: number): { start: number; end: number } {
  const d = new Date(now);
  const start = new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  return { start, end: start + 24 * 3600 * 1000 };
}

/** 截止时间是否落在本地"今天"内 */
export function isDueToday(dueIso: string | null, now: number): boolean {
  if (!dueIso) return false;
  const due = new Date(dueIso).getTime();
  if (Number.isNaN(due)) return false;
  const { start, end } = localDayRange(now);
  return due >= start && due < end;
}

/** 截止时间是否已经过去 */
export function isOverdue(dueIso: string | null, now: number): boolean {
  if (!dueIso) return false;
  const due = new Date(dueIso).getTime();
  if (Number.isNaN(due)) return false;
  return due < now;
}
