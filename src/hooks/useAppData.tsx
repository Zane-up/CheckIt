/**
 * 应用数据上下文：任务 / 分类 / 设置 的唯一数据源。
 *
 * - 初次加载调用一次 get_app_state
 * - 监听后端广播的 data-changed 事件，任何窗口的修改都会让所有窗口同步刷新
 * - 提供 mutation 方法，写后由后端事件驱动统一重取，保证多窗口一致性
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type DataChangedPayload } from "@/services/api";
import type { AppSettings, AppState, Category, TaskInput } from "@/types";

interface AppDataValue {
  state: AppState;
  /** 首次快照是否仍在加载（只有首屏使用；后续刷新不卸载界面） */
  loading: boolean;
  /** 是否至少成功拿到过一次快照 */
  hasData: boolean;
  /** 最近一次快照刷新失败（有旧数据时仅显示提示，不清空界面） */
  error: string | null;
  /** 实时事件订阅失败（降级为手动刷新） */
  syncError: string | null;
  reload: () => Promise<void>;
  /** 重新注册事件订阅并重取快照 */
  retrySync: () => void;
  createTask: (input: TaskInput) => Promise<void>;
  updateTask: (id: number, input: TaskInput) => Promise<void>;
  deleteTask: (id: number) => Promise<void>;
  setTaskStatus: (id: number, completed: boolean) => Promise<void>;
  createCategory: (name: string, color: string) => Promise<void>;
  updateCategory: (id: number, name: string, color: string) => Promise<void>;
  deleteCategory: (id: number) => Promise<void>;
  updateSettings: (patch: Partial<AppSettings>) => Promise<void>;
}

const AppDataContext = createContext<AppDataValue | null>(null);

const EMPTY_STATE: AppState = {
  tasks: [],
  categories: [],
  settings: {
    theme: "dark",
    glass_enabled: true,
    glass_opacity: 0.35,
    always_on_top: true,
    close_to_tray: true,
    notifications_enabled: true,
    popup_reminders_enabled: false,
    default_reminder_offsets: [3600, 600],
    widget_display_count: 8,
    launch_at_login: false,
    edge_dock_enabled: true,
  },
};

export function AppDataProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<AppState>(EMPTY_STATE);
  const [loading, setLoading] = useState(true);
  const [hasData, setHasData] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [syncError, setSyncError] = useState<string | null>(null);
  /** 订阅重试代次：递增后重新执行"先订阅、后取快照"流程 */
  const [listenAttempt, setListenAttempt] = useState(0);
  /** 请求序号：仅最新请求的响应允许写入 state，防止旧慢响应覆盖新数据 */
  const seqRef = useRef(0);
  /** 设置写请求序号：旧响应不得覆盖新补丁的乐观结果 */
  const settingsSeqRef = useRef(0);

  const reload = useCallback(async () => {
    const seq = ++seqRef.current;
    try {
      const s = await api.getAppState();
      if (seq !== seqRef.current) return; // 已有更新的请求，丢弃本次结果
      setState(s);
      setHasData(true);
      setError(null);
    } catch (e) {
      if (seq !== seqRef.current) return;
      // 关键：失败只记录错误，不清空已有数据；首屏失败由 DataGate 显示错误页
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      if (seq === seqRef.current) {
        setLoading(false);
      }
    }
  }, []);

  const retrySync = useCallback(() => {
    setListenAttempt((n) => n + 1);
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;

    // 关键顺序：先成功注册 data-changed 订阅，再读取快照。
    // 这样"快照读完后、订阅生效前"发生的事件不会丢失：
    // 订阅期间来的事件会使本次快照请求序号过期，并在其后重新取最新数据。
    (async () => {
      try {
        const stop = await listen<DataChangedPayload>("data-changed", () => {
          void reload();
        });
        if (disposed) {
          stop();
          return;
        }
        unlisten = stop;
        setSyncError(null);
      } catch (e) {
        if (disposed) return;
        // 订阅失败仍要加载一次数据，让界面可用；同时明确提示同步已降级。
        setSyncError(e instanceof Error ? e.message : String(e));
      }

      if (!disposed) await reload();
    })();

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [reload, listenAttempt]);

  // —— 以下 mutation 均调用后端命令；后端完成写库后广播事件，触发本窗口与其他窗口刷新 ——

  const createTask = useCallback(async (input: TaskInput) => {
    await api.createTask(input);
  }, []);

  const updateTask = useCallback(async (id: number, input: TaskInput) => {
    await api.updateTask(id, input);
  }, []);

  const deleteTask = useCallback(async (id: number) => {
    await api.deleteTask(id);
  }, []);

  const setTaskStatus = useCallback(async (id: number, completed: boolean) => {
    await api.setTaskStatus(id, completed);
  }, []);

  const createCategory = useCallback(async (name: string, color: string) => {
    await api.createCategory(name, color);
  }, []);

  const updateCategory = useCallback(async (id: number, name: string, color: string) => {
    await api.updateCategory(id, name, color);
  }, []);

  const deleteCategory = useCallback(async (id: number) => {
    await api.deleteCategory(id);
  }, []);

  const updateSettings = useCallback(async (patch: Partial<AppSettings>) => {
    const seq = ++settingsSeqRef.current;
    // 乐观更新本地状态，避免滑块类控件等待往返
    setState((s) => ({ ...s, settings: { ...s.settings, ...patch } }));
    try {
      const updated = await api.updateSettings(patch);
      // 乱序保护：只接受最后一次设置请求的响应；旧响应可能缺少后续补丁
      if (seq === settingsSeqRef.current) {
        setState((s) => ({ ...s, settings: updated }));
      }
    } catch (e) {
      if (seq === settingsSeqRef.current) {
        // 失败后用数据库真实值回滚乐观更新；reload 失败会进入全局错误提示
        await reload();
      }
      throw e;
    }
  }, [reload]);

  const value = useMemo<AppDataValue>(
    () => ({
      state,
      loading,
      hasData,
      error,
      syncError,
      reload,
      retrySync,
      createTask,
      updateTask,
      deleteTask,
      setTaskStatus,
      createCategory,
      updateCategory,
      deleteCategory,
      updateSettings,
    }),
    [
      state,
      loading,
      hasData,
      error,
      syncError,
      reload,
      retrySync,
      createTask,
      updateTask,
      deleteTask,
      setTaskStatus,
      createCategory,
      updateCategory,
      deleteCategory,
      updateSettings,
    ],
  );

  return <AppDataContext.Provider value={value}>{children}</AppDataContext.Provider>;
}

export function useAppData(): AppDataValue {
  const ctx = useContext(AppDataContext);
  if (!ctx) throw new Error("useAppData must be used within AppDataProvider");
  return ctx;
}

export function useCategories(): Category[] {
  return useAppData().state.categories;
}
