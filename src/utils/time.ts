/**
 * 时间工具：统一使用真实时间戳（Date.now()）与 UTC 存储策略。
 * 所有展示使用用户本地时区（Date 自动处理）。
 * 不允许字符串比较日期。
 *
 * 日历边界一律用 new Date(y, m, d ± n) 构造（日历语义），
 * 而非毫秒加减（+24h 在夏令时切换日会得到 23/25 小时，跨错日期）。
 */

/** 将 datetime-local 输入值（本地时区，如 "2026-10-26T18:00"）转为 UTC RFC3339 */
export function localInputToUtcIso(value: string): string | null {
  if (!value) return null;
  // datetime-local 的值被 Date 构造函数按本地时区解释，这正是我们需要的
  const d = new Date(value);
  if (Number.isNaN(d.getTime())) return null;
  return d.toISOString();
}

/** 将 UTC RFC3339 转为 datetime-local 输入值（本地时区，分钟精度） */
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

/**
 * 今天在本地时区的起止时间戳。
 * 结束点用「明天零点」（日历法构造）——夏令时切换日可能只有 23/25 小时，
 * 简单 +24h 会跨错日期。
 */
export function localDayRange(now: number): { start: number; end: number } {
  const d = new Date(now);
  const start = new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const end = new Date(d.getFullYear(), d.getMonth(), d.getDate() + 1).getTime();
  return { start, end };
}

/** 本地日历「今天」指定时刻的时间戳 */
export function calendarTodayAt(now: number, hour: number, minute: number): number {
  const d = new Date(now);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate(), hour, minute, 0, 0).getTime();
}

/** 本地日历「明天」指定时刻的时间戳（日历法，深夜 23:59 调用也正确） */
export function calendarTomorrowAt(now: number, hour: number, minute: number): number {
  const d = new Date(now);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() + 1, hour, minute, 0, 0).getTime();
}

/** 本地日历「下一个周六」指定时刻的时间戳 */
export function calendarNextSaturdayAt(now: number, hour: number, minute: number): number {
  const d = new Date(now);
  const daysAhead = (6 - d.getDay() + 7) % 7 || 7; // 周六=6；今天是周六则取下周
  return new Date(
    d.getFullYear(),
    d.getMonth(),
    d.getDate() + daysAhead,
    hour,
    minute,
    0,
    0,
  ).getTime();
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
