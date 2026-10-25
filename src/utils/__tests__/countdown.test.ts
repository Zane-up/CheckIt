import { describe, expect, it } from "vitest";
import { computeCountdown, formatRemaining, taskUrgency, utcTimestamp } from "@/utils/countdown";
import type { Task } from "@/types";

/** 构造测试任务 */
function makeTask(partial: Partial<Task>): Task {
  return {
    id: 1,
    title: "测试任务",
    description: "",
    category_id: null,
    priority: "medium",
    status: "pending",
    due_at: null,
    created_at: "2026-10-01T00:00:00Z",
    updated_at: "2026-10-01T00:00:00Z",
    completed_at: null,
    reminders: [],
    ...partial,
  };
}

describe("formatRemaining", () => {
  it("小于 1 分钟只显示秒", () => {
    expect(formatRemaining(45_000)).toBe("45秒");
  });

  it("小于 1 小时显示分和秒", () => {
    expect(formatRemaining(5 * 60_000 + 30_000)).toBe("5分钟 30秒");
  });

  it("小于 24 小时显示小时和分钟", () => {
    expect(formatRemaining(3 * 3600_000 + 20 * 60_000)).toBe("3小时 20分钟");
  });

  it("超过 24 小时显示天和小时", () => {
    expect(formatRemaining(2 * 86400_000 + 5 * 3600_000)).toBe("2天 5小时");
  });
});

describe("computeCountdown", () => {
  const now = utcTimestamp(2026, 10, 25, 12, 0, 0);

  it("超过 24 小时：normal 紧急度", () => {
    const due = new Date(utcTimestamp(2026, 10, 27, 18, 0, 0)).toISOString();
    const cd = computeCountdown(due, now)!;
    expect(cd.urgency).toBe("normal");
    expect(cd.text).toBe("2天 6小时");
    expect(cd.overdue).toBe(false);
  });

  it("24 小时内：warning 紧急度", () => {
    const due = new Date(utcTimestamp(2026, 10, 26, 10, 0, 0)).toISOString();
    const cd = computeCountdown(due, now)!;
    expect(cd.urgency).toBe("warning");
    expect(cd.text).toBe("22小时 0分钟");
  });

  it("1 小时内：urgent 紧急度", () => {
    const due = new Date(utcTimestamp(2026, 10, 25, 12, 45, 0)).toISOString();
    const cd = computeCountdown(due, now)!;
    expect(cd.urgency).toBe("urgent");
    expect(cd.text).toBe("45分钟 0秒");
  });

  it("已过期：overdue，文本带逾期前缀", () => {
    const due = new Date(utcTimestamp(2026, 10, 24, 10, 0, 0)).toISOString();
    const cd = computeCountdown(due, now)!;
    expect(cd.urgency).toBe("overdue");
    expect(cd.overdue).toBe(true);
    expect(cd.text).toBe("逾期 1天 2小时");
  });

  it("null 截止时间返回 null", () => {
    expect(computeCountdown(null, now)).toBeNull();
  });

  it("非法时间字符串返回 null", () => {
    expect(computeCountdown("not-a-date", now)).toBeNull();
  });

  it("跨月边界正确", () => {
    const now2 = utcTimestamp(2026, 10, 31, 23, 0, 0);
    const due = new Date(utcTimestamp(2026, 11, 1, 8, 0, 0)).toISOString();
    const cd = computeCountdown(due, now2)!;
    expect(cd.text).toBe("9小时 0分钟");
  });

  it("跨年边界正确", () => {
    const now2 = utcTimestamp(2026, 12, 31, 20, 0, 0);
    const due = new Date(utcTimestamp(2027, 1, 1, 6, 0, 0)).toISOString();
    const cd = computeCountdown(due, now2)!;
    expect(cd.text).toBe("10小时 0分钟");
  });

  it("闰年 2 月 29 日正确", () => {
    const now2 = utcTimestamp(2028, 2, 28, 12, 0, 0);
    const due = new Date(utcTimestamp(2028, 2, 29, 12, 0, 0)).toISOString();
    const cd = computeCountdown(due, now2)!;
    expect(cd.text).toBe("1天 0小时");
  });

  it("使用真实差值：同一截止时间随 now 推进而变化", () => {
    const due = new Date(utcTimestamp(2026, 10, 25, 13, 0, 0)).toISOString();
    const before = computeCountdown(due, utcTimestamp(2026, 10, 25, 12, 0, 0))!;
    const after = computeCountdown(due, utcTimestamp(2026, 10, 25, 12, 30, 0))!;
    expect(after.totalMs).toBe(before.totalMs - 30 * 60_000);
  });
});

describe("taskUrgency", () => {
  const now = utcTimestamp(2026, 10, 25, 12, 0, 0);

  it("已完成任务不显示紧急倒计时", () => {
    const task = makeTask({
      status: "completed",
      completed_at: "2026-10-24T00:00:00Z",
      due_at: new Date(utcTimestamp(2026, 10, 25, 12, 30, 0)).toISOString(),
    });
    expect(taskUrgency(task, now)).toBe("done");
  });

  it("无截止时间为 none", () => {
    expect(taskUrgency(makeTask({}), now)).toBe("none");
  });

  it("未完成且紧急返回 urgent", () => {
    const task = makeTask({ due_at: new Date(utcTimestamp(2026, 10, 25, 12, 30, 0)).toISOString() });
    expect(taskUrgency(task, now)).toBe("urgent");
  });
});
