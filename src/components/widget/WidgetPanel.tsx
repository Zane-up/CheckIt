import { widgetDate, useI18n } from "@/i18n";
/**
 * 悬浮窗完整面板与标题栏收起视图。
 *
 * 从 WidgetApp 抽出，供 WidgetApp（React 模式）与 DockBubble（圆圈模式）共用，
 * 保证"完整面板 / 标题栏 / 贴边圆圈"三种形态视觉与交互一致。
 */

import { useMemo } from "react";
import type { AppSettings, Task } from "@/types";
import { computeStats, sortTasks } from "@/utils/tasks";
import WidgetHeader from "@/components/widget/WidgetHeader";
import StatsOverview from "@/components/widget/StatsOverview";
import TaskList from "@/components/widget/TaskList";
import WidgetFooter from "@/components/widget/WidgetFooter";
import QuickAddTask, { type QuickAddDraft } from "@/components/widget/QuickAddTask";

export interface FullPanelProps {
  tasks: Task[];
  settings: AppSettings;
  now: number;
  quickAddOpen: boolean;
  quickAddDraft: QuickAddDraft;
  onDraftChange: (draft: QuickAddDraft) => void;
  onAdd: () => void;
  onCollapse: () => void;
  onCloseQuickAdd: () => void;
}

/** 完整面板：标题栏 / 状态概览 / 快速添加 / 任务列表 / 底部进度 */
export function WidgetFullPanel({
  tasks,
  settings,
  now,
  quickAddOpen,
  quickAddDraft,
  onDraftChange,
  onAdd,
  onCollapse,
  onCloseQuickAdd,
}: FullPanelProps) {
  useI18n();
  // 排序只依赖任务；统计随共享时钟更新，保证截止秒数与倒计时一致。
  const pendingSorted = useMemo(
    () => sortTasks(tasks.filter((task) => task.status === "pending"), "due_asc", 0),
    [tasks],
  );
  const stats = useMemo(() => computeStats(tasks, now), [tasks, now]);

  const dateText = widgetDate(now);

  return (
    <div className="glass-panel native-window-surface flex h-full w-full flex-col overflow-hidden">
      <WidgetHeader dateText={dateText} onAdd={onAdd} onToggleCollapse={onCollapse} collapsed={false} />

      <div className="anim-fade-in flex min-h-0 flex-1 flex-col">
        <StatsOverview stats={stats} />
        {quickAddOpen && (
          <QuickAddTask
            onClose={onCloseQuickAdd}
            defaultOffsets={settings.default_reminder_offsets}
            draft={quickAddDraft}
            onDraftChange={onDraftChange}
          />
        )}
        <TaskList tasks={pendingSorted} displayCount={settings.widget_display_count} now={now} />
        <WidgetFooter stats={stats} />
      </div>
    </div>
  );
}

export default WidgetFullPanel;

/** 标题栏收起视图：只保留标题栏（原生窗口高度 64，标题栏自身的展开按钮即可恢复） */
export function WidgetCollapsedBar({
  now,
  onExpand,
  onAdd,
}: {
  now: number;
  onExpand: () => void;
  onAdd: () => void;
}) {
  useI18n();
  const dateText = widgetDate(now);

  return (
    <div className="glass-panel native-window-surface flex h-full w-full flex-col overflow-hidden">
      <WidgetHeader dateText={dateText} onAdd={onAdd} onToggleCollapse={onExpand} collapsed />
    </div>
  );
}
