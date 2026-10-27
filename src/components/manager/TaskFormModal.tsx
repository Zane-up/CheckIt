/**
 * 任务新建 / 编辑表单：标题、描述、分类、优先级、截止时间、多级提醒。
 *
 * 截止时间处理（关键规则）：
 * - 进入表单时把 UTC 转为 datetime-local（分钟精度）仅用于展示；
 * - 保存时若输入值与进入时一致，则**原样传回任务的原始 UTC 值**——
 *   仅编辑标题/描述不会丢失秒数，也不会在夏令时重复时段偏移一小时；
 * - 只有用户真正修改了输入值，才按本地时区重新解析为 UTC。
 *   （datetime-local 无法表达重复时段的歧义，属于平台已知限制，见 docs/DECISIONS.md）
 */

import { useRef, useState } from "react";
import { Trash2 } from "lucide-react";
import { useAppData } from "@/hooks/useAppData";
import { t } from "@/i18n/zh";
import type { Priority, Task, TaskInput } from "@/types";
import { localInputToUtcIso, utcIsoToLocalInput } from "@/utils/time";
import { Modal } from "@/components/ui/Modal";
import { Button } from "@/components/ui/Buttons";
import { ReminderPicker } from "@/components/ui/ReminderPicker";

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
  /** 自定义提醒输入框存在未添加内容（保存前须先添加或清空） */
  const [reminderDraft, setReminderDraft] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  /** 进入表单时的输入快照：保存时据此判断截止时间是否被真正修改 */
  const initialDueInput = utcIsoToLocalInput(task?.due_at ?? null);
  /** 在途保护：Enter / 按钮 / 保存快捷键重复触发只执行一次 */
  const inFlight = useRef(false);

  const hasDue = dueInput !== "";
  const dueChanged = dueInput !== initialDueInput;

  const save = async () => {
    if (inFlight.current) return;
    const name = title.trim();
    if (!name) {
      setError(t.errors.titleRequired);
      return;
    }
    if (reminderDraft) {
      setError(t.errors.reminderDraftPending);
      return;
    }
    inFlight.current = true;
    setSaving(true);
    // 截止未动 → 传回原始 UTC（保秒、防 DST 偏移）；动了 → 按本地重新解析
    const dueAt = dueChanged ? localInputToUtcIso(dueInput) : task?.due_at ?? null;
    const input: TaskInput = {
      title: name,
      description: description.trim(),
      category_id: categoryId,
      priority,
      due_at: dueAt,
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
      inFlight.current = false;
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

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.nativeEvent.isComposing || e.keyCode === 229) return;
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void save();
    }
  };

  return (
    <Modal open title={task ? t.task.edit : t.task.create} onClose={onClose} width={520}>
      <div className="flex flex-col gap-3.5">
        <div>
          <label className="mb-1 block text-[12px] font-medium text-[var(--text-secondary)]">{t.task.title} *</label>
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
            onKeyDown={onKeyDown}
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
                      ? "border-[var(--accent)] bg-[var(--accent-soft)] font-medium text-[var(--accent-text)]"
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
              <button type="button" className="text-[12px] font-normal text-[var(--text-secondary)] hover:underline" onClick={() => setDueInput("")}>
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
            {!hasDue && <span className="ml-1.5 font-normal text-[var(--text-muted)]">（{t.task.reminderRequireDue}）</span>}
          </label>
          <ReminderPicker
            offsets={reminders}
            onChange={setReminders}
            disabled={!hasDue}
            onDraftChange={setReminderDraft}
          />
        </div>

        {error && <div className="text-[12px] text-[var(--urgent-text)]">{error}</div>}

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
