/**
 * 分类管理：新建 / 重命名 / 换色 / 删除。
 * 删除分类不删除任务（后端将关联任务置为未分类）。
 */

import { useState } from "react";
import { Check, Pencil, Plus, Trash2, X } from "lucide-react";
import { useAppData } from "@/hooks/useAppData";
import { t } from "@/i18n/zh";
import { Button } from "@/components/ui/Buttons";

const PRESET_COLORS = [
  "#6366f1", "#8b5cf6", "#ec4899", "#f43f5e",
  "#f59e0b", "#10b981", "#06b6d4", "#64748b",
];

interface Draft {
  id: number | null; // null = 新建
  name: string;
  color: string;
}

export default function CategoryManager() {
  const { state, createCategory, updateCategory, deleteCategory } = useAppData();
  const [draft, setDraft] = useState<Draft | null>(null);
  const [error, setError] = useState<string | null>(null);

  const startCreate = () => setDraft({ id: null, name: "", color: PRESET_COLORS[0] });
  const startEdit = (id: number, name: string, color: string) => setDraft({ id, name, color });

  const save = async () => {
    if (!draft) return;
    const name = draft.name.trim();
    if (!name) {
      setError("请输入分类名称");
      return;
    }
    try {
      if (draft.id === null) {
        await createCategory(name, draft.color);
      } else {
        await updateCategory(draft.id, name, draft.color);
      }
      setDraft(null);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : t.errors.saveFailed);
    }
  };

  const remove = async (id: number) => {
    if (!window.confirm(t.category.deleteConfirm + "\n" + t.category.deleteHint)) return;
    try {
      await deleteCategory(id);
    } catch (e) {
      setError(e instanceof Error ? e.message : t.errors.deleteFailed);
    }
  };

  return (
    <div className="mx-auto w-full max-w-xl px-6 py-5">
      <div className="mb-4 flex items-center justify-between">
        <h1 className="text-[15px] font-semibold">{t.nav.manageCategories}</h1>
        <Button variant="primary" size="sm" onClick={startCreate}>
          <Plus size={14} />
          {t.category.add}
        </Button>
      </div>

      {error && <div className="mb-3 text-xs text-[var(--urgent)]">{error}</div>}

      <p className="mb-3 text-[11px] text-[var(--text-muted)]">{t.category.deleteHint}</p>

      <ul className="flex flex-col gap-1.5">
        {state.categories.map((c) => (
          <li key={c.id} className="glass-subtle flex items-center gap-3 rounded-xl px-3 py-2">
            <span className="h-3 w-3 rounded-full" style={{ backgroundColor: c.color }} />
            <span className="min-w-0 flex-1 truncate text-[13px]">{c.name}</span>
            <span className="text-[11px] text-[var(--text-muted)]">
              {state.tasks.filter((task) => task.category_id === c.id && task.status === "pending").length} 项
            </span>
            <button
              type="button"
              className="flex h-6 w-6 items-center justify-center rounded-md text-[var(--text-muted)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
              title={t.category.edit}
              onClick={() => startEdit(c.id, c.name, c.color)}
            >
              <Pencil size={13} />
            </button>
            <button
              type="button"
              className="flex h-6 w-6 items-center justify-center rounded-md text-[var(--text-muted)] hover:bg-[rgba(248,113,113,0.12)] hover:text-[var(--urgent)]"
              title={t.category.delete}
              onClick={() => void remove(c.id)}
            >
              <Trash2 size={13} />
            </button>
          </li>
        ))}
      </ul>

      {draft && (
        <div className="anim-pop glass-panel mt-3 rounded-xl p-3" style={{ backgroundColor: "var(--glass-bg-strong)" }}>
          <div className="mb-2 text-xs font-medium">{draft.id === null ? t.category.add : t.category.edit}</div>
          <div className="flex items-center gap-2">
            <input
              className="field-input flex-1"
              value={draft.name}
              maxLength={30}
              autoFocus
              placeholder={t.category.name}
              onChange={(e) => setDraft({ ...draft, name: e.target.value })}
              onKeyDown={(e) => {
                if (e.key === "Enter") void save();
                if (e.key === "Escape") setDraft(null);
              }}
            />
            <Button variant="primary" size="sm" onClick={() => void save()}>
              <Check size={13} />
              {t.task.save}
            </Button>
            <Button size="sm" onClick={() => setDraft(null)}>
              <X size={13} />
              {t.task.cancel}
            </Button>
          </div>
          <div className="mt-2.5 flex gap-1.5">
            {PRESET_COLORS.map((color) => (
              <button
                key={color}
                type="button"
                onClick={() => setDraft({ ...draft, color })}
                className={`h-5 w-5 rounded-full transition-transform hover:scale-110 ${
                  draft.color === color ? "ring-2 ring-[var(--accent)] ring-offset-2 ring-offset-transparent" : ""
                }`}
                style={{ backgroundColor: color }}
                title={color}
              />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
