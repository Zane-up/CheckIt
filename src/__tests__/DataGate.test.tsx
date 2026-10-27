/**
 * DataGate 回归：首屏失败才替换界面；已加载后刷新失败保留组件树与未保存草稿。
 */

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { listen } from "@tauri-apps/api/event";
import { DataGate } from "@/App";
import { AppDataProvider } from "@/hooks/useAppData";
import { api } from "@/services/api";
import type { AppSettings, AppState } from "@/types";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const listenMock = vi.mocked(listen);
type Handler = (event: { payload: unknown }) => void;
let handlers: Map<string, Handler>;

function settings(): AppSettings {
  return {
    theme: "dark",
    glass_enabled: true,
    glass_opacity: 0.35,
    always_on_top: true,
    close_to_tray: true,
    notifications_enabled: true,
    default_reminder_offsets: [3600, 600],
    widget_display_count: 8,
    launch_at_login: false,
    edge_dock_enabled: true,
  };
}

function state(): AppState {
  return { tasks: [], categories: [], settings: settings() };
}

function DraftForm() {
  return <input aria-label="draft" defaultValue="" placeholder="未保存草稿" />;
}

function renderGate() {
  return render(
    <AppDataProvider>
      <DataGate>
        <DraftForm />
      </DataGate>
    </AppDataProvider>,
  );
}

beforeEach(() => {
  handlers = new Map();
  listenMock.mockImplementation((async (event: string, handler: Handler) => {
    handlers.set(event, handler);
    return () => handlers.delete(event);
  }) as unknown as typeof listen);
});

describe("DataGate", () => {
  it("首屏加载失败：显示错误页与重试；重试成功后渲染内容", async () => {
    vi.spyOn(api, "getAppState")
      .mockRejectedValueOnce(new Error("db unavailable"))
      .mockResolvedValue(state());

    renderGate();

    await waitFor(() => expect(screen.getByText("数据加载失败")).toBeTruthy());
    expect(screen.queryByPlaceholderText("未保存草稿")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: /重试/ }));
    await waitFor(() => expect(screen.getByPlaceholderText("未保存草稿")).toBeTruthy());
    expect(screen.queryByText("数据加载失败")).toBeNull();
  });

  it("已有数据后刷新失败：保留组件树与未保存草稿，仅显示可重试提示", async () => {
    const get = vi.spyOn(api, "getAppState").mockResolvedValue(state());
    renderGate();

    const draft = (await screen.findByPlaceholderText("未保存草稿")) as HTMLInputElement;
    fireEvent.change(draft, { target: { value: "写到一半的内容" } });
    expect(draft.value).toBe("写到一半的内容");

    get.mockRejectedValueOnce(new Error("snapshot failed"));
    handlers.get("data-changed")!({ payload: { entity: "tasks" } });

    await waitFor(() => expect(screen.getByTestId("data-error-banner")).toBeTruthy());
    expect(screen.getByText("刷新失败，当前显示上次数据")).toBeTruthy();
    // 关键：组件树没有卸载，草稿仍在
    const stillThere = screen.getByPlaceholderText("未保存草稿") as HTMLInputElement;
    expect(stillThere).toBe(draft);
    expect(stillThere.value).toBe("写到一半的内容");

    fireEvent.click(screen.getByRole("button", { name: /重试/ }));
    await waitFor(() => expect(screen.queryByTestId("data-error-banner")).toBeNull());
    expect((screen.getByPlaceholderText("未保存草稿") as HTMLInputElement).value).toBe(
      "写到一半的内容",
    );
  });

  it("事件订阅失败：显示同步降级提示，但界面与数据仍可用", async () => {
    listenMock.mockRejectedValueOnce(new Error("listen down"));
    vi.spyOn(api, "getAppState").mockResolvedValue(state());

    renderGate();

    await waitFor(() => expect(screen.getByTestId("data-error-banner")).toBeTruthy());
    expect(screen.getByText("实时同步不可用，数据可能不是最新")).toBeTruthy();
    expect(screen.getByPlaceholderText("未保存草稿")).toBeTruthy();
  });
});
