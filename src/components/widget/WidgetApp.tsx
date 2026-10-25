/**
 * 桌面悬浮小组件（主界面）。
 * 布局：标题栏 / 状态概览 / 任务列表 / 底部进度。
 * 支持收起（仅标题栏）与展开。
 */

import { useCallback, useEffect, useRef, useState } from "react";
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

const COLLAPSED_HEIGHT = 52;

export default function WidgetApp() {
  const { state } = useAppData();
  const now = useNow();
  const [collapsed, setCollapsed] = useState(false);
  const [quickAddOpen, setQuickAddOpen] = useState(false);
  const expandedHeightRef = useRef<number | null>(null);
  const win = getCurrentWindow();

  const toggleCollapsed = useCallback(async () => {
    try {
      if (!collapsed) {
        // 收起：记住当前高度，窗口缩小到仅标题栏
        const size = await win.innerSize();
        const scale = await win.scaleFactor();
        expandedHeightRef.current = size.height / scale;
        await win.setSize(new LogicalSize(size.width / scale, COLLAPSED_HEIGHT));
        setCollapsed(true);
      } else {
        const size = await win.innerSize();
        const scale = await win.scaleFactor();
        const width = size.width / scale;
        const height = expandedHeightRef.current ?? 440;
        await win.setSize(new LogicalSize(width, height));
        setCollapsed(false);
      }
    } catch {
      // 尺寸调整失败时仅切换内容显示，不影响可用性
      setCollapsed((c) => !c);
    }
  }, [collapsed, win]);

  const openQuickAdd = useCallback(() => {
    setCollapsed(false);
    setQuickAddOpen(true);
  }, []);

  // 托盘"添加任务"入口：后端通过事件请求悬浮窗打开快速添加
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let destroyed = false;
    (async () => {
      const stop = await listen("widget-quick-add", () => openQuickAdd());
      if (destroyed) stop();
      else unlisten = stop;
    })();
    return () => {
      destroyed = true;
      unlisten?.();
    };
  }, [openQuickAdd]);

  const pendingSorted = sortTasks(
    state.tasks.filter((task) => task.status === "pending"),
    "due_asc",
    now,
  );
  const stats = computeStats(state.tasks, now);

  const today = new Date(now);
  const dateText = `${today.getMonth() + 1}月${today.getDate()}日 周${"日一二三四五六"[today.getDay()]}`;

  return (
    <div className="glass-panel flex h-full w-full flex-col overflow-hidden rounded-2xl">
      <WidgetHeader
        dateText={dateText}
        onAdd={openQuickAdd}
        onToggleCollapse={toggleCollapsed}
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
          onClick={() => void toggleCollapsed()}
          title={t.widget.expand}
        >
          <ChevronUp size={14} />
        </button>
      )}
    </div>
  );
}
