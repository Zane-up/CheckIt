import { tr, useI18n } from "@/i18n";
/**
 * 桌面悬浮小组件（主界面）。
 *
 * 三种窗口形态，全部由 Rust 侧 `WidgetGeometry` 权威状态驱动（前端不再用 ref 记高度）：
 * - expanded（完整面板）：正常展开，唯一允许写回"正常几何"的形态
 * - collapsed（标题栏）：原生窗口 64 逻辑像素高
 * - docked（贴边圆圈）：原生窗口 40 逻辑像素圆圈
 *
 * 约定：
 * - 尺寸变化一律通过后端命令完成（widget_expand / widget_collapse /
 *   widget_quick_add_closed），避免前端与 Rust 各自维护一套几何真相。
 * - 快速添加所需的最小高度作为"临时高度"传给后端，不写回正常展开尺寸。
 * - 托盘「添加任务」与第二实例唤醒由后端先恢复到完整面板，再发 widget-quick-add。
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useAppData } from "@/hooks/useAppData";
import { useNow } from "@/hooks/useNow";
import type { WidgetMode } from "@/services/api";
import { api } from "@/services/api";
import WidgetFullPanel, { WidgetCollapsedBar } from "@/components/widget/WidgetPanel";
import DockBubble from "@/components/widget/DockBubble";
import type { QuickAddDraft } from "@/components/widget/QuickAddTask";

/** 快速添加面板完整可见所需的最小窗口高度（逻辑像素） */
export const QUICK_ADD_MIN_H = 320;

export default function WidgetApp() {
  useI18n();
  const { state } = useAppData();
  const now = useNow();
  const [mode, setMode] = useState<WidgetMode>("expanded");
  const [quickAddOpen, setQuickAddOpen] = useState(false);
  const [quickAddDraft, setQuickAddDraft] = useState<QuickAddDraft>({ title: "", dueInput: "" });
  const [windowError, setWindowError] = useState<string | null>(null);
  const modeRef = useRef<WidgetMode>("expanded");
  const modeVersion = useRef(0);
  const requestSeq = useRef(0);
  const queue = useRef<Promise<void>>(Promise.resolve());
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);

  const applyMode = useCallback((next: WidgetMode) => {
    modeVersion.current += 1;
    modeRef.current = next;
    setMode(next);
    if (next !== "expanded") setQuickAddOpen(false);
  }, []);

  // 同一个 WebView 内按用户操作顺序提交，后端转换锁负责跨入口串行化。
  const runOperation = useCallback((kind: "expand" | "collapse" | "close", minHeight?: number) => {
    const seq = ++requestSeq.current;
    const operation = queue.current.then(async () => {
      if (!mounted.current) return;
      const version = modeVersion.current;
      try {
        const result = kind === "collapse" ? await api.widgetCollapse()
          : kind === "close" ? await api.widgetQuickAddClosed()
          : minHeight === undefined ? await api.widgetExpand() : await api.widgetExpand(minHeight);
        if (!mounted.current) return;
        // 较新的后端事件比旧命令响应更权威，不能被异步响应覆盖。
        if (version === modeVersion.current) applyMode(result);
        if (seq === requestSeq.current) {
          setWindowError(null);
          if (modeRef.current === "expanded") {
            setQuickAddOpen(kind === "expand" && minHeight !== undefined);
          }
        }
      } catch (e) {
        console.error("窗口操作失败", e);
        if (mounted.current && seq === requestSeq.current) {
          setWindowError(tr("窗口操作失败，请重试，或通过托盘重新打开窗口。"));
          // 不渲染与原生尺寸不符的假面板；关闭失败则保留添加入口供再次关闭重试。
          if (kind === "close" && modeRef.current === "expanded") setQuickAddOpen(true);
        }
      }
    });
    queue.current = operation;
    return operation;
  }, [applyMode]);

  const expand = useCallback((minHeight?: number) => runOperation("expand", minHeight), [runOperation]);
  const collapse = useCallback(() => runOperation("collapse"), [runOperation]);
  const openQuickAdd = useCallback(() => { void expand(QUICK_ADD_MIN_H); }, [expand]);
  const closeQuickAdd = useCallback(() => {
    setQuickAddOpen(false);
    void runOperation("close");
  }, [runOperation]);

  // 先订阅再取快照；快照返回前的新事件/用户请求会使该快照失效。
  useEffect(() => {
    let destroyed = false;
    let unlisten: (() => void) | undefined;
    const version = modeVersion.current;
    const seq = requestSeq.current;
    void (async () => {
      try {
        const stop = await listen<{ mode: WidgetMode }>("widget-mode", (event) => {
          if (!destroyed && event.payload?.mode) applyMode(event.payload.mode);
        });
        if (destroyed) { stop(); return; }
        unlisten = stop;
      } catch (e) {
        console.error("订阅窗口模式失败", e);
      }
      if (destroyed) return;
      try {
        const result = await api.widgetGetMode();
        if (!destroyed && version === modeVersion.current && seq === requestSeq.current) applyMode(result);
      } catch (e) {
        console.error("读取窗口模式失败", e);
        if (!destroyed) setWindowError(tr("读取窗口模式失败，请通过托盘重新打开窗口。"));
      }
    })();
    return () => { destroyed = true; unlisten?.(); };
  }, [applyMode]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let destroyed = false;
    void (async () => {
      try {
        const stop = await listen("widget-quick-add", () => { if (!destroyed) openQuickAdd(); });
        if (destroyed) stop();
        else unlisten = stop;
      } catch (e) {
        console.error("订阅快速添加失败", e);
      }
    })();
    return () => { destroyed = true; unlisten?.(); };
  }, [openQuickAdd]);

  const tasks = useMemo(() => state.tasks, [state.tasks]);

  if (mode === "docked") {
    return <DockBubble tasks={tasks} onExpand={() => expand()} error={windowError} />;
  }

  if (mode === "collapsed") {
    return (
      <div className="h-full w-full" title={windowError ?? undefined}>
        <WidgetCollapsedBar
          now={now}
          onExpand={() => void expand()}
          onAdd={openQuickAdd}
        />
      </div>
    );
  }

  return (
    <>
      {windowError && (
        <div role="alert" className="fixed inset-x-3 bottom-12 z-50 rounded-lg bg-[var(--surface-overlay)] p-2 text-sm text-[var(--text-primary)] shadow-lg">
          {windowError}
          <button type="button" onClick={() => setWindowError(null)} className="ml-2 underline">{tr("关闭提示")}</button>
        </div>
      )}
      <WidgetFullPanel
        tasks={tasks}
        settings={state.settings}
        now={now}
        quickAddOpen={quickAddOpen}
        quickAddDraft={quickAddDraft}
        onDraftChange={setQuickAddDraft}
        onAdd={openQuickAdd}
        onCollapse={() => void collapse()}
        onCloseQuickAdd={closeQuickAdd}
      />
    </>
  );
}
