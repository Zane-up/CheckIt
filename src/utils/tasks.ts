/**
 * 任务列表的排序 / 筛选 / 统计纯函数（可测试）。
 */

import type { Category, Task } from "@/types";
import { PRIORITY_ORDER } from "@/types";
import { isDueToday, isOverdue } from "@/utils/time";
import { computeCountdown } from "@/utils/countdown";

export type SortKey = "due_asc" | "due_desc" | "priority" | "created_desc";

export interface TaskFilter {
  status: "all" | "pending" | "completed";
  categoryId: number | null; // null = 全部分类
  priority: "all" | "high" | "medium" | "low";
  search: string;
}

export const DEFAULT_FILTER: TaskFilter = {
  status: "all",
  categoryId: null,
  priority: "all",
  search: "",
};

export function applyTaskFilter(tasks: Task[], filter: TaskFilter): Task[] {
  const kw = filter.search.trim().toLowerCase();
  return tasks.filter((t) => {
    if (filter.status === "pending" && t.status !== "pending") return false;
    if (filter.status === "completed" && t.status !== "completed") return false;
    if (filter.categoryId !== null && t.category_id !== filter.categoryId) return false;
    if (filter.priority !== "all" && t.priority !== filter.priority) return false;
    if (kw) {
      const hit =
        t.title.toLowerCase().includes(kw) || t.description.toLowerCase().includes(kw);
      if (!hit) return false;
    }
    return true;
  });
}

export function sortTasks(tasks: Task[], key: SortKey, _now: number): Task[] {
  const arr = [...tasks];
  const dueMs = (t: Task) => (t.due_at ? new Date(t.due_at).getTime() : Number.POSITIVE_INFINITY);
  switch (key) {
    case "due_asc":
      arr.sort((a, b) => {
        // 已完成沉底；未完成按截止时间从近到远，无截止排最后
        if (a.status !== b.status) return a.status === "pending" ? -1 : 1;
        return dueMs(a) - dueMs(b);
      });
      break;
    case "due_desc":
      arr.sort((a, b) => dueMs(b) - dueMs(a));
      break;
    case "priority":
      arr.sort((a, b) => {
        if (a.status !== b.status) return a.status === "pending" ? -1 : 1;
        const p = PRIORITY_ORDER[a.priority] - PRIORITY_ORDER[b.priority];
        if (p !== 0) return p;
        return dueMs(a) - dueMs(b);
      });
      break;
    case "created_desc":
      arr.sort((a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime());
      break;
  }
  return arr;
}

export interface TaskStats {
  pending: number;
  dueToday: number;
  upcoming: number;
  overdue: number;
  completedToday: number;
  /** 今日应完成（今日到期或已逾期且未完成） */
  dueTotalToday: number;
}

export function computeStats(tasks: Task[], now: number): TaskStats {
  let pending = 0;
  let dueToday = 0;
  let upcoming = 0;
  let overdue = 0;
  let completedToday = 0;
  let dueTotalToday = 0;

  for (const t of tasks) {
    if (t.status === "pending") {
      pending += 1;
      const cd = computeCountdown(t.due_at, now);
      if (cd) {
        if (isOverdue(t.due_at, now)) {
          overdue += 1;
          dueTotalToday += 1;
        } else if (isDueToday(t.due_at, now)) {
          dueToday += 1;
          dueTotalToday += 1;
        } else if (cd.totalMs <= 3 * 24 * 3600 * 1000) {
          upcoming += 1;
        }
      }
    } else if (t.completed_at && isDueToday(t.completed_at, now)) {
      completedToday += 1;
    }
  }

  return { pending, dueToday, upcoming, overdue, completedToday, dueTotalToday };
}

/** 分类 → 未完成任务数映射（侧边栏计数） */
export function categoryCounts(tasks: Task[], categories: Category[]): Map<number, number> {
  const map = new Map<number, number>();
  for (const c of categories) map.set(c.id, 0);
  for (const t of tasks) {
    if (t.status === "pending" && t.category_id !== null) {
      map.set(t.category_id, (map.get(t.category_id) ?? 0) + 1);
    }
  }
  return map;
}
