/**
 * Tauri 命令封装层：前端唯一与后端通信的入口。
 * 组件不得直接 invoke，便于统一错误处理与类型约束。
 */

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { errorText } from "@/i18n";
import type { AppState, AppSettings, Category, Task, TaskInput } from "@/types";

async function invoke<T>(cmd: string, payload?: Record<string, unknown>): Promise<T> {
  try {
    return (await tauriInvoke(cmd, payload)) as T;
  } catch (e) {
    // Rust 侧返回的 Err 为字符串，保持原样向上抛出
    throw new Error(errorText(e));
  }
}

export const api = {
  getDeletedTasks: () => invoke<{task:Task;deleted_at:string}[]>("get_deleted_tasks"),
  restoreTask: (id:number) => invoke<void>("restore_task",{id}),
  getBackupStatus: () => invoke<BackupStatus>("get_backup_status"),
  retryAutoBackup: () => invoke<BackupStatus>("retry_auto_backup"),
  openSettings: () => invoke<void>("open_manager_target", {target:{view:"settings"}}),
  openTask: (taskId: number) => invoke<void>("open_manager_target", {target:{view:"task",task_id:taskId}}),
  getManagerNavigation: () => invoke<ManagerNavigationRequest | null>("get_manager_navigation"),
  acknowledgeManagerNavigation: (id:number) => invoke<void>("acknowledge_manager_navigation", {id}),
  completePopupReminder: (id: number) => invoke<void>("complete_popup_reminder", {id}),
  createBackup: () => invoke<string>("create_backup"),
  listBackups: () => invoke<{ name: string; bytes: number }[]>("list_backups"),
  restoreBackup: (name: string) => invoke<void>("restore_backup", { name }),
  openBackupFolder: () => invoke<void>("open_backup_folder"),
  snoozePopupReminders: (ids: number[], minutes: number) => invoke<void>("snooze_popup_reminders", { ids, minutes }),
  openReleasePage: () => invoke<void>("open_release_page"),
  /** 一次性加载全部状态（任务/分类/设置） */
  getAppState: async (): Promise<AppState> => {
    // 静态 WebView 可以早于 Rust setup 完成加载。只重试这个明确的瞬时错误；
    // 磁盘、数据库等真实失败仍直接交给 DataGate 展示。
    for (let attempt = 0; ; attempt++) {
      try { return await invoke<AppState>("get_app_state"); }
      catch (e) {
        if (attempt >= 19 || !(e instanceof Error) || !e.message.includes("state not managed")) throw e;
        await new Promise<void>((resolve) => window.setTimeout(resolve, 250));
      }
    }
  },

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

  getPopupReminders: () => invoke<PopupSnapshot>("get_popup_reminders"),
  dismissPopupReminders: (ids: number[]) => invoke<void>("dismiss_popup_reminders", { ids }),
  previewPopupReminder: () => invoke<void>("preview_popup_reminder"),

  getDbPath: () => invoke<string>("get_db_path"),

  /** 打开完整管理窗口 */
  openManager: () => invoke<void>("open_manager"),

  /** 显示悬浮窗 */
  showWidget: () => invoke<void>("show_widget"),

  /** 悬浮窗模式：expanded（完整面板）/ collapsed（标题栏）/ docked（贴边圆圈） */
  widgetGetMode: () => invoke<WidgetMode>("widget_get_mode"),
  /** 展开为完整面板；minHeight 为快速添加所需最小高度（临时抬高，不写回正常几何） */
  widgetExpand: (minHeight?: number) =>
    invoke<WidgetMode>("widget_expand", { minHeight: minHeight ?? null }),
  /** 收起为标题栏 */
  widgetCollapse: () => invoke<WidgetMode>("widget_collapse"),
  /** 快速添加已关闭：清除临时高度并按需恢复 */
  widgetQuickAddClosed: () => invoke<WidgetMode>("widget_quick_add_closed"),

  quitApp: () => invoke<void>("quit_app"),
};

export type WidgetMode = "expanded" | "collapsed" | "docked";
export interface BackupStatus {last_success:string|null;last_attempt:string|null;error:string|null}
export interface ManagerNavigationRequest {
  id: number;
  target: {view:"settings"} | {view:"task";task_id:number};
}

export interface PopupReminder {
  id: number;
  task_id: number;
  title: string;
  body: string;
  due_at: string;
}

export interface PopupSnapshot {
  reminders: PopupReminder[];
  preview: boolean;
}

/** 后端数据变更事件载荷 */
export interface DataChangedPayload {
  entity: "tasks" | "categories" | "settings";
}
