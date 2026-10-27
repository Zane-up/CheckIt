/**
 * 悬浮窗任务列表：显示最近即将截止的任务，支持滚动。
 * 列表项为共享的 TaskListItem（管理窗口复用）。
 */

import { Inbox } from "lucide-react";
import type { Task } from "@/types";
import { t } from "@/i18n/zh";
import TaskListItem from "@/components/shared/TaskListItem";

interface TaskListProps {
  tasks: Task[];
  displayCount: number;
  now: number;
}

export default function TaskList({ tasks, displayCount, now }: TaskListProps) {
  const shown = tasks.slice(0, Math.max(1, displayCount));

  return (
    <div className="min-h-0 flex-1 overflow-y-auto px-3">
      {shown.length === 0 ? (
        <div className="flex h-full flex-col items-center justify-center gap-1.5 py-6 text-center">
          <Inbox size={26} className="text-[var(--text-secondary)]" strokeWidth={1.5} />
          <div className="text-[13px] text-[var(--text-secondary)]">{t.widget.empty}</div>
          <div className="text-[12px] text-[var(--text-muted)]">{t.widget.emptyHint}</div>
        </div>
      ) : (
        <ul className="flex flex-col gap-1 pb-1">
          {shown.map((task) => (
            <TaskListItem key={task.id} task={task} now={now} compact />
          ))}
        </ul>
      )}
      {tasks.length > shown.length && (
        <div className="pb-1.5 pt-0.5 text-center text-[11px] text-[var(--text-secondary)]">
          还有 {tasks.length - shown.length} 个任务
        </div>
      )}
    </div>
  );
}
