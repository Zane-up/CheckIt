/**
 * 悬浮窗回归（三模式）：
 * - Window 包装对象只获取一次，时钟 tick 不重注册监听
 * - 模式由后端权威状态驱动：welcome 读 widgetGetMode、widget-mode 事件同步
 * - 托盘快速添加：收起态也恢复到完整面板并请求临时 320 高度
 * - 快速添加关闭后调用后端清除临时高度（不把 320 当正常高度）
 * - 圆圈模式只渲染圆圈，单击展开；拖动松手不误展开
 * - 监听器卸载时全部释放
 */

import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => {
  const handlers = new Map<string, (event: unknown) => void>();
  const listenCalls: string[] = [];
  let mode: "expanded" | "collapsed" | "docked" = "expanded";
  return {
    handlers,
    listenCalls,
    get mode() {
      return mode;
    },
    setMode(next: "expanded" | "collapsed" | "docked") {
      mode = next;
    },
    listen: vi.fn(async (event: string, handler: (payload: unknown) => void) => {
      listenCalls.push(event);
      handlers.set(event, handler);
      return () => {
        handlers.delete(event);
      };
    }),
    clock: { now: new Date(2026, 9, 26, 12, 0).getTime() },
    tasks: [] as Array<Record<string, unknown>>,
    createTask: vi.fn(async () => {}),
  };
});

const apiMocks = vi.hoisted(() => ({
  widgetGetMode: vi.fn(async () => "expanded" as string),
  widgetExpand: vi.fn(async (_min?: number) => "expanded" as string),
  widgetCollapse: vi.fn(async () => "collapsed" as string),
  widgetQuickAddClosed: vi.fn(async () => "expanded" as string),
}));
const windowMocks = vi.hoisted(() => ({ startDragging: vi.fn(async () => {}) }));

vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => windowMocks }));
vi.mock("@/hooks/useNow", () => ({ useNow: () => mocks.clock.now }));
vi.mock("@/services/api", () => ({ api: apiMocks }));
vi.mock("@/hooks/useAppData", () => ({
  useAppData: () => ({
    state: {
      tasks: mocks.tasks,
      categories: [],
      settings: {
        auto_backup_enabled: true,
        auto_backup_keep: 7,
        language: "zh-CN",
        theme: "dark",
        glass_enabled: true,
        glass_opacity: 0.35,
        always_on_top: true,
        close_to_tray: true,
        notifications_enabled: true,
        popup_reminders_enabled: false,
        default_reminder_offsets: [3600, 600],
        widget_display_count: 8,
        launch_at_login: false,
        edge_dock_enabled: true,
      },
    },
    createTask: mocks.createTask,
    setTaskStatus: vi.fn(async () => {}),
  }),
}));

import WidgetApp from "@/components/widget/WidgetApp";
import { t } from "@/i18n/zh";

const registrations = (event: string) =>
  mocks.listenCalls.filter((name) => name === event).length;

function pendingTask(id: number, overdue = false) {
  return {
    id,
    title: `任务${id}`,
    description: "",
    category_id: null,
    priority: "medium",
    status: "pending",
    due_at: overdue ? "2020-01-01T00:00:00Z" : null,
    created_at: "2026-10-01T00:00:00Z",
    updated_at: "2026-10-01T00:00:00Z",
    completed_at: null,
    reminders: [],
  };
}

beforeEach(() => {
  mocks.handlers.clear();
  mocks.listenCalls.length = 0;
  mocks.setMode("expanded");
  mocks.clock.now = new Date(2026, 9, 26, 12, 0).getTime();
  mocks.tasks = [];
  mocks.createTask.mockClear();
  apiMocks.widgetGetMode.mockClear();
  apiMocks.widgetExpand.mockClear();
  apiMocks.widgetCollapse.mockClear();
  apiMocks.widgetQuickAddClosed.mockClear();
});

