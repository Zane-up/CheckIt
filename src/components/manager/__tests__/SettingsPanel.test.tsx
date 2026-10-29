/**
 * 设置页“默认提醒”回归：
 * - 自定义默认提醒回显（含非预设值）
 * - 添加/移除自定义默认提醒的保存载荷
 * - 校验与去重与任务表单一致（复用 ReminderPicker）
 */

import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSettings } from "@/types";

const mocks = vi.hoisted(() => {
  return {
    updateSettings: vi.fn(async () => {}),
    settings: null as AppSettings | null,
    reset() {
      mocks.updateSettings.mockClear();
      mocks.settings = {
        theme: "dark",
        glass_enabled: true,
        glass_opacity: 0.35,
        always_on_top: true,
        close_to_tray: true,
        notifications_enabled: true,
        popup_reminders_enabled: false,
        default_reminder_offsets: [3600, 1500],
        widget_display_count: 8,
        launch_at_login: false,
        edge_dock_enabled: true,
      };
    },
  };
});

vi.mock("@/hooks/useAppData", () => ({
  useAppData: () => ({
    state: { tasks: [], categories: [], settings: mocks.settings },
    updateSettings: mocks.updateSettings,
  }),
}));

vi.mock("@/services/api", () => ({
  api: { getDbPath: vi.fn(async () => "C:\\fake\\checkit.db"), previewPopupReminder: vi.fn(async () => {}) },
}));

import SettingsPanel from "@/components/manager/SettingsPanel";
import { t } from "@/i18n/zh";

beforeEach(() => {
  mocks.reset();
});

/** 等待 getDbPath 异步回填完成，避免 act 警告 */
async function waitForDbPath() {
  await screen.findByText("C:\\fake\\checkit.db");
}

describe("SettingsPanel 默认提醒", () => {
  it("居中弹窗开关独立保存，不改变系统通知", async () => {
    render(<SettingsPanel />);
    await waitForDbPath();
    const toggle = screen.getByRole("switch", { name: t.settings.popupReminders });
    expect(toggle.getAttribute("aria-checked")).toBe("false");
    await userEvent.click(toggle);
    expect(mocks.updateSettings).toHaveBeenCalledWith({ popup_reminders_enabled: true });
  });

  it("回显自定义默认提醒（非预设值 25 分钟）", async () => {
    render(<SettingsPanel />);
    await waitForDbPath();
    expect(screen.getByText("提前 25 分钟")).toBeTruthy();
    expect(screen.getByLabelText(`${t.task.reminderRemove} 提前 25 分钟`)).toBeTruthy();
    expect(screen.getByText(t.settings.defaultRemindersHint)).toBeTruthy();
  });

  it("添加自定义默认提醒：换算秒数并持久化", async () => {
    render(<SettingsPanel />);
    await waitForDbPath();
    const amount = screen.getByLabelText(t.task.reminderAmountLabel);
    const unit = screen.getByLabelText(t.task.reminderUnitLabel);
    await userEvent.type(amount, "2");
    fireEvent.change(unit, { target: { value: "hour" } });
    fireEvent.click(screen.getByRole("button", { name: t.task.reminderAdd }));

    expect(mocks.updateSettings).toHaveBeenCalledWith({
      default_reminder_offsets: [1500, 3600, 7200],
    });
  });

  it("移除一个默认提醒：其他提醒保留", async () => {
    render(<SettingsPanel />);
    await waitForDbPath();
    await userEvent.click(screen.getByLabelText(`${t.task.reminderRemove} 提前 25 分钟`));
    expect(mocks.updateSettings).toHaveBeenCalledWith({
      default_reminder_offsets: [3600],
    });
  });

  it("非法输入不触发保存：与任务表单一致", async () => {
    render(<SettingsPanel />);
    await waitForDbPath();
    const amount = screen.getByLabelText(t.task.reminderAmountLabel);
    await userEvent.type(amount, "61");
    fireEvent.change(screen.getByLabelText(t.task.reminderUnitLabel), {
      target: { value: "day" },
    });
    fireEvent.click(screen.getByRole("button", { name: t.task.reminderAdd }));

    expect(screen.getByText(t.errors.reminderAmountTooLarge)).toBeTruthy();
    expect(mocks.updateSettings).not.toHaveBeenCalled();
    expect((amount as HTMLInputElement).value).toBe("61");
  });
});
