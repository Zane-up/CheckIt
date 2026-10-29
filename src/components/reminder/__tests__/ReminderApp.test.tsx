import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PopupSnapshot } from "@/services/api";

const mocks = vi.hoisted(() => ({
  now: Date.parse("2030-01-01T00:05:00Z"),
  snapshot: { reminders: [], preview: false } as PopupSnapshot,
  handler: null as (() => void) | null,
  stop: vi.fn(),
  get: vi.fn<() => Promise<PopupSnapshot>>(),
  dismiss: vi.fn(async (_ids: number[]) => {}),
  manager: vi.fn(async () => {}),
}));
vi.mock("@/services/api", () => ({ api: {
  getPopupReminders: mocks.get, dismissPopupReminders: mocks.dismiss, openManager: mocks.manager,
} }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async (_event: string, handler: () => void) => {
  mocks.handler = handler; return mocks.stop;
}) }));
vi.mock("@/hooks/useNow", () => ({ useNow: () => mocks.now }));
import ReminderApp from "../ReminderApp";

const item = (id: number) => ({ id, task_id: id, title: `待办 ${id}`, body: "还有10分钟到期", due_at: "2030-01-01T00:00:00Z" });

beforeEach(() => {
  mocks.snapshot = { reminders: [item(1)], preview: false };
  mocks.get.mockReset().mockImplementation(async () => structuredClone(mocks.snapshot));
  mocks.dismiss.mockReset().mockResolvedValue(undefined);
  mocks.stop.mockClear(); mocks.handler = null;
});
afterEach(() => { vi.useRealTimers(); });

describe("居中提醒窗口", () => {
  it("根据当前时钟展示逾期时间，不沿用触发时的旧正文", async () => {
    render(<ReminderApp />);
    await screen.findByText("逾期 5分钟 0秒");
    expect(screen.queryByText("还有10分钟到期")).toBeNull();
  });

  it("从快照恢复提醒，确认只提交当前可见 ID", async () => {
    render(<ReminderApp />);
    await screen.findByText("待办 1");
    mocks.snapshot = { reminders: [item(1), item(2)], preview: false };
    // 后端刚收到第二条、前端尚未显示时，确认不能误清第二条。
    fireEvent.click(screen.getByRole("button", { name: "知道了" }));
    expect(mocks.dismiss).toHaveBeenCalledWith([1]);
  });

  it("新提醒事件刷新列表，多条提醒合并在同一个窗口", async () => {
    render(<ReminderApp />);
    await screen.findByText("待办 1");
    mocks.snapshot = { reminders: [item(1), item(2)], preview: false };
    await act(async () => { mocks.handler?.(); });
    await screen.findByText("待办 2");
    fireEvent.click(screen.getByRole("button", { name: "全部知道了（2）" }));
    expect(mocks.dismiss).toHaveBeenCalledWith([1, 2]);
  });

  it("确认失败时保留提醒并展示错误", async () => {
    mocks.dismiss.mockRejectedValueOnce(new Error("磁盘不可写"));
    render(<ReminderApp />);
    await screen.findByText("待办 1");
    fireEvent.click(screen.getByRole("button", { name: "知道了" }));
    await screen.findByRole("alert");
    expect(screen.getByText("待办 1")).toBeTruthy();
  });

  it("纯预览无需真实任务，关闭不会确认真实提醒", async () => {
    mocks.snapshot = { reminders: [], preview: true };
    render(<ReminderApp />);
    await screen.findByText("这是你的提醒弹窗");
    fireEvent.click(screen.getByRole("button", { name: "关闭提醒" }));
    expect(mocks.dismiss).toHaveBeenCalledWith([]);
  });

  it("卸载时清理事件和轮询定时器", async () => {
    vi.useFakeTimers();
    const view = render(<ReminderApp />);
    await act(async () => { await Promise.resolve(); });
    view.unmount();
    expect(mocks.stop).toHaveBeenCalledTimes(1);
    const calls = mocks.get.mock.calls.length;
    await act(async () => { vi.advanceTimersByTime(10_000); });
    expect(mocks.get.mock.calls.length).toBe(calls);
  });

  it("读取失败后通过轮询自动恢复", async () => {
    vi.useFakeTimers();
    mocks.get.mockRejectedValue(new Error("读取失败"));
    render(<ReminderApp />);
    await act(async () => { await Promise.resolve(); });
    expect(screen.getByRole("alert")).toBeTruthy();
    mocks.get.mockResolvedValue({ reminders: [item(1)], preview: false });
    await act(async () => { vi.advanceTimersByTime(3000); });
    expect(screen.getByText("待办 1")).toBeTruthy();
  });
});
