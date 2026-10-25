/**
 * 快速添加任务：悬浮窗内联面板。
 * 输入名称 + 选择截止时间（今天/明天/具体时间）+ 默认提醒，回车保存。
 */

import { CalendarClock, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useAppData } from "@/hooks/useAppData";
import { t } from "@/i18n/zh";
import { localInputToUtcIso } from "@/utils/time";
import { Button } from "@/components/ui/Buttons";

interface QuickAddTaskProps {
  onClose: () => void;
  defaultOffsets: number[];
}

function toLocalInput(ts: number): string {
  const d = new Date(ts);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export default function QuickAddTask({ onClose, defaultOffsets }: QuickAddTaskProps) {
  const { createTask } = useAppData();
  const [title, setTitle] = useState("");
  const [dueInput, setDueInput] = useState("");
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const save = async () => {
    const name = title.trim();
    if (!name) {
      setError(t.errors.titleRequired);
      return;
    }
    try {
      await createTask({
        title: name,
        description: "",
        category_id: null,
        priority: "medium",
        due_at: localInputToUtcIso(dueInput),
        reminders: dueInput ? defaultOffsets : [],
      });
      setTitle("");
      setDueInput("");
      setError(null);
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : t.errors.saveFailed);
    }
  };

  const preset = (hoursAhead: number, endOfDay = false) => {
    const base = new Date();
    if (endOfDay) {
      base.setHours(21, 0, 0, 0);
      if (base.getTime() < Date.now()) base.setDate(base.getDate() + 1);
    } else {
      base.setMinutes(0, 0, 0);
      base.setHours(base.getHours() + hoursAhead);
    }
    setDueInput(toLocalInput(base.getTime()));
  };

  return (
    <div className="anim-fade-in mx-3 mb-1.5 shrink-0 rounded-xl border border-[var(--glass-border)] bg-[var(--glass-bg-strong)] p-2.5">
      <div className="flex items-center gap-2">
        <input
          ref={inputRef}
          className="field-input"
          placeholder={t.quickAdd.placeholder}
          value={title}
          maxLength={200}
          onChange={(e) => {
            setTitle(e.target.value);
            setError(null);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") void save();
            if (e.key === "Escape") onClose();
          }}
        />
        <button
          type="button"
          onClick={onClose}
          className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg text-[var(--text-muted)] hover:bg-[var(--surface-hover)]"
          title={t.task.cancel}
        >
          <X size={14} />
        </button>
      </div>

      <div className="mt-2 flex items-center gap-1.5">
        <button type="button" className="glass-subtle rounded-lg px-2 py-0.5 text-[11px] hover:bg-[var(--surface-hover)]" onClick={() => preset(2)}>
          {t.quickAdd.today}
        </button>
        <button type="button" className="glass-subtle rounded-lg px-2 py-0.5 text-[11px] hover:bg-[var(--surface-hover)]" onClick={() => preset(26)}>
          {t.quickAdd.tomorrow}
        </button>
        <div className="relative flex min-w-0 flex-1 items-center">
          <CalendarClock size={13} className="pointer-events-none absolute left-2 text-[var(--text-muted)]" />
          <input
            type="datetime-local"
            className="field-input !py-0.5 pl-7 text-[11px]"
            value={dueInput}
            onChange={(e) => setDueInput(e.target.value)}
          />
        </div>
        <Button variant="primary" size="sm" onClick={() => void save()}>
          {t.quickAdd.save}
        </Button>
      </div>

      {error && <div className="mt-1.5 text-[11px] text-[var(--urgent)]">{error}</div>}
    </div>
  );
}
