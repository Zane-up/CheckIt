import { tr, useI18n } from "@/i18n";
/**
 * 完整任务管理窗口：左侧导航 + 主内容区。
 * 视图：全部 / 今日 / 未来 7 天 / 已逾期 / 已完成 / 分类 / 设置。
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { useManagerNavigation } from "@/hooks/useManagerNavigation";
import { api } from "@/services/api";
import {
  CalendarDays,
  CheckCircle2,
  Clock3,
  ListTodo,
  Settings as SettingsIcon,
  AlarmClockOff,
  Trash2,
} from "lucide-react";
import { useAppData } from "@/hooks/useAppData";
import { useNow } from "@/hooks/useNow";
import { t } from "@/i18n";
import type { Task } from "@/types";
import { applyTaskFilter, DEFAULT_FILTER, sortTasks, type SortKey, type TaskFilter } from "@/utils/tasks";
import { isDueToday, isOverdue } from "@/utils/time";
import TaskListView from "@/components/manager/TaskListView";
import CategoryManager from "@/components/manager/CategoryManager";
import TrashPanel from "./TrashPanel";
import SettingsPanel from "@/components/manager/SettingsPanel";
import TaskFormModal from "@/components/manager/TaskFormModal";

type ViewId =
  | { kind: "all" }
  | { kind: "today" }
  | { kind: "week" }
  | { kind: "overdue" }
  | { kind: "completed" }
  | { kind: "category"; id: number }
  | { kind: "categories" }
  | { kind: "trash" }
  | { kind: "settings" };

export default function ManagerApp() {
  useI18n();
  const { state } = useAppData();
  const now = useNow();
  const [view, setView] = useState<ViewId>({ kind: "all" });
  const [filter, setFilter] = useState<TaskFilter>(DEFAULT_FILTER);
  const [sortKey, setSortKey] = useState<SortKey>("due_asc");
  const [editingTask, setEditingTask] = useState<Task | null>(null);
  const [creating, setCreating] = useState(false);
  const {request,clearRequest}=useManagerNavigation();
  const navigationHandled=useRef(0);
  const [navigationError,setNavigationError]=useState("");
  useEffect(()=>{
    if(!request || creating || editingTask || request.id<=navigationHandled.current)return;
    let cancelled=false;
    if(request.target.view==="settings"){
      navigationHandled.current=request.id;setNavigationError("");setView({kind:"settings"});clearRequest(request.id);
    }else{
      const taskId=request.target.task_id;
      void api.getAppState().then(latest=>{
        if(cancelled)return;
        const task=latest.tasks.find(task=>task.id===taskId);
        navigationHandled.current=request.id;clearRequest(request.id);
        if(!task){setNavigationError(tr("该任务已不存在，请刷新任务列表。"));return;}
        setNavigationError("");setView({kind:"all"});setFilter(DEFAULT_FILTER);setCreating(false);setEditingTask(task);
      }).catch(e=>{if(!cancelled){navigationHandled.current=request.id;clearRequest(request.id);setNavigationError(String(e));}});
    }
    return()=>{cancelled=true;};
  },[request,creating,editingTask]);

  // 视图过滤/排序按分钟粒度重算（跨日边界正确），不随每秒 tick 重算
  const nowMinute = Math.floor(now / 60000);
  const visibleTasks = useMemo(() => {
    let tasks: Task[];
    switch (view.kind) {
      case "today":
        tasks = state.tasks.filter(
          (task) => task.status === "pending" && (isDueToday(task.due_at, now) || isOverdue(task.due_at, now)),
        );
        break;
      case "week":
        tasks = state.tasks.filter((task) => {
          if (task.status !== "pending" || !task.due_at) return false;
          const due = new Date(task.due_at).getTime();
          return due > now && due <= now + 7 * 24 * 3600 * 1000;
        });
        break;
      case "overdue":
        tasks = state.tasks.filter((task) => task.status === "pending" && isOverdue(task.due_at, now));
        break;
      case "completed":
        tasks = state.tasks.filter((task) => task.status === "completed");
        break;
      case "category":
        tasks = state.tasks.filter((task) => task.category_id === view.id);
        break;
      default:
        tasks = state.tasks;
    }
    return sortTasks(
      applyTaskFilter(tasks, filter),
      sortKey,
      now,
    );
    // eslint 未启用；now 通过 nowMinute 参与依赖
  }, [state.tasks, view, filter, sortKey, nowMinute]);

  const navItems: { key: string; icon: typeof ListTodo; label: string; count?: number; active: boolean; onClick: () => void }[] = [
    {
      key: "all",
      icon: ListTodo,
      label: t.nav.all,
      count: state.tasks.filter((task) => task.status === "pending").length,
      active: view.kind === "all",
      onClick: () => setView({ kind: "all" }),
    },
    {
      key: "today",
      icon: CalendarDays,
      label: t.nav.today,
      count: state.tasks.filter((task) => task.status === "pending" && (isDueToday(task.due_at, now) || isOverdue(task.due_at, now))).length,
      active: view.kind === "today",
      onClick: () => setView({ kind: "today" }),
    },
    {
      key: "week",
      icon: Clock3,
      label: t.nav.week,
      active: view.kind === "week",
      onClick: () => setView({ kind: "week" }),
    },
    {
      key: "overdue",
      icon: AlarmClockOff,
      label: t.nav.overdue,
      count: state.tasks.filter((task) => task.status === "pending" && isOverdue(task.due_at, now)).length,
      active: view.kind === "overdue",
      onClick: () => setView({ kind: "overdue" }),
    },
    {
      key: "completed",
      icon: CheckCircle2,
      label: t.nav.completed,
      active: view.kind === "completed",
      onClick: () => setView({ kind: "completed" }),
    },
  ];

  const viewTitle =
    view.kind === "category"
      ? state.categories.find((c) => c.id === view.id)?.name ?? t.nav.all
      : view.kind === "settings"
        ? t.settings.title
        : view.kind === "categories"
          ? t.nav.manageCategories
          : navItems.find((n) => n.key === view.kind)?.label ?? t.nav.all;

  return (
    <div className="glass-panel flex h-full w-full overflow-hidden rounded-none">
      {/* 左侧导航 */}
      <aside className="flex w-[176px] shrink-0 flex-col border-r border-[var(--glass-border)] bg-[var(--surface-overlay)]">
        <div className="flex h-[52px] items-center gap-2 px-4">
          <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-gradient-to-br from-[#6366f1] to-[#8b5cf6] text-white">
            <ListTodo size={15} strokeWidth={2.4} />
          </div>
          <span className="text-[13px] font-semibold">CheckIt</span>
        </div>

        <nav className="min-h-0 flex-1 overflow-y-auto px-2 py-1">
          {navItems.map(({ key, icon: Icon, label, count, active, onClick }) => (
            <button
              key={key}
              type="button"
              onClick={onClick}
              className={`mb-0.5 flex w-full items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-left text-[12.5px] transition-colors ${
                active
                  ? "bg-[var(--accent-soft)] font-medium text-[var(--accent-text)]"
                  : "text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
              }`}
            >
              <Icon size={14} />
              <span className="min-w-0 flex-1 truncate">{label}</span>
              {count !== undefined && count > 0 && (
                <span className="rounded-full bg-[var(--surface-active)] px-1.5 text-[11px] tabular-nums text-[var(--text-secondary)]">
                  {count}
                </span>
              )}
            </button>
          ))}

          <div className="mb-1 mt-3 flex items-center justify-between px-2.5">
            <span className="text-[11px] font-medium uppercase tracking-wider text-[var(--text-secondary)]">
              {t.nav.categories}
            </span>
            <button
              type="button"
              className="text-[11px] text-[var(--accent-text)] hover:underline"
              onClick={() => setView({ kind: "categories" })}
            >
              {t.nav.manageCategories}
            </button>
          </div>
          {state.categories.map((c) => {
            const count = state.tasks.filter((task) => task.status === "pending" && task.category_id === c.id).length;
            const active = view.kind === "category" && view.id === c.id;
            return (
              <button
                key={c.id}
                type="button"
                onClick={() => setView({ kind: "category", id: c.id })}
                className={`mb-0.5 flex w-full items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-left text-[12.5px] transition-colors ${
                  active
                    ? "bg-[var(--accent-soft)] font-medium text-[var(--accent-text)]"
                    : "text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
                }`}
              >
                <span className="h-2 w-2 shrink-0 rounded-full" style={{ backgroundColor: c.color }} />
                <span className="min-w-0 flex-1 truncate">{c.name}</span>
                {count > 0 && (
                  <span className="text-[11px] tabular-nums text-[var(--text-secondary)]">{count}</span>
                )}
              </button>
            );
          })}
        </nav>

        <button type="button" onClick={()=>setView({kind:"trash"})} className={`mx-2 flex items-center gap-2 rounded-lg px-3 py-2 text-left text-xs ${view.kind==="trash"?"bg-[var(--surface-active)] text-[var(--text-primary)]":"text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]"}`}><Trash2 size={14}/>{tr("最近删除")}</button>
        <button
          type="button"
          onClick={() => setView({ kind: "settings" })}
          className={`m-2 flex items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-left text-[12.5px] transition-colors ${
            view.kind === "settings"
              ? "bg-[var(--accent-soft)] font-medium text-[var(--accent-text)]"
              : "text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
          }`}
        >
          <SettingsIcon size={14} />
          {t.nav.settings}
        </button>
      </aside>

      {/* 主内容区 */}
      <main className="flex min-w-0 flex-1 flex-col">
        {navigationError && <div role="alert" className="p-3 text-xs text-[var(--urgent-text)]">{navigationError}</div>}
        {view.kind === "trash" ? <TrashPanel/> : view.kind === "settings" ? (
          <SettingsPanel />
        ) : view.kind === "categories" ? (
          <CategoryManager />
        ) : (
          <TaskListView
            title={viewTitle}
            tasks={visibleTasks}
            now={now}
            filter={filter}
            onFilterChange={setFilter}
            sortKey={sortKey}
            onSortChange={setSortKey}
            onEditTask={setEditingTask}
            onCreate={() => setCreating(true)}
          />
        )}
      </main>

      {(creating || editingTask) && (
        <TaskFormModal
          task={editingTask}
          notice={request ? <div role="status" className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-[var(--glass-border)] p-3 text-xs">
            <span>{tr("请先完成或关闭当前编辑，随后将打开请求的页面。")}</span>
            <button type="button" onClick={()=>clearRequest(request.id)}>{tr("取消跳转")}</button>
          </div> : undefined}
          onClose={() => {
            setCreating(false);
            setEditingTask(null);
          }}
        />
      )}
    </div>
  );
}
