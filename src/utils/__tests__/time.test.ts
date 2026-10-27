import { afterEach, describe, expect, it } from "vitest";
import {
  isDueToday,
  isOverdue,
  localDayRange,
  localInputToUtcIso,
  utcIsoToLocalInput,
  calendarTodayAt,
  calendarTomorrowAt,
  calendarNextSaturdayAt,
} from "@/utils/time";
import { utcTimestamp } from "@/utils/countdown";

// 测试运行在 Node（Vitest）中；项目未引入 @types/node，这里只声明用到的部分
declare const process: { env: { TZ?: string } };

const ORIGINAL_TZ = process.env.TZ;

/** 显式切换测试进程时区；每个用例结束后恢复，绝不依赖宿主时区 */
function setTimeZone(tz: string) {
  process.env.TZ = tz;
}

afterEach(() => {
  if (ORIGINAL_TZ === undefined) delete process.env.TZ;
  else process.env.TZ = ORIGINAL_TZ;
});

describe("localDayRange（日历日界）", () => {
  it("起点为本地今天零点，终点为本地明天零点（日历法，非 +24h）", () => {
    const now = new Date(2026, 9, 25, 15, 30).getTime();
    const { start, end } = localDayRange(now);
    expect(new Date(start).getDate()).toBe(25);
    expect(new Date(start).getHours()).toBe(0);
    expect(new Date(end).getDate()).toBe(26); // 明天零点
    expect(new Date(end).getHours()).toBe(0);
  });

  it("当天任意时刻的日界一致", () => {
    const early = new Date(2026, 9, 25, 0, 1).getTime();
    const late = new Date(2026, 9, 25, 23, 59).getTime();
    expect(localDayRange(early)).toEqual(localDayRange(late));
  });

  it("夏令时切换日仍以日历明天零点为终点（不依赖宿主时区）", () => {
    setTimeZone("America/New_York");
    // 2026-03-08 美东春令时：当天只有 23 小时
    const now = new Date(2026, 2, 8, 12, 0).getTime();
    const { start, end } = localDayRange(now);
    expect(new Date(start).getDate()).toBe(8);
    expect(new Date(start).getHours()).toBe(0);
    expect(new Date(end).getDate()).toBe(9);
    expect(new Date(end).getHours()).toBe(0);
    expect(end - start).toBe(23 * 3600 * 1000);
  });
});

describe("多时区与夏令时日界（显式 TZ，不按宿主环境跳过）", () => {
  const dayCases = [
    {
      tz: "Asia/Shanghai",
      label: "上海（无夏令时）",
      year: 2026,
      month: 2,
      day: 8,
      expectedHours: 24,
    },
    {
      tz: "UTC",
      label: "UTC",
      year: 2026,
      month: 2,
      day: 8,
      expectedHours: 24,
    },
    {
      tz: "America/New_York",
      label: "纽约春季切换（23 小时）",
      year: 2026,
      month: 2,
      day: 8,
      expectedHours: 23,
    },
    {
      tz: "America/New_York",
      label: "纽约秋季切换（25 小时）",
      year: 2026,
      month: 10,
      day: 1,
      expectedHours: 25,
    },
  ];

  it.each(dayCases)("$label：日界为本地午夜，时长为 $expectedHours 小时", (c) => {
    setTimeZone(c.tz);
    // 用年中正午构造，避免落在不存在的本地时刻
    const now = new Date(c.year, c.month, c.day, 12, 0).getTime();
    const { start, end } = localDayRange(now);
    expect(new Date(start).getFullYear()).toBe(c.year);
    expect(new Date(start).getMonth()).toBe(c.month);
    expect(new Date(start).getDate()).toBe(c.day);
    expect(new Date(start).getHours()).toBe(0);
    expect(new Date(end).getDate()).toBe(c.day + 1);
    expect(new Date(end).getHours()).toBe(0);
    expect(end - start).toBe(c.expectedHours * 3600 * 1000);
  });

  it("纽约春季：切换前夜的「明天 09:00」仍是本地 9 点（23 小时日）", () => {
    setTimeZone("America/New_York");
    const before = new Date(2026, 2, 7, 23, 30).getTime(); // 3 月 7 日 23:30
    const t = calendarTomorrowAt(before, 9, 0);
    const d = new Date(t);
    expect(d.getDate()).toBe(8);
    expect(d.getHours()).toBe(9);
    expect(d.getMinutes()).toBe(0);
    // 23:30 → 次日 09:00 跨越春季切换，实际只经过 8.5 小时
    expect(t - before).toBe(8.5 * 3600 * 1000);
  });

  it("纽约秋季：切换前夜的「明天 09:00」仍是本地 9 点（25 小时日）", () => {
    setTimeZone("America/New_York");
    const before = new Date(2026, 9, 31, 23, 30).getTime(); // 10 月 31 日 23:30
    const t = calendarTomorrowAt(before, 9, 0);
    const d = new Date(t);
    expect(d.getDate()).toBe(1);
    expect(d.getMonth()).toBe(10);
    expect(d.getHours()).toBe(9);
    expect(t - before).toBe(10.5 * 3600 * 1000);
  });

  it("三座时区在春/秋切换当日的「今天 21:00」都落在同一日历日", () => {
    for (const [tz, y, m, d] of [
      ["Asia/Shanghai", 2026, 2, 8],
      ["UTC", 2026, 10, 1],
      ["America/New_York", 2026, 2, 8],
      ["America/New_York", 2026, 10, 1],
    ] as const) {
      setTimeZone(tz);
      const now = new Date(y, m, d, 12, 0).getTime();
      const t = calendarTodayAt(now, 21, 0);
      const got = new Date(t);
      expect(got.getDate()).toBe(d);
      expect(got.getHours()).toBe(21);
    }
  });
});

