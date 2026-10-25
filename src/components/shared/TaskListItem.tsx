/**
 * 任务列表项：悬浮窗与管理窗口共用。
 * compact 模式用于悬浮窗（更紧凑，点击不展开详情）。
 */

import { useAppData } from "@/hooks/useAppData";
import { t } from "@/i18n/zh";
import type { Task } from "@/types";
import { Checkbox } from "@/components/ui/Checkbox";
import CountdownBadge from "@/components/shared/CountdownBadge";
import { formatDueLocal } from "@/utils/time";

interface TaskListItemProps {
  task: Task;
  now: number;
  compact?: boolean;
  onEdit?: (task: Task) => void;
}

export default function TaskListItem({ task, now, compact = false, onEdit }: TaskListItemProps) {
  const { state, setTaskStatus } = useAppData();
  const category = task.category_id !== null
    ? state.categories.find((c) => c.id === task.category_id)
    : null;
  const completed = task.status === "completed";

  const toggle = async (checked: boolean) => {
    try {
      await setTaskStatus(task.id, checked);
    } catch (e) {
      console.error("更新任务状态失败", e);
    }
  };

  return (
    <li
      className={`group flex items-center gap-2 rounded-xl px-2 transition-colors hover:bg-[var(--surface-hover)] ${
        compact ? "py-1.5" : "py-2"
      } ${completed ? "opacity-60" : ""}`}
      onDoubleClick={() => onEdit?.(task)}
    >
      {/* 优先级指示条 */}
      {!completed && (
        <span
          className={`h-4 w-[3px] shrink-0 rounded-full prio-${task.priority}`}
          title={`${t.task.priority}: ${t.task[`priority${task.priority[0].toUpperCase()}${task.priority.slice(1)}` as "priorityHigh" | "priorityMedium" | "priorityLow"]}`}
        />
      )}

      <Checkbox checked={completed} onChange={(c) => void toggle(c)} title={completed ? t.task.restore : t.task.complete} />

      <div className="min-w-0 flex-1 cursor-default" onDoubleClick={() => onEdit?.(task)}>
        <div className="flex items-center gap-1.5">
          {category && (
            <span
              className="h-1.5 w-1.5 shrink-0 rounded-full"
              style={{ backgroundColor: category.color }}
              title={category.name}
            />
          )}
          <span
            className={`truncate text-[12.5px] leading-snug ${completed ? "line-through decoration-[var(--text-muted)]" : ""}`}
            title={task.title}
          >
            {task.title}
          </span>
        </div>
        {!compact && task.description && (
          <div className="selectable mt-0.5 truncate text-[11px] text-[var(--text-muted)]" title={task.description}>
            {task.description}
          </div>
        )}
      </div>

      <div className="flex shrink-0 flex-col items-end">
        <CountdownBadge dueAt={task.due_at} now={now} completed={completed} />
        {!compact && task.due_at && (
          <span className="text-[10px] text-[var(--text-muted)]">{formatDueLocal(task.due_at)}</span>
        )}
      </div>
    </li>
  );
}