describe("WidgetApp 窗口模式与快速添加", () => {
  it("时钟 tick 重渲染不重新注册任何监听器", async () => {
    const { rerender } = render(<WidgetApp />);
    await waitFor(() => expect(registrations("widget-quick-add")).toBe(1));
    expect(registrations("widget-mode")).toBe(1);

    mocks.clock.now += 1000;
    rerender(<WidgetApp />);
    mocks.clock.now += 1000;
    rerender(<WidgetApp />);

    expect(registrations("widget-quick-add")).toBe(1);
    expect(registrations("widget-mode")).toBe(1);
  });

  it("启动时读取后端模式：后端为圆圈时直接渲染圆圈", async () => {
    apiMocks.widgetGetMode.mockResolvedValueOnce("docked");
    mocks.tasks = [pendingTask(1), pendingTask(2)];
    render(<WidgetApp />);
    await waitFor(() => expect(screen.getByTestId("dock-bubble")).toBeTruthy());
    expect(screen.getByTestId("dock-bubble-count").textContent).toBe("2");
  });

  it("收起：调用后端收起命令并渲染标题栏（只剩标题栏）", async () => {
    render(<WidgetApp />);
    fireEvent.click(screen.getByTitle(t.widget.collapse));
    await waitFor(() => expect(apiMocks.widgetCollapse).toHaveBeenCalledTimes(1));
    // 标题栏内唯一的"展开"按钮（不再有额外的底部展开条）
    await waitFor(() => expect(screen.getAllByTitle(t.widget.expand)).toHaveLength(1));
    expect(screen.queryByTitle(t.widget.collapse)).toBeNull();
  });

  it("展开高度低于快速添加所需：请求临时 320，关闭后清除临时高度", async () => {
    render(<WidgetApp />);
    fireEvent.click(screen.getByTitle(t.widget.addTask));
    await waitFor(() =>
      expect(apiMocks.widgetExpand).toHaveBeenCalledWith(320),
    );
    expect(screen.getByPlaceholderText(t.quickAdd.placeholder)).toBeTruthy();

    fireEvent.click(screen.getByTitle(t.task.cancel));
    await waitFor(() => expect(apiMocks.widgetQuickAddClosed).toHaveBeenCalledTimes(1));
  });

  it("收起态托盘快速添加：先展开完整面板再打开面板", async () => {
    render(<WidgetApp />);
    fireEvent.click(screen.getByTitle(t.widget.collapse));
    await waitFor(() => expect(screen.getAllByTitle(t.widget.expand)).toHaveLength(1));

    await act(async () => {
      mocks.handlers.get("widget-quick-add")!({});
    });
    await waitFor(() => expect(apiMocks.widgetExpand).toHaveBeenCalledWith(320));
    expect(screen.getByPlaceholderText(t.quickAdd.placeholder)).toBeTruthy();
  });

  it("快速添加草稿跨收纳和收起保留，重新打开时标题与日期不丢失", async () => {
    render(<WidgetApp />);
    await waitFor(() => expect(registrations("widget-mode")).toBe(1));
    fireEvent.click(screen.getByTitle(t.widget.addTask));
    const title = await screen.findByPlaceholderText(t.quickAdd.placeholder);
    fireEvent.change(title, { target: { value: "尚未保存的草稿" } });
    fireEvent.change(screen.getByLabelText(t.task.dueAt), { target: { value: "2026-10-29T09:30" } });

    for (const mode of ["docked", "collapsed"]) {
      await act(async () => { mocks.handlers.get("widget-mode")!({ payload: { mode } }); });
      expect(screen.queryByPlaceholderText(t.quickAdd.placeholder)).toBeNull();
      await act(async () => { mocks.handlers.get("widget-mode")!({ payload: { mode: "expanded" } }); });
      fireEvent.click(screen.getByTitle(t.widget.addTask));
      expect((await screen.findByPlaceholderText(t.quickAdd.placeholder) as HTMLInputElement).value).toBe("尚未保存的草稿");
      expect((screen.getByLabelText(t.task.dueAt) as HTMLInputElement).value).toBe("2026-10-29T09:30");
    }
  });

  it("保存成功后清空草稿，下次快速添加不会重复带入已保存内容", async () => {
    render(<WidgetApp />);
    fireEvent.click(screen.getByTitle(t.widget.addTask));
    fireEvent.change(await screen.findByPlaceholderText(t.quickAdd.placeholder), { target: { value: "只保存一次" } });
    fireEvent.change(screen.getByLabelText(t.task.dueAt), { target: { value: "2026-10-29T09:30" } });
    fireEvent.click(screen.getByRole("button", { name: t.quickAdd.save }));
    await waitFor(() => expect(screen.queryByPlaceholderText(t.quickAdd.placeholder)).toBeNull());
    expect(mocks.createTask).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByTitle(t.widget.addTask));
    expect((await screen.findByPlaceholderText(t.quickAdd.placeholder) as HTMLInputElement).value).toBe("");
    expect((screen.getByLabelText(t.task.dueAt) as HTMLInputElement).value).toBe("");
  });

  it("同一分钟跨过截止秒数时，逾期统计随倒计时一起更新", async () => {
    const start = new Date(2026, 9, 28, 12, 0, 1).getTime();
    mocks.clock.now = start;
    mocks.tasks = [{ ...pendingTask(1), due_at: new Date(start + 9000).toISOString() }];
    const { rerender } = render(<WidgetApp />);
    const overdueValue = () => screen.getByText(t.widget.stats.overdue).previousElementSibling?.textContent;
    expect(overdueValue()).toBe("0");
    mocks.clock.now = start + 10000;
    rerender(<WidgetApp />);
    expect(overdueValue()).toBe("1");
    expect(screen.getByText("逾期 1秒")).toBeTruthy();
  });

  it("圆圈模式：单击展开完整面板", async () => {
    apiMocks.widgetGetMode.mockResolvedValueOnce("docked");
    render(<WidgetApp />);
    const bubble = await screen.findByTestId("dock-bubble");
    fireEvent.click(bubble);
    await waitFor(() => expect(apiMocks.widgetExpand).toHaveBeenCalledTimes(1));
  });

  it("圆圈模式：轻微抖动的单击仍然展开（拖动判定交给后端）", async () => {
    apiMocks.widgetGetMode.mockResolvedValueOnce("docked");
    render(<WidgetApp />);
    const bubble = await screen.findByTestId("dock-bubble");

    fireEvent.pointerDown(bubble, { button: 0, screenX: 100, screenY: 100 });
    fireEvent.pointerMove(bubble, { screenX: 101, screenY: 100 });
    fireEvent.pointerUp(bubble, { screenX: 101, screenY: 100 });
    fireEvent.click(bubble);

    await waitFor(() => expect(apiMocks.widgetExpand).toHaveBeenCalledTimes(1));
  });

  it("圆圈模式：展开失败保持圆圈，提示错误并允许再次点击重试", async () => {
    apiMocks.widgetGetMode.mockResolvedValueOnce("docked");
    apiMocks.widgetExpand.mockRejectedValueOnce(new Error("原生窗口操作失败"));
    render(<WidgetApp />);
    const bubble = await screen.findByTestId("dock-bubble");
    fireEvent.click(bubble);
    await waitFor(() => expect(bubble.title).toContain("点击重试"));
    expect(screen.queryByTitle(t.widget.addTask)).toBeNull();
    fireEvent.click(bubble);
    await waitFor(() => expect(screen.getByTitle(t.widget.addTask)).toBeTruthy());
    expect(apiMocks.widgetExpand).toHaveBeenCalledTimes(2);
  });

  it("圆圈模式：拖动后收到合成点击也不展开，并且只启动一次原生拖动", async () => {
    apiMocks.widgetGetMode.mockResolvedValueOnce("docked");
    render(<WidgetApp />);
    const bubble = await screen.findByTestId("dock-bubble");

    fireEvent.pointerDown(bubble, { button: 0, screenX: 100, screenY: 100 });
    fireEvent.pointerMove(bubble, { screenX: 140, screenY: 100 });
    fireEvent.pointerMove(bubble, { screenX: 150, screenY: 100 });
    fireEvent.pointerUp(bubble, { screenX: 140, screenY: 100 });
    fireEvent.click(bubble, { detail: 1 });
    fireEvent.click(bubble, { detail: 1 }); // 原生接管后重复补发的同手势点击也应被拦截

    expect(apiMocks.widgetExpand).not.toHaveBeenCalled();
    expect(windowMocks.startDragging).toHaveBeenCalledTimes(1);
  });

  it("圆圈模式：原生接管引发 pointercancel 后的合成点击仍不展开", async () => {
    apiMocks.widgetGetMode.mockResolvedValueOnce("docked");
    render(<WidgetApp />);
    const bubble = await screen.findByTestId("dock-bubble");
    fireEvent.pointerDown(bubble, { button: 0, screenX: 100, screenY: 100 });
    fireEvent.pointerMove(bubble, { screenX: 140, screenY: 100 });
    fireEvent.pointerCancel(bubble);
    fireEvent.click(bubble, { detail: 1 });
    expect(apiMocks.widgetExpand).not.toHaveBeenCalled();
    fireEvent.pointerDown(bubble, { button: 0, screenX: 140, screenY: 100 });
    fireEvent.pointerUp(bubble);
    fireEvent.click(bubble, { detail: 1 });
    await waitFor(() => expect(apiMocks.widgetExpand).toHaveBeenCalledTimes(1));
  });

  it("圆圈模式：拖动后仍支持键盘激活展开", async () => {
    apiMocks.widgetGetMode.mockResolvedValueOnce("docked");
    render(<WidgetApp />);
    const bubble = await screen.findByTestId("dock-bubble");
    fireEvent.pointerDown(bubble, { button: 0, screenX: 100, screenY: 100 });
    fireEvent.pointerMove(bubble, { screenX: 140, screenY: 100 });
    fireEvent.pointerUp(bubble);
    fireEvent.click(bubble, { detail: 0 });
    await waitFor(() => expect(apiMocks.widgetExpand).toHaveBeenCalledTimes(1));
  });

  it("圆圈模式托盘快速添加：先恢复完整面板，再请求临时 320", async () => {
    apiMocks.widgetGetMode.mockResolvedValueOnce("docked");
    const rendered = render(<WidgetApp />);
    await screen.findByTestId("dock-bubble");

    // 后端 show_widget 先把圆圈展开为完整面板，再发 widget-quick-add
    await act(async () => {
      mocks.handlers.get("widget-quick-add")!({ payload: null });
    });

    await waitFor(() =>
      expect(apiMocks.widgetExpand.mock.calls.some((c) => c[0] === 320)).toBe(true),
    );
    rendered.unmount();
  });

  it("从圆圈展开会清空快速添加标记（不残留窄高度面板）", async () => {
    apiMocks.widgetGetMode.mockResolvedValueOnce("docked");
    render(<WidgetApp />);
    const bubble = await screen.findByTestId("dock-bubble");
    fireEvent.click(bubble);
    await waitFor(() => expect(apiMocks.widgetExpand).toHaveBeenCalled());
    // 展开后是完整面板，快速添加面板必须处于关闭态
    await waitFor(() =>
      expect(screen.queryByPlaceholderText(t.quickAdd.placeholder)).toBeNull(),
    );
    expect(apiMocks.widgetExpand).toHaveBeenCalledWith();
  });

  it("widget-mode 事件：切换到圆圈并从圆圈恢复完整面板", async () => {
    render(<WidgetApp />);
    await waitFor(() => expect(registrations("widget-mode")).toBe(1));

    mocks.tasks = [pendingTask(1)];
    await act(async () => {
      mocks.handlers.get("widget-mode")!({ payload: { mode: "docked" } });
    });
    expect(screen.getByTestId("dock-bubble")).toBeTruthy();

    // 后端展开后推送 widget-mode=expanded，前端据此切回完整面板
    await act(async () => {
      mocks.handlers.get("widget-mode")!({ payload: { mode: "expanded" } });
    });
    expect(screen.queryByTestId("dock-bubble")).toBeNull();
    expect(screen.queryByTitle(t.widget.addTask)).not.toBeNull();
  });

  it("圆圈计数：包含逾期任务，0 与 99+ 边界", async () => {
    apiMocks.widgetGetMode.mockResolvedValueOnce("docked");

    const { unmount } = render(<WidgetApp />);
    await screen.findByTestId("dock-bubble");
    expect(screen.getByTestId("dock-bubble-count").textContent).toBe("0");
    unmount();

    mocks.setMode("expanded");
    mocks.tasks = [
      pendingTask(1, true), // 逾期仍计入
      pendingTask(2),
      { ...pendingTask(3), status: "completed" },
    ];
    apiMocks.widgetGetMode.mockResolvedValueOnce("docked");
    const second = render(<WidgetApp />);
    await screen.findByTestId("dock-bubble");
    expect(screen.getByTestId("dock-bubble-count").textContent).toBe("2");
    second.unmount();

    mocks.setMode("expanded");
    mocks.tasks = Array.from({ length: 128 }, (_, i) => pendingTask(i + 1));
    apiMocks.widgetGetMode.mockResolvedValueOnce("docked");
    render(<WidgetApp />);
    await screen.findByTestId("dock-bubble");
    expect(screen.getByTestId("dock-bubble-count").textContent).toBe("99+");
  });

  it("卸载时注销所有监听器", async () => {
    const { unmount } = render(<WidgetApp />);
    await waitFor(() => expect(registrations("widget-mode")).toBe(1));
    unmount();
    await waitFor(() => {
      expect(mocks.handlers.has("widget-mode")).toBe(false);
      expect(mocks.handlers.has("widget-quick-add")).toBe(false);
    });
  });

  it("旧启动快照不能覆盖已收到的圆圈模式事件", async () => {
    let resolve!: (mode: string) => void;
    apiMocks.widgetGetMode.mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
    render(<WidgetApp />);
    await waitFor(() => expect(apiMocks.widgetGetMode).toHaveBeenCalled());
    await act(async () => {
      mocks.handlers.get("widget-mode")!({ payload: { mode: "docked" } });
      resolve("expanded");
    });
    expect(screen.getByTestId("dock-bubble")).toBeTruthy();
  });

  it("旧展开响应不能覆盖更新的后端收纳事件", async () => {
    let resolve!: (mode: string) => void;
    apiMocks.widgetExpand.mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
    render(<WidgetApp />);
    await waitFor(() => expect(apiMocks.widgetGetMode).toHaveBeenCalled());
    fireEvent.click(screen.getByTitle(t.widget.addTask));
    await waitFor(() => expect(apiMocks.widgetExpand).toHaveBeenCalled());
    await act(async () => {
      mocks.handlers.get("widget-mode")!({ payload: { mode: "docked" } });
      resolve("expanded");
    });
    expect(screen.getByTestId("dock-bubble")).toBeTruthy();
    expect(screen.queryByPlaceholderText(t.quickAdd.placeholder)).toBeNull();
  });

  it("快速添加等待原生高度调整成功，失败时不打开被裁切的表单", async () => {
    apiMocks.widgetExpand.mockRejectedValueOnce(new Error("resize failed"));
    render(<WidgetApp />);
    fireEvent.click(screen.getByTitle(t.widget.addTask));
    await screen.findByRole("alert");
    expect(screen.queryByPlaceholderText(t.quickAdd.placeholder)).toBeNull();
    expect(screen.getByTitle(t.widget.collapse)).toBeTruthy();
  });

  it("原生先发 expanded 事件再返回命令结果时，快速添加仍能打开", async () => {
    apiMocks.widgetExpand.mockImplementationOnce(async () => {
      mocks.handlers.get("widget-mode")!({ payload: { mode: "expanded" } });
      return "expanded";
    });
    render(<WidgetApp />);
    await waitFor(() => expect(registrations("widget-mode")).toBe(1));
    fireEvent.click(screen.getByTitle(t.widget.addTask));
    expect(await screen.findByPlaceholderText(t.quickAdd.placeholder)).toBeTruthy();
  });

  it("收起失败时保持完整面板", async () => {
    apiMocks.widgetCollapse.mockRejectedValueOnce(new Error("collapse failed"));
    render(<WidgetApp />);
    fireEvent.click(screen.getByTitle(t.widget.collapse));
    await screen.findByRole("alert");
    expect(screen.getByTitle(t.widget.collapse)).toBeTruthy();
    expect(screen.queryByTitle(t.widget.expand)).toBeNull();
  });

  it("高度恢复失败时保留快速添加，允许再次关闭重试", async () => {
    apiMocks.widgetQuickAddClosed.mockRejectedValueOnce(new Error("restore failed"));
    render(<WidgetApp />);
    fireEvent.click(screen.getByTitle(t.widget.addTask));
    await screen.findByPlaceholderText(t.quickAdd.placeholder);
    fireEvent.change(screen.getByPlaceholderText(t.quickAdd.placeholder), { target: { value: "关闭失败仍保留" } });
    fireEvent.click(screen.getByTitle(t.task.cancel));
    await screen.findByRole("alert");
    expect(screen.getByPlaceholderText(t.quickAdd.placeholder)).toBeTruthy();
    expect((screen.getByPlaceholderText(t.quickAdd.placeholder) as HTMLInputElement).value).toBe("关闭失败仍保留");
    fireEvent.click(screen.getByTitle(t.task.cancel));
    await waitFor(() => expect(screen.queryByPlaceholderText(t.quickAdd.placeholder)).toBeNull());
    expect(apiMocks.widgetQuickAddClosed).toHaveBeenCalledTimes(2);
  });

  it("任务保存失败后关闭并重开，仍保留可重试的草稿", async () => {
    mocks.createTask.mockRejectedValueOnce(new Error("test save failure"));
    render(<WidgetApp />);
    fireEvent.click(screen.getByTitle(t.widget.addTask));
    fireEvent.change(await screen.findByPlaceholderText(t.quickAdd.placeholder), { target: { value: "保存失败仍保留" } });
    fireEvent.click(screen.getByRole("button", { name: t.quickAdd.save }));
    await screen.findByText("test save failure");
    fireEvent.click(screen.getByTitle(t.task.cancel));
    await waitFor(() => expect(screen.queryByPlaceholderText(t.quickAdd.placeholder)).toBeNull());
    fireEvent.click(screen.getByTitle(t.widget.addTask));
    expect((await screen.findByPlaceholderText(t.quickAdd.placeholder) as HTMLInputElement).value).toBe("保存失败仍保留");
  });
});
