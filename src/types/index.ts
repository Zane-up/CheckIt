import { tr } from "@/i18n";
/**
 * CheckIt 前端共享类型定义。
 * 与 Rust 侧 models 中的 serde 结构一一对应（snake_case 字段）。
 */

export type Priority = "high" | "medium" | "low";
export type TaskStatus = "pending" | "completed";
export type ThemeMode = "dark" | "light" | "system";

/** 截止时间的派生紧急程度（由倒计时计算得出，不落库） */
export type Urgency = "none" | "normal" | "warning" | "urgent" | "overdue" | "done";

export interface Category {
  id: number;
  name: string;
  color: string;
  sort_order: number;
  created_at: string;
  updated_at: string;
}

export interface Task {
  id: number;
  title: string;
  description: string;
  category_id: number | null;
  priority: Priority;
  status: TaskStatus;
  /** RFC3339 UTC；允许为 null（无截止时间的任务） */
  due_at: string | null;
  created_at: string;
  updated_at: string;
  completed_at: string | null;
  /** 该任务配置的提醒（offset_seconds = 提前秒数，0 表示截止时） */
  reminders: number[];
}

export interface AppSettings {
  language: "zh-CN" | "en";
  theme: ThemeMode;
  glass_enabled: boolean;
  /** 玻璃面板底色不透明度 0.05 ~ 0.6 */
  glass_opacity: number;
  always_on_top: boolean;
  close_to_tray: boolean;
  notifications_enabled: boolean;
  /** 屏幕居中提醒，可独立于系统通知启用 */
  popup_reminders_enabled: boolean;
  /** 新任务的默认提醒提前秒数 */
  default_reminder_offsets: number[];
  /** 悬浮窗任务列表默认显示条数 */
  widget_display_count: number;
  launch_at_login: boolean;
  /** 拖到屏幕边缘时收纳为圆圈（默认开启） */
  edge_dock_enabled: boolean;
}

export interface WindowGeometry {
  x: number;
  y: number;
  width: number;
  height: number;
  maximized: boolean;
}

/** 应用初始一次性加载的完整状态，减少多窗口首屏往返 */
export interface AppState {
  tasks: Task[];
  categories: Category[];
  settings: AppSettings;
}

/** 任务创建/编辑载荷 */
export interface TaskInput {
  title: string;
  description: string;
  category_id: number | null;
  priority: Priority;
  due_at: string | null;
  reminders: number[];
}

export const PRIORITY_ORDER: Record<Priority, number> = {
  high: 0,
  medium: 1,
  low: 2,
};

export const PRIORITY_LABEL: Record<Priority, string> = {
  get high() { return tr("高"); },
  get medium() { return tr("中"); },
  get low() { return tr("低"); },
};
