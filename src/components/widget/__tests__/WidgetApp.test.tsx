/**
 * 悬浮窗回归：
 * - Window 包装对象只获取一次，时钟 tick 不重注册 tray 快速添加监听
 * - 收起后从托盘"添加任务"仍能恢复展开尺寸并打开快速添加
 */

import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => {
  const handlers = new Map<string, (event: unknown) => void>();
  const listenCalls: string[] = [];
  let inner = { width: 340, height: 440 };

  const windowMock = {
    scaleFactor: vi.fn(async () => 1),
    innerSize: vi.fn(async () => ({ ...inner })),
    setMinSize: vi.fn(async (_size: { width: number; height: number }) => {}),
    setSize: vi.fn(async (size: { width: number; height: number }) => {
      inner = { width: size.width, height: size.height };
    }),
    setAlwaysOnTop: vi.fn(async () => {}),
  };
  const getCurrentWindow = vi.fn(() => windowMock);
  const listen = vi.fn(
    async (event: string, handler: (payload: unknown) => void) => {
      listenCalls.push(event);
      handlers.set(event, handler);
      return () => {
        handlers.delete(event);
      };
    },
  );
  return {
    handlers,
    listenCalls,
    windowMock,
    getCurrentWindow,
    listen,
    clock: { now: new Date(2026, 9, 26, 12, 0).getTime() },
    resetGeometry() {
      inner = { width: 340, height: 440 };
    },
  };
});

vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: mocks.getCurrentWindow }));
vi.mock("@/hooks/useNow", () => ({ useNow: () => mocks.clock.now }));
vi.mock("@/hooks/useAppData", () => ({
  useAppData: () => ({
    state: {
      tasks: [],
      categories: [],
      settings: {
        theme: "dark",
        glass_enabled: true,
        glass_opacity: 0.35,
        always_on_top: true,
        close_to_tray: true,
        notifications_enabled: true,
        default_reminder_offsets: [3600, 600],
        widget_display_count: 8,
        launch_at_login: false,
      },
    },
    createTask: vi.fn(async () => {}),
    setTaskStatus: vi.fn(async () => {}),
  }),
}));

import WidgetApp from "@/components/widget/WidgetApp";
import { t } from "@/i18n/zh";

const quickAddRegistrations = () =>
  mocks.listenCalls.filter((event) => event === "widget-quick-add").length;

beforeEach(() => {
  mocks.handlers.clear();
  mocks.listenCalls.length = 0;
  mocks.getCurrentWindow.mockClear();
  mocks.windowMock.scaleFactor.mockClear();
  mocks.windowMock.innerSize.mockClear();
  mocks.windowMock.setMinSize.mockClear();
  mocks.windowMock.setSize.mockClear();
  mocks.clock.now = new Date(2026, 9, 26, 12, 0).getTime();
  mocks.resetGeometry();
});

describe("WidgetApp 窗口与快速添加", () => {
  it("时钟 tick 重渲染不重新获取 Window、也不重注册 widget-quick-add", () => {
    const { rerender } = render(<WidgetApp />);
    expect(mocks.getCurrentWindow).toHaveBeenCalledTimes(1);
    expect(quickAddRegistrations()).toBe(1);

    mocks.clock.now += 1000;
    rerender(<WidgetApp />);
    mocks.clock.now += 1000;
    rerender(<WidgetApp />);

    expect(mocks.getCurrentWindow).toHaveBeenCalledTimes(1);
    expect(quickAddRegistrations()).toBe(1);
  });

  it("收起后托盘快速添加：恢复展开高度并显示面板", async () => {
    render(<WidgetApp />);

    // 收起：窗口高度变为 64
    fireEvent.click(screen.getByTitle(t.widget.collapse));
    await waitFor(() => {
      const calls = mocks.windowMock.setSize.mock.calls;
      expect(calls[calls.length - 1][0].height).toBe(64);
    });
    // 收起事件不应影响监听器注册数量
    expect(quickAddRegistrations()).toBe(1);

    // 托盘"添加任务"事件：恢复到收起前高度（440），而不是停在 64
    await act(async () => {
      mocks.handlers.get("widget-quick-add")!({});
    });
    await waitFor(() => {
      const calls = mocks.windowMock.setSize.mock.calls;
      const size = calls[calls.length - 1][0];
      expect(size.width).toBe(340);
      expect(size.height).toBe(440);
    });
    const minCalls = mocks.windowMock.setMinSize.mock.calls;
    expect(minCalls[minCalls.length - 1][0].height).toBe(200);
    expect(screen.getByPlaceholderText(t.quickAdd.placeholder)).toBeTruthy();
  });
});
