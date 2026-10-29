import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { BellRing, X } from "lucide-react";
import { api, type PopupSnapshot } from "@/services/api";
import { useNow } from "@/hooks/useNow";
import { computeCountdown } from "@/utils/countdown";

/** 独立原生窗口，后台队列是唯一数据源；事件、首屏读取和轮询共同防止丢提醒。 */
export default function ReminderApp() {
  const now = useNow();
  const [snapshot, setSnapshot] = useState<PopupSnapshot>({ reminders: [], preview: false });
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const sequence = useRef(0);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const reload = async () => {
      const request = ++sequence.current;
      try {
        const next = await api.getPopupReminders();
        if (!disposed && request === sequence.current) {
          setSnapshot(next);
          setError(null);
        }
      } catch (e) {
        if (!disposed && request === sequence.current) setError(String(e));
      }
    };
    void listen("popup-reminders-changed", () => void reload()).then((stop) => {
      if (disposed) stop();
      else { unlisten = stop; void reload(); }
    }).catch(() => { if (!disposed) void reload(); });
    void reload();
    const timer = window.setInterval(() => void reload(), 3000);
    return () => { disposed = true; unlisten?.(); window.clearInterval(timer); };
  }, []);

  const dismiss = useCallback(async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      // 只确认当前展示的提醒，点击期间新到达的消息会留在窗口里。
      await api.dismissPopupReminders(snapshot.reminders.map((r) => r.id));
    } catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  }, [busy, snapshot]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => { if (event.key === "Escape") void dismiss(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [dismiss]);

  return (
    <main role="dialog" aria-modal="true" aria-labelledby="reminder-title"
      className="flex h-full flex-col overflow-hidden rounded-2xl border border-[var(--glass-border)] bg-[var(--surface-overlay)] text-[var(--text-primary)]">
      <header className="flex items-center gap-3 border-b border-[var(--glass-border)] px-5 py-4">
        <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-[var(--accent-soft)] text-[var(--accent-text)]"><BellRing size={21} /></div>
        <div className="min-w-0 flex-1">
          <div className="text-[11px] font-medium tracking-wide text-[var(--text-secondary)]">CheckIt · 任务提醒</div>
          <h1 id="reminder-title" className="text-base font-semibold">该关注一下你的任务了</h1>
        </div>
        <button type="button" aria-label="关闭提醒" disabled={busy} onClick={() => void dismiss()}
          className="rounded-lg p-1.5 text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] disabled:opacity-50"><X size={18} /></button>
      </header>
      <div className="sr-only" role="status">{snapshot.reminders.length} 条任务提醒</div>
      <div className="min-h-0 flex-1 space-y-2 overflow-y-auto px-5 py-4">
        {snapshot.preview && (
          <article className="rounded-xl border border-[var(--glass-border)] bg-[var(--surface-card)] p-3">
            <h2 className="text-sm font-semibold">这是你的提醒弹窗</h2>
            <p className="mt-1 text-xs leading-relaxed text-[var(--text-secondary)]">任务到达提醒时间时会在这里显示。此预览不会创建任务或修改提醒记录。</p>
          </article>
        )}
        {snapshot.reminders.map((item) => {
          const countdown = computeCountdown(item.due_at, now);
          return (
          <article key={item.id} className="rounded-xl border border-[var(--glass-border)] bg-[var(--surface-card)] p-3">
            <h2 className="break-words text-sm font-semibold">{item.title}</h2>
            <p className="mt-1 whitespace-pre-wrap break-words text-xs leading-relaxed text-[var(--text-secondary)]">
              {countdown ? countdown.overdue ? countdown.text : `距离截止还有 ${countdown.text}` : item.body}
            </p>
            <p className="mt-1 text-xs text-[var(--text-muted)]">截止时间：{new Date(item.due_at).toLocaleString("zh-CN", { hour12: false })}</p>
          </article>
          );
        })}
        {error && <p role="alert" className="break-words text-xs text-[var(--urgent-text)]">提醒读取或确认失败：{error}</p>}
        {!snapshot.preview && snapshot.reminders.length === 0 && !error && <p className="text-sm text-[var(--text-secondary)]">正在读取提醒…</p>}
      </div>
      <footer className="flex shrink-0 items-center justify-between gap-3 border-t border-[var(--glass-border)] px-5 py-3">
        <button type="button" disabled={busy} className="text-xs text-[var(--accent-text)] hover:underline"
          onClick={() => void api.openManager().catch((e) => setError(String(e)))}>查看任务</button>
        <button type="button" disabled={busy} onClick={() => void dismiss()}
          className="rounded-xl bg-[var(--btn-primary-bg)] px-5 py-2 text-sm font-medium text-[var(--btn-primary-text)] hover:bg-[var(--btn-primary-hover)] disabled:opacity-50">
          {busy ? "确认中…" : snapshot.reminders.length > 1 ? `全部知道了（${snapshot.reminders.length}）` : "知道了"}
        </button>
      </footer>
    </main>
  );
}
