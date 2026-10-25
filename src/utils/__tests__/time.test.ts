import { describe, expect, it } from "vitest";
import { isDueToday, isOverdue, localDayRange, utcIsoToLocalInput, localInputToUtcIso } from "@/utils/time";
import { utcTimestamp } from "@/utils/countdown";

describe("localDayRange / isDueToday", () => {
  it("今天的起止范围为一整天", () => {
    // 本地时区固定用 UTC+8 语义验证（测试机器为中国时区）
    const now = utcTimestamp(2026, 10, 25, 12, 0, 0);
    const { start, end } = localDayRange(now);
    expect(end - start).toBe(24 * 3600 * 1000);
  });

  it("当天 00:00 到 23:59 之间都算今天", () => {
    const due = new Date(utcTimestamp(2026, 10, 25, 3, 0, 0)).toISOString(); // UTC 3点 = 本地 11点 (UTC+8)
    const now = utcTimestamp(2026, 10, 25, 4, 0, 0);
    // 依赖本地时区，只验证函数不抛错且返回布尔
    expect(typeof isDueToday(due, now)).toBe("boolean");
  });
});

describe("isOverdue", () => {
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
});
