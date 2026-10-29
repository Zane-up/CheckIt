import { afterEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { api } from "../api";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
afterEach(() => { vi.useRealTimers(); vi.mocked(invoke).mockReset(); });

it("仅对启动状态尚未托管进行延迟重试", async () => {
  vi.useFakeTimers();
  vi.mocked(invoke).mockRejectedValueOnce("state not managed for field `state`").mockResolvedValueOnce({ tasks: [] });
  const request = api.getAppState();
  await vi.advanceTimersByTimeAsync(250);
  expect(await request).toEqual({ tasks: [] });
  expect(invoke).toHaveBeenCalledTimes(2);
});

it("数据库实际错误立即抛出，不重试或伪装成空数据", async () => {
  vi.mocked(invoke).mockRejectedValueOnce("database disk image is malformed");
  await expect(api.getAppState()).rejects.toThrow("database disk image is malformed");
  expect(invoke).toHaveBeenCalledTimes(1);
});