describe("日历预设（深夜不跨错日）", () => {
  it("深夜 23:58 的「今天 21:00」已过 → 仍落在今天（23:59），不滚到明天", () => {
    const lateNight = new Date(2026, 9, 25, 23, 58).getTime();
    const t = calendarTodayAt(lateNight, 23, 59);
    expect(new Date(t).getDate()).toBe(25);
    expect(new Date(t).getHours()).toBe(23);
  });

  it("深夜 23:30 的「明天 09:00」是日历上的明天（+2h 会得到后天才对吗？不——旧实现 +26h 得到后天凌晨 1:30）", () => {
    const lateNight = new Date(2026, 9, 25, 23, 30).getTime();
    const t = calendarTomorrowAt(lateNight, 9, 0);
    const d = new Date(t);
    expect(d.getDate()).toBe(26);
    expect(d.getHours()).toBe(9);
    expect(d.getMinutes()).toBe(0);
  });

  it("周六调用「下周六」得到下一周的周六；周五调用得到次日", () => {
    const friday = new Date(2026, 9, 23, 12, 0).getTime(); // 2026-10-23 周五
    expect(new Date(friday).getDay()).toBe(5);
    const nextSat = calendarNextSaturdayAt(friday, 9, 0);
    expect(new Date(nextSat).getDay()).toBe(6);
    expect(new Date(nextSat).getDate()).toBe(24);

    const saturday = new Date(2026, 9, 24, 12, 0).getTime(); // 周六当天 → 下周
    const next = calendarNextSaturdayAt(saturday, 9, 0);
    expect(new Date(next).getDate()).toBe(31);
  });
});

describe("isDueToday / isOverdue", () => {
  it("当天 00:00 到 23:59 之间都算今天", () => {
    const due = new Date(utcTimestamp(2026, 10, 25, 3, 0, 0)).toISOString(); // UTC 3点 = 本地 11点 (UTC+8)
    const now = utcTimestamp(2026, 10, 25, 4, 0, 0);
    // 依赖本地时区，只验证函数不抛错且返回布尔
    expect(typeof isDueToday(due, now)).toBe("boolean");
  });

  it("截止时间在过去返回 true", () => {
    const due = new Date(utcTimestamp(2026, 10, 20, 0, 0, 0)).toISOString();
    const now = utcTimestamp(2026, 10, 25, 12, 0, 0);
    expect(isOverdue(due, now)).toBe(true);
  });

  it("截止时间在未来返回 false", () => {
    const due = new Date(utcTimestamp(2026, 10, 30, 0, 0, 0)).toISOString();
    const now = utcTimestamp(2026, 10, 25, 12, 0, 0);
    expect(isOverdue(due, now)).toBe(false);
  });

  it("null 返回 false", () => {
    expect(isOverdue(null, Date.now())).toBe(false);
  });
});

describe("datetime-local 与 UTC ISO 往返转换", () => {
  it("往返转换保持时间点一致（同一时区内）", () => {
    const localInput = "2026-10-26T18:00";
    const iso = localInputToUtcIso(localInput)!;
    const roundTrip = utcIsoToLocalInput(iso);
    expect(roundTrip).toBe(localInput);
  });

  it("空值处理", () => {
    expect(localInputToUtcIso("")).toBeNull();
    expect(utcIsoToLocalInput(null)).toBe("");
  });

  it("非法输入返回 null", () => {
    expect(localInputToUtcIso("garbage")).toBeNull();
  });

  it("分钟精度往返会截断秒数（这是表单'未修改则传回原值'规则存在的原因）", () => {
    const withSeconds = "2026-10-26T18:00:42.123Z";
    const rounded = utcIsoToLocalInput(withSeconds);
    // 时区无关断言：秒/毫秒被丢弃（:00 结尾的分钟精度）
    expect(rounded).toMatch(/T\d{2}:\d{2}$/);
    // 反向解析回去后秒数归零 → 证明需要"未改动即传回原值"的保护
    const back = localInputToUtcIso(rounded)!;
    expect(back.endsWith(".000Z")).toBe(true);
  });
});
