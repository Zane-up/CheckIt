import { describe, expect, it } from "vitest";
import { applyTaskFilter, computeStats, sortTasks, DEFAULT_FILTER } from "@/utils/tasks";
import { normalizeOffsets, offsetLabel } from "@/utils/reminders";
import type { Task } from "@/types";
import { utcTimestamp } from "@/utils/countdown";

function makeTask(partial: Partial<Task>): Task {
  return {
    id: 1,
    title: "任务",
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

const NOW = utcTimestamp(2026, 10, 25, 12, 0, 0);
const iso = (y: number, m: number, d: number, h = 0) => new Date(utcTimestamp(y, m, d, h)).toISOString();

describe("sortTasks", () => {
  const tasks = [
    makeTask({ id: 1, due_at: iso(2026, 10, 28), priority: "low" }),
    makeTask({ id: 2, due_at: iso(2026, 10, 26), priority: "high" }),
    makeTask({ id: 3, due_at: null, priority: "medium" }),
    makeTask({ id: 4, due_at: iso(2026, 10, 20), priority: "medium", status: "completed" }),
  ];

  it("按截止时间升序，已完成沉底，无截止时间排最后", () => {
    const sorted = sortTasks(tasks, "due_asc", NOW);
    expect(sorted.map((t) => t.id)).toEqual([2, 1, 3, 4]);
  });

  it("按优先级排序", () => {
    const sorted = sortTasks(tasks, "priority", NOW);
    expect(sorted[0].id).toBe(2); // high
    expect(sorted[1].id).toBe(3); // medium
    expect(sorted[2].id).toBe(1); // low
  });
});

describe("applyTaskFilter", () => {
  const tasks = [
    makeTask({ id: 1, title: "数学作业", category_id: 1, priority: "high" }),
    makeTask({ id: 2, title: "英语阅读", category_id: 2, status: "completed" }),
    makeTask({ id: 3, title: "小组会议", description: "讨论数模", category_id: 1 }),
  ];

  it("按状态筛选", () => {
    expect(applyTaskFilter(tasks, { ...DEFAULT_FILTER, status: "completed" }).map((t) => t.id)).toEqual([2]);
  });

  it("按分类筛选", () => {
    expect(applyTaskFilter(tasks, { ...DEFAULT_FILTER, categoryId: 1 }).map((t) => t.id)).toEqual([1, 3]);
  });

  it("按优先级筛选", () => {
    expect(applyTaskFilter(tasks, { ...DEFAULT_FILTER, priority: "high" }).map((t) => t.id)).toEqual([1]);
  });

  it("按关键词搜索标题与描述", () => {
    expect(applyTaskFilter(tasks, { ...DEFAULT_FILTER, search: "数模" }).map((t) => t.id)).toEqual([3]);
    expect(applyTaskFilter(tasks, { ...DEFAULT_FILTER, search: "英语" }).map((t) => t.id)).toEqual([2]);
  });
});

describe("computeStats", () => {
  it("统计待办 / 逾期 / 今日到期", () => {
    // NOW 为 UTC 2026-10-25 12:00；测试机本地时区为 UTC+8，即本地 10月25日 20:00
    const tasks = [
      makeTask({ id: 1, due_at: iso(2026, 10, 20) }), // 已逾期
      makeTask({ id: 2, due_at: iso(2026, 10, 26) }), // 明天（本地）
      makeTask({ id: 3, status: "completed", completed_at: new Date(NOW - 3600_000).toISOString() }),
    ];
    const stats = computeStats(tasks, NOW);
    expect(stats.pending).toBe(2);
    expect(stats.overdue).toBe(1);
    expect(stats.completedToday).toBe(1);
  });
});

describe("normalizeOffsets / offsetLabel", () => {
  it("去重并升序", () => {
    expect(normalizeOffsets([3600, 600, 3600, 0])).toEqual([0, 600, 3600]);
  });

  it("过滤非法值", () => {
    expect(normalizeOffsets([-5, 61 * 86400, 1.5] as number[])).toEqual([]);
  });

  it("预设与自定义偏移的标签", () => {
    expect(offsetLabel(0)).toBe("截止时");
    expect(offsetLabel(600)).toBe("提前 10 分钟");
    expect(offsetLabel(86400)).toBe("提前 1 天");
    expect(offsetLabel(4500)).toBe("提前 75 分钟");
  });
});
