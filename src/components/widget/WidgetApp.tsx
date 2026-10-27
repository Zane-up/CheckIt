/**
 * 桌面悬浮小组件（主界面）。
 * 布局：标题栏 / 状态概览 / 任务列表 / 底部进度。
 *
 * 收起/展开：动态调整原生窗口 min-size（收起高度 64 < 默认 minHeight），
 * 展开入口（收起条、快速添加、托盘）全部走 expandWindow() 统一恢复尺寸。
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { ChevronUp } from "lucide-react";
import { useAppData } from "@/hooks/useAppData";
import { useNow } from "@/hooks/useNow";
import { computeStats, sortTasks } from "@/utils/tasks";
import { t } from "@/i18n/zh";
import WidgetHeader from "@/components/widget/WidgetHeader";
import StatsOverview from "@/components/widget/StatsOverview";
import TaskList from "@/components/widget/TaskList";
import WidgetFooter from "@/components/widget/WidgetFooter";
import QuickAddTask from "@/components/widget/QuickAddTask";

/** 收起态高度（逻辑像素）与展开态默认/最小尺寸 */
const COLLAPSED_HEIGHT = 64;
const EXPANDED_MIN_W = 300;
const EXPANDED_MIN_H = 200;
const DEFAULT_EXPANDED_H = 440;

export default function WidgetApp() {
  const { state } = useAppData();
  const now = useNow();
  const [collapsed, setCollapsed] = useState(false);
  const [quickAddOpen, setQuickAddOpen] = useState(false);
  /** 展开态高度记忆（收起前的真实高度，用于恢复） */
  const expandedHeightRef = useRef<number | null>(null);
  // 缓存窗口对象：getCurrentWindow() 每次返回新包装，若参与回调依赖会导致
  // 每秒时钟 tick 重新注册托盘快速添加监听器。
  const win = useMemo(() => getCurrentWindow(), []);

  const expandWindow = useCallback(async () => {
    try {
      const scale = await win.scaleFactor();
      const size = await win.innerSize();
      const width = size.width / scale;
      const height = size.height / scale;
      // 记忆当前高度（若处于展开高度才记忆，避免把收起高度存进去）
      if (height > COLLAPSED_HEIGHT + 8) {
        expandedHeightRef.current = height;
      }
      // 先放宽 min-size 再恢复尺寸（顺序关键：否则尺寸被当前 min 卡住）
      await win.setMinSize(new LogicalSize(EXPANDED_MIN_W, EXPANDED_MIN_H));
      const target = expandedHeightRef.current ?? DEFAULT_EXPANDED_H;
      await win.setSize(new LogicalSize(width, target));
      setCollapsed(false);
    } catch (e) {
      console.error("展开窗口失败", e);
      setCollapsed(false);
    }
  }, [win]);

  const collapseWindow = useCallback(async () => {
    try {
      const scale = await win.scaleFactor();
      const size = await win.innerSize();
      const width = size.width / scale;
      const height = size.height / scale;
      if (height > COLLAPSED_HEIGHT + 8) {
        expandedHeightRef.current = height;
      }
      await win.setMinSize(new LogicalSize(EXPANDED_MIN_W, COLLAPSED_HEIGHT));
      await win.setSize(new LogicalSize(width, COLLAPSED_HEIGHT));
      setCollapsed(true);
    } catch (e) {
      console.error("收起窗口失败", e);
      setCollapsed(false);
    }
  }, [win]);

  const openQuickAdd = useCallback(() => {
    setQuickAddOpen(true);
    // 任何入口（标题栏 + / 托盘"添加任务"）都先确保展开到可用尺寸。
    // 不依赖 collapsed 状态：即使收起动画/事件与点击交错，也能恢复尺寸。
    void expandWindow();
  }, [expandWindow]);

  // 托盘"添加任务"入口：后端通过事件请求悬浮窗打开快速添加。
  // openQuickAdd 依赖稳定（expandWindow 依赖稳定的 win），时钟 tick 不会重注册。
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let destroyed = false;
    (async () => {
      if (typeof window === "undefined") return;
      const stop = await listen("widget-quick-add", () => openQuickAdd());
      if (destroyed) stop();
      else unlisten = stop;
    })();
    return () => {
      destroyed = true;
      unlisten?.();
    };
  }, [openQuickAdd]);

  // 排序/统计按分钟粒度重算（跨日/跨分钟边界足够），倒计时显示仍由共享 1Hz 时钟驱动
  const nowMinute = Math.floor(now / 60000);
  const pendingSorted = useMemo(
    () =>
      sortTasks(
        state.tasks.filter((task) => task.status === "pending"),
        "due_asc",
        now,
      ),
    // eslint 未启用；now 仅作显示，排序本身不依赖它
    [state.tasks, nowMinute],
  );
  const stats = useMemo(() => computeStats(state.tasks, now), [state.tasks, nowMinute]);

  const today = new Date(now);
  const dateText = `${today.getMonth() + 1}月${today.getDate()}日 周${"日一二三四五六"[today.getDay()]}`;

  return (
    <div className="glass-panel flex h-full w-full flex-col overflow-hidden rounded-2xl">
      <WidgetHeader
        dateText={dateText}
        onAdd={openQuickAdd}
        onToggleCollapse={() => (collapsed ? void expandWindow() : void collapseWindow())}
        collapsed={collapsed}
      />

      {!collapsed && (
        <div className="anim-fade-in flex min-h-0 flex-1 flex-col">
          <StatsOverview stats={stats} />
          {quickAddOpen && (
            <QuickAddTask
              onClose={() => setQuickAddOpen(false)}
              defaultOffsets={state.settings.default_reminder_offsets}
            />
          )}
          <TaskList tasks={pendingSorted} displayCount={state.settings.widget_display_count} now={now} />
          <WidgetFooter stats={stats} />
        </div>
      )}

      {collapsed && (
        <button
          type="button"
          className="hover-surface flex h-6 items-center justify-center text-[var(--text-muted)]"
          onClick={() => void expandWindow()}
          title={t.widget.expand}
        >
          <ChevronUp size={14} />
        </button>
      )}
    </div>
  );
}
