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
  useState,
  type ReactNode,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type DataChangedPayload } from "@/services/api";
import type { AppSettings, AppState, Category, TaskInput } from "@/types";

interface AppDataValue {
  state: AppState;
  loading: boolean;
  error: string | null;
  reload: () => Promise<void>;
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
    default_reminder_offsets: [3600, 600],
    widget_display_count: 8,
    launch_at_login: false,
  },
};

export function AppDataProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<AppState>(EMPTY_STATE);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      const s = await api.getAppState();
      setState(s);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void reload();
    let unlisten: (() => void) | null = null;
    let destroyed = false;

    (async () => {
      const stop = await listen<DataChangedPayload>("data-changed", () => {
        // 后端广播：任何窗口的修改都会触发本窗口刷新
        void reload();
      });
      if (destroyed) stop();
      else unlisten = stop;
    })();

    return () => {
      destroyed = true;
      unlisten?.();
    };
  }, [reload]);

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
    // 乐观更新本地状态，避免滑块类控件等待往返
    setState((s) => ({ ...s, settings: { ...s.settings, ...patch } }));
    try {
      const updated = await api.updateSettings(patch);
      setState((s) => ({ ...s, settings: updated }));
    } catch (e) {
      await reload();
      throw e;
    }
  }, [reload]);

  const value = useMemo<AppDataValue>(
    () => ({
      state,
      loading,
      error,
      reload,
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
      error,
      reload,
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
