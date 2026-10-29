/**
 * 任务表单 × 自定义提醒集成回归：
 * - 编辑已有非预设提醒时回显，未修改保存不丢值
 * - 自定义输入 Enter 只添加提醒，不误触任务保存
 * - 草稿未添加时阻止保存并明确提示
 * - 未设截止时间时禁用自定义添加
 * - 新建任务继承设置中的默认提醒
 */

import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AppSettings, Task } from "@/types";

const mocks = vi.hoisted(() => {
  return {
    createTask: vi.fn(async (_input: unknown) => {}),
    updateTask: vi.fn(async (_id: number, _input: unknown) => {}),
    deleteTask: vi.fn(async () => {}),
    settings: null as AppSettings | null,
    reset() {
      mocks.createTask.mockClear();
      mocks.updateTask.mockClear();
      mocks.deleteTask.mockClear();
      mocks.settings = {
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
      };
    },
  };
});

vi.mock("@/hooks/useAppData", () => ({
  useAppData: () => ({
    state: { tasks: [], categories: [], settings: mocks.settings },
    createTask: mocks.createTask,
    updateTask: mocks.updateTask,
    deleteTask: mocks.deleteTask,
  }),
}));

import TaskFormModal from "@/components/manager/TaskFormModal";
import { t } from "@/i18n/zh";

function makeTask(overrides: Partial<Task> = {}): Task {
  return {
    id: 1,
    title: "已有任务",
    description: "",
    category_id: null,
    priority: "medium",
    status: "pending",
    due_at: "2026-11-01T04:00:00Z",
    created_at: "2026-10-26T00:00:00Z",
    updated_at: "2026-10-26T00:00:00Z",
    completed_at: null,
    reminders: [1500, 3600],
    ...overrides,
  };
}

beforeEach(() => {
  mocks.reset();
});

/** 填写新建任务必填项并设置截止时间 */
async function fillNewTaskForm(container: HTMLElement) {
  const title = screen.getByPlaceholderText(t.task.titlePlaceholder);
  await userEvent.type(title, "测试任务");
  const due = container.querySelector('input[type="datetime-local"]') as HTMLInputElement;
  fireEvent.change(due, { target: { value: "2026-11-01T12:00" } });
  return due;
}

describe("TaskFormModal 自定义提醒", () => {
  it("编辑已有非预设提醒：回显为自定义标签，未修改保存不丢值", async () => {
    const task = makeTask({ reminders: [1500, 3600] });
    render(<TaskFormModal task={task} onClose={() => {}} />);
    // 非预设值 1500 回显为“提前 25 分钟”，预设 3600 照常展示
    expect(screen.getByText("提前 25 分钟")).toBeTruthy();
    expect(screen.getByLabelText(`${t.task.reminderRemove} 提前 1 小时`)).toBeTruthy();

    await userEvent.click(screen.getByRole("button", { name: t.task.save }));
    expect(mocks.updateTask).toHaveBeenCalledTimes(1);
    const input = mocks.updateTask.mock.calls[0]![1] as { reminders: number[]; due_at: string | null };
    expect(input.reminders).toEqual([1500, 3600]);
    expect(input.due_at).toBe("2026-11-01T04:00:00Z");
  });

  it("Enter 只添加自定义提醒，不触发任务保存", async () => {
    const { container } = render(<TaskFormModal task={null} onClose={() => {}} />);
    await fillNewTaskForm(container);

    const amount = screen.getByLabelText(t.task.reminderAmountLabel);
    await userEvent.type(amount, "25");
    fireEvent.keyDown(amount, { key: "Enter" });

    expect(mocks.createTask).not.toHaveBeenCalled();
    expect(screen.getByText("提前 25 分钟")).toBeTruthy();

    // 添加完成后再保存：reminders 携带 1500
    await userEvent.click(screen.getByRole("button", { name: t.task.save }));
    expect(mocks.createTask).toHaveBeenCalledTimes(1);
    expect((mocks.createTask.mock.calls[0]![0] as { reminders: number[] }).reminders).toContain(1500);
  });

  it("草稿未添加时：阻止保存并提示先添加或清空", async () => {
    const { container } = render(<TaskFormModal task={null} onClose={() => {}} />);
    await fillNewTaskForm(container);

    const amount = screen.getByLabelText(t.task.reminderAmountLabel);
    await userEvent.type(amount, "25");

    await userEvent.click(screen.getByRole("button", { name: t.task.save }));
    expect(mocks.createTask).not.toHaveBeenCalled();
    expect(screen.getByText(t.errors.reminderDraftPending)).toBeTruthy();
    // 输入仍保留，未被静默丢弃
    expect((amount as HTMLInputElement).value).toBe("25");
  });

  it("未设截止时间：禁用自定义添加并说明原因", () => {
    render(<TaskFormModal task={null} onClose={() => {}} />);
    const amount = screen.getByLabelText(t.task.reminderAmountLabel);
    expect((amount as HTMLInputElement).disabled).toBe(true);
    expect((screen.getByRole("button", { name: t.task.reminderAdd }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByText(`（${t.task.reminderRequireDue}）`)).toBeTruthy();
  });

  it("新建任务继承设置中的自定义默认提醒", () => {
    mocks.settings = { ...mocks.settings!, default_reminder_offsets: [1500] };
    render(<TaskFormModal task={null} onClose={() => {}} />);
    // 1500 秒（25 分钟）作为默认值选中展示
    expect(screen.getByLabelText(`${t.task.reminderRemove} 提前 25 分钟`)).toBeTruthy();
  });

  it("移除一个自定义提醒后保存：其他提醒保留", async () => {
    const task = makeTask({ reminders: [600, 1500, 3600] });
    render(<TaskFormModal task={task} onClose={() => {}} />);
    await userEvent.click(screen.getByLabelText(`${t.task.reminderRemove} 提前 25 分钟`));

    await userEvent.click(screen.getByRole("button", { name: t.task.save }));
    const input = mocks.updateTask.mock.calls[0]![1] as { reminders: number[] };
    expect(input.reminders).toEqual([600, 3600]);
  });
});
