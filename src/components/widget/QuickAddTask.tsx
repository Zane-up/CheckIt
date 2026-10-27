/**
 * 快速添加任务：悬浮窗内联面板。
 * - 截止预设用本地日历语义（今天/明天在深夜调用也不会跨错日期）
 * - 单一提交入口 + 在途保护：Enter/按钮/回车重复触发不会重复创建
 * - 过滤输入法组合中的 Enter（确认候选词不提交）
 */

import { CalendarClock, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useAppData } from "@/hooks/useAppData";
import { t } from "@/i18n/zh";
import {
  calendarTodayAt,
  calendarTomorrowAt,
  localInputToUtcIso,
} from "@/utils/time";
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

/** 今天 21:00，已过则今天 23:59（仍在"今天"日历日内，不滚到明天） */
function presetToday(now: number): string {
  const evening = calendarTodayAt(now, 21, 0);
  if (evening > now) return toLocalInput(evening);
  return toLocalInput(calendarTodayAt(now, 23, 59));
}

/** 明天 09:00（日历法的"明天"，与当前时刻无关） */
function presetTomorrow(now: number): string {
  return toLocalInput(calendarTomorrowAt(now, 9, 0));
}

export default function QuickAddTask({ onClose, defaultOffsets }: QuickAddTaskProps) {
  const { createTask } = useAppData();
  const [title, setTitle] = useState("");
  const [dueInput, setDueInput] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const inFlight = useRef(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const save = async () => {
    // 在途保护：上一次提交未完成前忽略重复触发（Enter 连击/双击按钮）
    if (inFlight.current) return;
    const name = title.trim();
    if (!name) {
      setError(t.errors.titleRequired);
      return;
    }
    inFlight.current = true;
    setSubmitting(true);
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
    } finally {
      inFlight.current = false;
      setSubmitting(false);
    }
  };

  /** 统一的键盘处理：过滤输入法组合键（确认候选词的 Enter 不算提交） */
  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.nativeEvent.isComposing || e.keyCode === 229) return;
    if (e.key === "Enter") {
      e.preventDefault();
      void save();
    } else if (e.key === "Escape") {
      onClose();
    }
  };

  const now = Date.now();

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
          onKeyDown={onKeyDown}
        />
        <button
          type="button"
          onClick={onClose}
          className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
          title={t.task.cancel}
        >
          <X size={14} />
        </button>
      </div>

      <div className="mt-2 flex items-center gap-1.5">
        <button type="button" className="glass-subtle rounded-lg px-2 py-0.5 text-[12px] text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]" onClick={() => setDueInput(presetToday(now))}>
          {t.quickAdd.today}
        </button>
        <button type="button" className="glass-subtle rounded-lg px-2 py-0.5 text-[12px] text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]" onClick={() => setDueInput(presetTomorrow(now))}>
          {t.quickAdd.tomorrow}
        </button>
        <div className="relative flex min-w-0 flex-1 items-center">
          <CalendarClock size={13} className="pointer-events-none absolute left-2 text-[var(--text-muted)]" />
          <input
            type="datetime-local"
            className="field-input !py-0.5 pl-7 text-[12px]"
            value={dueInput}
            onChange={(e) => setDueInput(e.target.value)}
          />
        </div>
        <Button variant="primary" size="sm" disabled={submitting} onClick={() => void save()}>
          {t.quickAdd.save}
        </Button>
      </div>

      {error && <div className="mt-1.5 text-[12px] text-[var(--urgent-text)]">{error}</div>}
    </div>
  );
}
