/**
 * 提醒提前量选择器：预设标签 + 自定义输入行（数值 + 单位 + 添加）。
 *
 * 供任务新建/编辑表单与设置页"默认提醒"共用：
 * - 标签统一按提前量从大到小排列（与预设顺序一致），位置稳定；
 * - 相同秒数只保留一份：60 分钟 = 1 小时 = 3600 秒；
 *   与预设重复时不生成重复标签，直接显示该预设已选中；
 * - 自定义校验失败就地提示并保留输入，不静默丢弃；
 * - 输入框 Enter 只添加自定义提醒（过滤输入法组合态），不触发外层保存；
 * - onDraftChange 向外层同步"是否存在未添加的输入"，供保存前拦截。
 */

import { useEffect, useRef, useState } from "react";
import { X } from "lucide-react";
import { t } from "@/i18n/zh";
import {
  REMINDER_PRESETS,
  normalizeOffsets,
  offsetLabel,
  parseReminderAmount,
  sortOffsetsForDisplay,
  type ReminderParseError,
  type ReminderUnit,
} from "@/utils/reminders";
import { Button } from "@/components/ui/Buttons";

const UNIT_OPTIONS: { value: ReminderUnit; label: string }[] = [
  { value: "minute", label: t.task.reminderUnitMinute },
  { value: "hour", label: t.task.reminderUnitHour },
  { value: "day", label: t.task.reminderUnitDay },
];

const ERROR_TEXT: Record<ReminderParseError, string> = {
  required: t.errors.reminderAmountRequired,
  invalid: t.errors.reminderAmountInvalid,
  zero: t.errors.reminderAmountZero,
  tooLarge: t.errors.reminderAmountTooLarge,
};

interface ReminderPickerProps {
  /** 当前选中的提前秒数集合（受控；0 = 截止时） */
  offsets: number[];
  onChange: (offsets: number[]) => void;
  /** 禁用全部交互（如任务未设置截止时间）；禁用时自定义输入会被清空 */
  disabled?: boolean;
  /** 禁用原因说明（就地展示） */
  disabledHint?: string;
  /** 自定义输入是否存在未添加内容；保存前外层据此阻止并提示 */
  onDraftChange?: (hasDraft: boolean) => void;
}

export function ReminderPicker({
  offsets,
  onChange,
  disabled = false,
  disabledHint,
  onDraftChange,
}: ReminderPickerProps) {
  const [amount, setAmount] = useState("");
  const [unit, setUnit] = useState<ReminderUnit>("minute");
  const [error, setError] = useState<ReminderParseError | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // 禁用（如清除截止时间）时清掉草稿，避免残留输入被静默丢弃的歧义
  useEffect(() => {
    if (disabled && amount !== "") {
      setAmount("");
      setError(null);
      onDraftChange?.(false);
    }
  }, [disabled, amount, onDraftChange]);

  const setDraft = (v: string) => {
    setAmount(v);
    onDraftChange?.(v.trim() !== "");
  };

  const addCustom = () => {
    const result = parseReminderAmount(amount, unit);
    if (!result.ok) {
      setError(result.error);
      return;
    }
    setError(null);
    // 与已选（含预设）重复时不重复添加，标签区已显示选中状态
    if (!offsets.includes(result.offsetSeconds)) {
      onChange(normalizeOffsets([...offsets, result.offsetSeconds]));
    }
    setDraft("");
    inputRef.current?.focus();
  };

  const toggle = (offset: number) => {
    onChange(offsets.includes(offset) ? offsets.filter((o) => o !== offset) : normalizeOffsets([...offsets, offset]));
  };

  /** Enter 只添加自定义提醒；输入法组合态（确认候选词）不视为 Enter */
  const onInputKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.nativeEvent.isComposing || e.keyCode === 229) return;
    if (e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      addCustom();
    }
  };

  // 展示集合 = 已选 ∪ 全部预设，按提前量从大到小稳定排列
  const allOffsets = sortOffsetsForDisplay([
    ...new Set([...offsets, ...REMINDER_PRESETS.map((p) => p.offsetSeconds)]),
  ]);

  return (
    <div className="flex flex-col gap-2">
      <div className={`flex flex-wrap gap-1.5 ${disabled ? "opacity-65" : ""}`}>
        {allOffsets.map((offset) => {
          const preset = REMINDER_PRESETS.find((p) => p.offsetSeconds === offset);
          const label = preset?.label ?? offsetLabel(offset);
          if (offsets.includes(offset)) {
            return (
              <span
                key={offset}
                className="inline-flex items-center gap-0.5 rounded-full border border-[var(--accent)] bg-[var(--accent-soft)] px-2.5 py-1 text-[12px] text-[var(--accent-text)]"
              >
                {label}
                <button
                  type="button"
                  aria-label={`${t.task.reminderRemove} ${label}`}
                  title={`${t.task.reminderRemove} ${label}`}
                  disabled={disabled}
                  onClick={() => toggle(offset)}
                  className="-mr-1 flex h-4 w-4 items-center justify-center rounded-full transition-colors hover:bg-[rgba(0,0,0,0.10)] focus-visible:outline-1 focus-visible:outline-[var(--accent)] disabled:pointer-events-none disabled:opacity-50"
                >
                  <X size={11} />
                </button>
              </span>
            );
          }
          return (
            <button
              key={offset}
              type="button"
              disabled={disabled}
              onClick={() => toggle(offset)}
              className="rounded-full border border-[var(--glass-border)] px-2.5 py-1 text-[12px] text-[var(--text-secondary)] transition-all hover:bg-[var(--surface-hover)] focus-visible:outline-1 focus-visible:outline-[var(--accent)] disabled:pointer-events-none disabled:opacity-50"
            >
              {label}
            </button>
          );
        })}
      </div>

      <div className="flex flex-wrap items-center gap-1.5">
        <span className="text-[12px] text-[var(--text-secondary)]">{t.task.reminderCustomLead}</span>
        <input
          ref={inputRef}
          className="field-input !w-[76px] px-2 py-1 text-center text-[12px]"
          value={amount}
          disabled={disabled}
          inputMode="numeric"
          placeholder="25"
          aria-label={t.task.reminderAmountLabel}
          onChange={(e) => {
            setError(null);
            setDraft(e.target.value);
          }}
          onKeyDown={onInputKeyDown}
        />
        <select
          className="field-input !w-auto px-1.5 py-1 text-[12px]"
          value={unit}
          disabled={disabled}
          aria-label={t.task.reminderUnitLabel}
          onChange={(e) => {
            setUnit(e.target.value as ReminderUnit);
            setError(null);
          }}
        >
          {UNIT_OPTIONS.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
        <Button size="sm" disabled={disabled} onClick={addCustom}>
          {t.task.reminderAdd}
        </Button>
      </div>

      {disabled ? (
        disabledHint && <div className="text-[12px] leading-snug text-[var(--text-muted)]">{disabledHint}</div>
      ) : (
        error && <div className="text-[12px] text-[var(--urgent-text)]">{ERROR_TEXT[error]}</div>
      )}
    </div>
  );
}
