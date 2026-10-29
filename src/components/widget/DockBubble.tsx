import { tr, useI18n } from "@/i18n";
/**
 * 贴边收纳球：40 逻辑像素的圆形窗口，显示全部 pending 任务数。
 *
 * 关键设计：
 * - 原生窗口本身就是 40×40（由 Rust `dock_to` / `restore_widget_geometry` 真正缩小），
 *   圆圈之外不存在透明区域拦截鼠标。
 * - 计数统计**全部** status=pending 任务（含逾期），不受筛选与显示条数限制。
 * - 单击 → 命令后端展开完整面板；拖动 → 由 Rust 拖动观察线程判定
 *   （仍在边缘保持圆圈 / 拖离边缘恢复面板），前端不自行展开，避免误展开。
 * - 悬停只显示提示，不自动展开。
 */

import { useCallback, useMemo, useRef } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { Task } from "@/types";
import { formatBubbleCount, bubbleTooltip } from "@/utils/bubble";

/** 圆圈按下后的位移容差（逻辑像素）：超过则视为拖动，不触发展开 */
const DRAG_TOLERANCE = 4;

interface DockBubbleProps {
  tasks: Task[];
  onExpand: () => Promise<void>;
  error?: string | null;
}

export default function DockBubble({ tasks, onExpand, error }: DockBubbleProps) {
  useI18n();
  const pressStart = useRef<{ x: number; y: number } | null>(null);
  const dragged = useRef(false);
  const nativeDragStarted = useRef(false);
  const expanding = useRef(false);

  const pendingCount = useMemo(
    () => tasks.filter((task) => task.status === "pending").length,
    [tasks],
  );
  const label = formatBubbleCount(pendingCount);

  const onPointerDown = useCallback((e: React.PointerEvent) => {
    if (e.button !== 0) return;
    pressStart.current = { x: e.screenX, y: e.screenY };
    dragged.current = false;
    nativeDragStarted.current = false;
    e.currentTarget.setPointerCapture?.(e.pointerId);
  }, []);

  const onPointerMove = useCallback((e: React.PointerEvent) => {
    const start = pressStart.current;
    if (!start) return;
    if (
      Math.abs(e.screenX - start.x) > DRAG_TOLERANCE ||
      Math.abs(e.screenY - start.y) > DRAG_TOLERANCE
    ) {
      dragged.current = true;
      // 先识别拖动，再交给原生窗口。原生接管后不保证继续收到 pointerMove。
      if (!nativeDragStarted.current) {
        nativeDragStarted.current = true;
        void getCurrentWindow().startDragging().catch((error) => {
          console.error("拖动圆圈失败", error);
        });
      }
    }
  }, []);

  const onPointerUp = useCallback(() => {
    pressStart.current = null;
    // 保留到后续 click，防止松手后合成的点击把圆圈展开。
  }, []);

  /**
   * 单击展开。
   *
   * 位移超过容差的鼠标手势只拖动。键盘激活（detail=0）始终可以展开。
   */
  const onClick = useCallback((event: React.MouseEvent) => {
    if (event.detail > 0 && dragged.current) {
      return;
    }
    if (expanding.current) return;
    expanding.current = true;
    void onExpand().finally(() => {
      expanding.current = false;
    });
  }, [onExpand]);

  return (
    <button
      type="button"
      data-testid="dock-bubble"
      title={error ? tr("{0} 点击重试。", error) : bubbleTooltip(pendingCount)}
      aria-label={bubbleTooltip(pendingCount)}
      onClick={onClick}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
      data-tauri-drag-region="false"
      className="dock-bubble flex h-full w-full items-center justify-center rounded-full"
    >
      <span
        className="dock-bubble-count pointer-events-none select-none font-semibold leading-none tabular-nums"
        data-testid="dock-bubble-count"
        data-len={label.length}
      >
        {label}
      </span>
    </button>
  );
}
