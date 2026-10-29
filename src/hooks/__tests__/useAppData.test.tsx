/**
 * 数据同步回归测试：
 * - 先订阅、后取快照；快照期间事件不会丢失，旧慢响应不得覆盖新数据
 * - 订阅失败降级为手动刷新，并可重试
 * - 已加载后刷新失败保留旧数据
 * - 设置响应乱序只接受最后一次
 */

import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { listen } from "@tauri-apps/api/event";
import { AppDataProvider, useAppData } from "@/hooks/useAppData";
import { api } from "@/services/api";
import type { AppSettings, AppState } from "@/types";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const listenMock = vi.mocked(listen);

type Handler = (event: { payload: unknown }) => void;

let handlers: Map<string, Handler>;
let unlistenCalls: string[];

function baseSettings(overrides: Partial<AppSettings> = {}): AppSettings {
  return {
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
    ...overrides,
  };
}

function makeState(titles: string[]): AppState {
  return {
    tasks: titles.map((title, i) => ({
      id: i + 1,
      title,
      description: "",
      category_id: null,
      priority: "medium",
      status: "pending",
      due_at: null,
      created_at: "2026-10-26T00:00:00Z",
      updated_at: "2026-10-26T00:00:00Z",
      completed_at: null,
      reminders: [],
    })),
    categories: [],
    settings: baseSettings(),
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function Probe() {
  const { state, hasData, loading, error, syncError, reload, retrySync, updateSettings } =
    useAppData();
  return (
    <div>
      <span data-testid="title">{state.tasks[0]?.title ?? ""}</span>
      <span data-testid="has-data">{String(hasData)}</span>
      <span data-testid="loading">{String(loading)}</span>
      <span data-testid="error">{error ?? ""}</span>
      <span data-testid="sync-error">{syncError ?? ""}</span>
      <span data-testid="theme">{state.settings.theme}</span>
      <button type="button" onClick={() => void reload()}>
        reload
      </button>
      <button type="button" onClick={() => retrySync()}>
        retry-sync
      </button>
      <button type="button" onClick={() => void updateSettings({ theme: "light" })}>
        light
      </button>
      <button type="button" onClick={() => void updateSettings({ theme: "system" })}>
        system
      </button>
    </div>
  );
}

beforeEach(() => {
  handlers = new Map();
  unlistenCalls = [];
  listenMock.mockImplementation((async (event: string, handler: Handler) => {
    handlers.set(event, handler);
    return () => {
      unlistenCalls.push(event);
      handlers.delete(event);
    };
  }) as typeof listen);
});

describe("useAppData 首屏与事件订阅顺序", () => {
  it("首屏早于后端初始化时，初始化完成事件可自动恢复", async () => {
    vi.spyOn(api, "getAppState")
      .mockRejectedValueOnce(new Error("state not managed"))
      .mockResolvedValue(makeState(["初始化完成"]));
    render(<AppDataProvider><Probe /></AppDataProvider>);
    await waitFor(() => expect(screen.getByTestId("error").textContent).toContain("state not managed"));
    await act(async () => { handlers.get("data-changed")!({ payload: { entity: "settings" } }); });
    await waitFor(() => expect(screen.getByTestId("title").textContent).toBe("初始化完成"));
    expect(screen.getByTestId("error").textContent).toBe("");
  });

  it("订阅就绪前不读取快照；订阅成功后立即读取", async () => {
    const listenGate = deferred<() => void>();
    listenMock.mockImplementationOnce((() => listenGate.promise) as typeof listen);
    const get = vi.spyOn(api, "getAppState").mockResolvedValue(makeState(["首屏任务"]));

    render(
      <AppDataProvider>
        <Probe />
      </AppDataProvider>,
    );

    // listen 尚未 resolve：绝不能先取快照（否则中间事件会丢）
    expect(get).not.toHaveBeenCalled();
    expect(listenMock).toHaveBeenCalledWith("data-changed", expect.any(Function));

    await act(async () => {
      listenGate.resolve(() => {});
    });

    await waitFor(() => expect(get).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(screen.getByTestId("title").textContent).toBe("首屏任务"));
  });

  it("快照请求期间收到事件 → 重取最新；旧慢响应被丢弃", async () => {
    const first = deferred<AppState>();
    const second = deferred<AppState>();
    const get = vi
      .spyOn(api, "getAppState")
      .mockImplementationOnce(() => first.promise)
      .mockImplementationOnce(() => second.promise);

    render(
      <AppDataProvider>
        <Probe />
      </AppDataProvider>,
    );

    await waitFor(() => expect(get).toHaveBeenCalledTimes(1));
    // 订阅已就绪，且首个快照仍在途
    expect(handlers.has("data-changed")).toBe(true);

    act(() => {
      handlers.get("data-changed")!({ payload: { entity: "tasks" } });
    });
    expect(get).toHaveBeenCalledTimes(2);

    await act(async () => {
      second.resolve(makeState(["新数据"]));
    });
    await waitFor(() => expect(screen.getByTestId("title").textContent).toBe("新数据"));

    // 旧响应后到：序号过期，必须被忽略
    await act(async () => {
      first.resolve(makeState(["旧数据"]));
    });
    expect(screen.getByTestId("title").textContent).toBe("新数据");
  });

  it("卸载时注销 data-changed 订阅", async () => {
    vi.spyOn(api, "getAppState").mockResolvedValue(makeState(["卸载"]));
    const { unmount } = render(
      <AppDataProvider>
        <Probe />
      </AppDataProvider>,
    );
    await waitFor(() => expect(screen.getByTestId("has-data").textContent).toBe("true"));
    unmount();
    expect(unlistenCalls).toContain("data-changed");
  });

  it("订阅注册失败仍可加载数据，并显示降级提示；重试后恢复订阅", async () => {
    listenMock.mockRejectedValueOnce(new Error("listen unavailable"));
    const get = vi.spyOn(api, "getAppState").mockResolvedValue(makeState(["降级可用"]));

    render(
      <AppDataProvider>
        <Probe />
      </AppDataProvider>,
    );

    await waitFor(() =>
      expect(screen.getByTestId("sync-error").textContent).toContain("listen unavailable"),
    );
    expect(screen.getByTestId("has-data").textContent).toBe("true");
    expect(screen.getByTestId("title").textContent).toBe("降级可用");

    await userEvent.click(screen.getByRole("button", { name: "retry-sync" }));

    await waitFor(() => expect(screen.getByTestId("sync-error").textContent).toBe(""));
    await waitFor(() => expect(get.mock.calls.length).toBeGreaterThanOrEqual(2));
    expect(handlers.has("data-changed")).toBe(true);
  });
});

describe("useAppData 刷新失败与旧数据", () => {
  it("已加载后刷新失败：保留旧数据并记录错误", async () => {
    const get = vi.spyOn(api, "getAppState");
    get.mockResolvedValueOnce(makeState(["保留的任务"]));

    render(
      <AppDataProvider>
        <Probe />
      </AppDataProvider>,
    );
    await waitFor(() => expect(screen.getByTestId("title").textContent).toBe("保留的任务"));

    get.mockRejectedValueOnce(new Error("database locked"));
    act(() => {
      handlers.get("data-changed")!({ payload: { entity: "tasks" } });
    });

    await waitFor(() =>
      expect(screen.getByTestId("error").textContent).toContain("database locked"),
    );
    expect(screen.getByTestId("title").textContent).toBe("保留的任务");
    expect(screen.getByTestId("has-data").textContent).toBe("true");
  });
});

describe("useAppData 设置响应顺序", () => {
  it("乱序响应只应用最后一次请求的结果", async () => {
    vi.spyOn(api, "getAppState").mockResolvedValue(makeState([]));
    const slow = deferred<AppSettings>();
    const fast = deferred<AppSettings>();
    vi.spyOn(api, "updateSettings")
      .mockImplementationOnce(() => slow.promise)
      .mockImplementationOnce(() => fast.promise);

    render(
      <AppDataProvider>
        <Probe />
      </AppDataProvider>,
    );
    await waitFor(() => expect(screen.getByTestId("has-data").textContent).toBe("true"));

    await userEvent.click(screen.getByRole("button", { name: "light" }));
    await userEvent.click(screen.getByRole("button", { name: "system" }));

    // 乐观更新：立即反映第二次操作
    expect(screen.getByTestId("theme").textContent).toBe("system");

    await act(async () => {
      fast.resolve(baseSettings({ theme: "system" }));
    });
    await act(async () => {
      slow.resolve(baseSettings({ theme: "light" }));
    });

    expect(screen.getByTestId("theme").textContent).toBe("system");
  });
});
