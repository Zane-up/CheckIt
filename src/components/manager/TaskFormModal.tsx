/**
 * 任务新建 / 编辑表单：标题、描述、分类、优先级、截止时间、多级提醒。
 */

import { useEffect, useState } from "react";
import { Trash2 } from "lucide-react";
import { useAppData } from "@/hooks/useAppData";
import { t } from "@/i18n/zh";
import type { Priority, Task, TaskInput } from "@/types";
import { CUSTOM_REMINDER_MINUTES, REMINDER_PRESETS, normalizeOffsets, offsetLabel } from "@/utils/reminders";
import { localInputToUtcIso, utcIsoToLocalInput } from "@/utils/time";
import { Modal } from "@/components/ui/Modal";
import { Button } from "@/components/ui/Buttons";

interface TaskFormModalProps {
  /** null = 新建 */
  task: Task | null;
  onClose: () => void;
}

const PRIORITIES: Priority[] = ["high", "medium", "low"];

export default function TaskFormModal({ task, onClose }: TaskFormModalProps) {
  const { state, createTask, updateTask, deleteTask } = useAppData();
  const [title, setTitle] = useState(task?.title ?? "");
  const [description, setDescription] = useState(task?.description ?? "");
  const [categoryId, setCategoryId] = useState<number | null>(task?.category_id ?? null);
  const [priority, setPriority] = useState<Priority>(task?.priority ?? "medium");
  const [dueInput, setDueInput] = useState(utcIsoToLocalInput(task?.due_at ?? null));
  const [reminders, setReminders] = useState<number[]>(task?.reminders ?? state.settings.default_reminder_offsets);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  // 编辑已有任务时按需重置（Modal 每次打开是新实例，这里主要防御性处理）
  useEffect(() => {
    setTitle(task?.title ?? "");
    setDescription(task?.description ?? "");
  }, [task]);

  const hasDue = dueInput !== "";

  const toggleReminder = (offset: number) => {
    setReminders((rs) => (rs.includes(offset) ? rs.filter((r) => r !== offset) : normalizeOffsets([...rs, offset])));
  };

  const save = async () => {
    const name = title.trim();
    if (!name) {
      setError(t.errors.titleRequired);
      return;
    }
    setSaving(true);
    const input: TaskInput = {
      title: name,
      description: description.trim(),
      category_id: categoryId,
      priority,
      due_at: localInputToUtcIso(dueInput),
      reminders: hasDue ? reminders : [],
    };
    try {
      if (task) {
        await updateTask(task.id, input);
      } else {
        await createTask(input);
      }
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : t.errors.saveFailed);
    } finally {
      setSaving(false);
    }
  };

  const remove = async () => {
    if (!task) return;
    if (!window.confirm(t.task.deleteConfirm)) return;
    try {
      await deleteTask(task.id);
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : t.errors.deleteFailed);
    }
  };

  return (
    <Modal open title={task ? t.task.edit : t.task.create} onClose={onClose} width={520}>
      <div className="flex flex-col gap-3.5">
        <div>
          <label className="mb-1 block text-xs font-medium text-[var(--text-secondary)]">{t.task.title} *</label>
          <input
            className="field-input"
            value={title}
            maxLength={200}
            autoFocus
            placeholder={t.task.titlePlaceholder}
            onChange={(e) => {
              setTitle(e.target.value);
              setError(null);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) void save();
            }}
          />
        </div>

        <div>
          <label className="mb-1 block text-xs font-medium text-[var(--text-secondary)]">{t.task.description}</label>
          <textarea
            className="field-input min-h-[64px] resize-y"
            value={description}
            maxLength={2000}
            placeholder={t.task.descriptionPlaceholder}
            onChange={(e) => setDescription(e.target.value)}
          />
        </div>

        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="mb-1 block text-xs font-medium text-[var(--text-secondary)]">{t.task.category}</label>
            <select
              className="field-input"
              value={categoryId ?? ""}
              onChange={(e) => setCategoryId(e.target.value ? Number(e.target.value) : null)}
            >
              <option value="">{t.task.uncategorized}</option>
              {state.categories.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </select>
          </div>
          <div>
            <label className="mb-1 block text-xs font-medium text-[var(--text-secondary)]">{t.task.priority}</label>
            <div className="flex gap-1">
              {PRIORITIES.map((p) => (
                <button
                  key={p}
                  type="button"
                  onClick={() => setPriority(p)}
                  className={`flex-1 rounded-lg border px-2 py-1.5 text-xs transition-all ${
                    priority === p
                      ? "border-[var(--accent)] bg-[var(--accent-soft)] font-medium text-[var(--accent)]"
                      : "border-[var(--glass-border)] text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]"
                  }`}
                >
                  {t.task[`priority${p[0].toUpperCase()}${p.slice(1)}` as "priorityHigh" | "priorityMedium" | "priorityLow"]}
                </button>
              ))}
            </div>
          </div>
        </div>

        <div>
          <label className="mb-1 flex items-center justify-between text-xs font-medium text-[var(--text-secondary)]">
            {t.task.dueAt}
            {hasDue && (
              <button type="button" className="text-[11px] text-[var(--text-muted)] hover:underline" onClick={() => setDueInput("")}>
                {t.task.clearDue}
              </button>
            )}
          </label>
          <input
            type="datetime-local"
            className="field-input"
            value={dueInput}
            onChange={(e) => setDueInput(e.target.value)}
          />
        </div>

        <div>
          <label className="mb-1.5 block text-xs font-medium text-[var(--text-secondary)]">
            {t.task.reminders}
            {!hasDue && <span className="ml-1.5 font-normal text-[var(--text-muted)]">（需先设置截止时间）</span>}
          </label>
          <div className={`flex flex-wrap gap-1.5 ${hasDue ? "" : "pointer-events-none opacity-40"}`}>
            {REMINDER_PRESETS.map(({ offsetSeconds, label }) => (
              <button
                key={offsetSeconds}
                type="button"
                onClick={() => toggleReminder(offsetSeconds)}
                className={`rounded-full border px-2.5 py-1 text-[11px] transition-all ${
                  reminders.includes(offsetSeconds)
                    ? "border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]"
                    : "border-[var(--glass-border)] text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]"
                }`}
              >
                {label}
              </button>
            ))}
            {CUSTOM_REMINDER_MINUTES.map((m) => {
              const offset = m * 60;
              if (REMINDER_PRESETS.some((p) => p.offsetSeconds === offset)) return null;
              return (
                <button
                  key={m}
                  type="button"
                  onClick={() => toggleReminder(offset)}
                  className={`rounded-full border px-2.5 py-1 text-[11px] transition-all ${
                    reminders.includes(offset)
                      ? "border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]"
                      : "border-[var(--glass-border)] text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]"
                  }`}
                >
                  {offsetLabel(offset)}
                </button>
              );
            })}
          </div>
        </div>

        {error && <div className="text-xs text-[var(--urgent)]">{error}</div>}

        <div className="mt-1 flex items-center justify-between border-t border-[var(--glass-border)] pt-3">
          <div>
            {task && (
              <Button variant="danger" size="sm" onClick={() => void remove()}>
                <Trash2 size={13} />
                {t.task.delete}
              </Button>
            )}
          </div>
          <div className="flex gap-2">
            <Button onClick={onClose}>{t.task.cancel}</Button>
            <Button variant="primary" disabled={saving} onClick={() => void save()}>
              {t.task.save}
            </Button>
          </div>
        </div>
      </div>
    </Modal>
  );
}
