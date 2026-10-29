import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import UpdateChecker from "../UpdateChecker";

vi.mock("@/services/api", () => ({ api: { openReleasePage: vi.fn(async () => {}) } }));
afterEach(() => { vi.unstubAllGlobals(); vi.useRealTimers(); });
describe("手动检查更新", () => {
  it("不自动联网，重复点击不重复请求，卸载取消请求", () => {
    const fetcher = vi.fn(() => new Promise(() => {}));
    vi.stubGlobal("fetch", fetcher);
    const view = render(<UpdateChecker />);
    expect(fetcher).not.toHaveBeenCalled();
    fireEvent.click(screen.getByText("检查更新"));
    fireEvent.click(screen.getByText("正在检查…"));
    expect(fetcher).toHaveBeenCalledTimes(1);
    const signal = (fetcher.mock.calls as unknown as [string, RequestInit][])[0][1].signal;
    view.unmount();
    expect(signal?.aborted).toBe(true);
  });
  it("网络失败可重试", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("offline")));
    render(<UpdateChecker />);
    fireEvent.click(screen.getByText("检查更新"));
    expect((await screen.findByRole("alert")).textContent).toContain("无法连接 GitHub");
    await waitFor(() => expect((screen.getByText("检查更新") as HTMLButtonElement).disabled).toBe(false));
  });
  it("超时终止等待并展示提示", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("fetch", vi.fn((_url, init: RequestInit) => new Promise((_resolve, reject) => init.signal?.addEventListener("abort", () => reject(new Error("aborted"))))));
    render(<UpdateChecker />);
    fireEvent.click(screen.getByText("检查更新"));
    await act(async () => { await vi.advanceTimersByTimeAsync(15000); });
    expect(screen.getByRole("alert").textContent).toContain("检查超时");
  });
});
