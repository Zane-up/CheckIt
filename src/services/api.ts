/**
 * Tauri 命令封装层：前端唯一与后端通信的入口。
 * 组件不得直接 invoke，便于统一错误处理与类型约束。
 */

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import type { AppState, AppSettings, Category, Task, TaskInput } from "@/types";

async function invoke<T>(cmd: string, payload?: Record<string, unknown>): Promise<T> {
  try {
    return (await tauriInvoke(cmd, payload)) as T;
  } catch (e) {
    // Rust 侧返回的 Err 为字符串，保持原样向上抛出
    throw e instanceof Error ? e : new Error(String(e));
  }
}

export const api = {
  /** 一次性加载全部状态（任务/分类/设置） */
  getAppState: () => invoke<AppState>("get_app_state"),

  createTask: (input: TaskInput) => invoke<Task>("create_task", { input }),
  updateTask: (id: number, input: TaskInput) => invoke<Task>("update_task", { id, input }),
  deleteTask: (id: number) => invoke<void>("delete_task", { id }),
  setTaskStatus: (id: number, completed: boolean) =>
    invoke<Task>("set_task_status", { id, completed }),

  createCategory: (name: string, color: string) =>
    invoke<Category>("create_category", { name, color }),
  updateCategory: (id: number, name: string, color: string) =>
    invoke<Category>("update_category", { id, name, color }),
  deleteCategory: (id: number) => invoke<void>("delete_category", { id }),

  updateSettings: (patch: Partial<AppSettings>) =>
    invoke<AppSettings>("update_settings", { patch }),

  /** 切换悬浮窗置顶（唯一路径：窗口+设置+托盘+广播由后端原子完成） */
  toggleAlwaysOnTop: () => invoke<boolean>("toggle_always_on_top"),

  /** 手动触发一次提醒检查（窗口聚焦/恢复时调用，加速补查） */
  checkRemindersNow: () => invoke<void>("check_reminders_now"),

  getDbPath: () => invoke<string>("get_db_path"),

  /** 打开完整管理窗口 */
  openManager: () => invoke<void>("open_manager"),

  /** 显示悬浮窗 */
  showWidget: () => invoke<void>("show_widget"),

  quitApp: () => invoke<void>("quit_app"),
};

/** 后端数据变更事件载荷 */
export interface DataChangedPayload {
  entity: "tasks" | "categories" | "settings";
}
