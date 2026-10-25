/**
 * 任务列表视图（管理窗口主区）：工具栏（搜索/筛选/排序）+ 任务列表 + 新建按钮。
 */

import { Filter, Plus, Search } from "lucide-react";
import { useAppData } from "@/hooks/useAppData";
import { t } from "@/i18n/zh";
import type { Task } from "@/types";
import type { SortKey, TaskFilter } from "@/utils/tasks";
import TaskListItem from "@/components/shared/TaskListItem";
import { Button } from "@/components/ui/Buttons";

interface TaskListViewProps {
  title: string;
  tasks: Task[];
  now: number;
  filter: TaskFilter;
  onFilterChange: (f: TaskFilter) => void;
  sortKey: SortKey;
  onSortChange: (k: SortKey) => void;
  onEditTask: (task: Task) => void;
  onCreate: () => void;
}

export default function TaskListView({
  title,
  tasks,
  now,
  filter,
  onFilterChange,
  sortKey,
  onSortChange,
  onEditTask,
  onCreate,
}: TaskListViewProps) {
  const { state } = useAppData();

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* 工具栏 */}
      <div className="flex h-[52px] shrink-0 items-center gap-2 border-b border-[var(--glass-border)] px-4">
        <h1 className="text-[15px] font-semibold">{title}</h1>
        <span className="text-xs text-[var(--text-muted)]">{tasks.length}</span>

        <div className="flex-1" />

        <div className="relative w-44">
          <Search size={13} className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-[var(--text-muted)]" />
          <input
            className="field-input !py-1 pl-7 text-xs"
            placeholder={t.filter.searchPlaceholder}
            value={filter.search}
            onChange={(e) => onFilterChange({ ...filter, search: e.target.value })}
          />
        </div>

        <select
          className="field-input !w-auto !py-1 text-xs"
          value={filter.categoryId ?? ""}
          onChange={(e) => onFilterChange({ ...filter, categoryId: e.target.value ? Number(e.target.value) : null })}
        >
          <option value="">{t.filter.allCategories}</option>
          {state.categories.map((c) => (
            <option key={c.id} value={c.id}>
              {c.name}
            </option>
          ))}
        </select>

        <select
          className="field-input !w-auto !py-1 text-xs"
          value={sortKey}
          onChange={(e) => onSortChange(e.target.value as SortKey)}
          title="排序"
        >
          <option value="due_asc">{t.filter.sortDueAsc}</option>
          <option value="due_desc">{t.filter.sortDueDesc}</option>
          <option value="priority">{t.filter.sortPriority}</option>
          <option value="created_desc">{t.filter.sortCreated}</option>
        </select>

        <Button variant="primary" size="sm" onClick={onCreate}>
          <Plus size={14} />
          {t.task.create}
        </Button>
      </div>

      {/* 任务列表 */}
      <div className="min-h-0 flex-1 overflow-y-auto px-2 py-2">
        {tasks.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 text-[var(--text-muted)]">
            <Filter size={26} strokeWidth={1.5} />
            <div className="text-[13px]">没有符合条件的任务</div>
          </div>
        ) : (
          <ul className="flex flex-col gap-0.5">
            {tasks.map((task) => (
              <TaskListItem key={task.id} task={task} now={now} onEdit={onEditTask} />
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
