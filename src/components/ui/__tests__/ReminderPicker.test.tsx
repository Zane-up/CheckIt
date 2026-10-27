/**
 * ReminderPicker 回归：自定义提醒提前量。
 * - 单位换算与秒数保存（25 分钟 = 1500 秒）
 * - 去重：60 分钟 = 1 小时 = 3600 秒；自定义与预设重复
 * - 校验：空值 / 负数 / 小数 / 非数字 / 0 / 超大数值 / 60 天边界
 * - Enter 添加与输入法组合态；非预设值回显与移除
 */

import { useState } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ReminderPicker } from "@/components/ui/ReminderPicker";
import { t } from "@/i18n/zh";

/** 受控挂载：onChange 回写 state，模拟真实父组件行为 */
function setup(offsets0: number[] = [], disabled = false) {
  const onChange = vi.fn();
  const onDraftChange = vi.fn();
  function Harness() {
    const [offsets, setOffsets] = useState(offsets0);
    return (
      <ReminderPicker
        offsets={offsets}
        onChange={(next) => {
          onChange(next);
          setOffsets(next);
        }}
        disabled={disabled}
        onDraftChange={onDraftChange}
      />
    );
  }
  render(<Harness />);
  return {
    onChange,
    onDraftChange,
    amount: screen.getByLabelText(t.task.reminderAmountLabel),
    unit: screen.getByLabelText(t.task.reminderUnitLabel),
    add: screen.getByRole("button", { name: t.task.reminderAdd }),
  };
}

/** 通过自定义输入行添加一个提前量 */
async function addCustom(
  amount: HTMLElement,
  unit: HTMLElement,
  add: HTMLElement,
  value: string,
  unitValue: "minute" | "hour" | "day" = "minute",
) {
  await userEvent.clear(amount);
  await userEvent.type(amount, value);
  if ((unit as HTMLSelectElement).value !== unitValue) {
    fireEvent.change(unit, { target: { value: unitValue } });
  }
  fireEvent.click(add);
}

describe("ReminderPicker 自定义提前量", () => {
  it("添加 25 分钟：换算为 1500 秒并清空输入", async () => {
    const { onChange, amount, unit, add } = setup([]);
    await addCustom(amount, unit, add, "25");
    expect(onChange).toHaveBeenCalledWith([1500]);
    expect((amount as HTMLInputElement).value).toBe("");
    // 回显为“提前 25 分钟”标签（不把秒数当分钟显示）
    expect(screen.getByText("提前 25 分钟")).toBeTruthy();
  });

  it("60 分钟与 1 小时视为相同：已选 3600 时不重复添加", async () => {
    const { onChange, amount, unit, add } = setup([3600]);
    await addCustom(amount, unit, add, "60");
    expect(onChange).not.toHaveBeenCalled();
    expect((amount as HTMLInputElement).value).toBe("");
    // 预设“提前 1 小时”显示为已选中（附移除按钮），无重复标签
    expect(screen.getByLabelText(`${t.task.reminderRemove} 提前 1 小时`)).toBeTruthy();
  });

  it("自定义值与预设重复：不生成重复标签，预设直接选中", async () => {
    const { onChange, amount, unit, add } = setup([]);
    // 30 分钟 = 预设 1800 秒 → 选中该预设
    await addCustom(amount, unit, add, "30");
    expect(onChange).toHaveBeenLastCalledWith([1800]);
    expect(screen.getByLabelText(`${t.task.reminderRemove} 提前 30 分钟`)).toBeTruthy();
    // 1 小时 = 预设 3600 秒
    await addCustom(amount, unit, add, "1", "hour");
    expect(onChange).toHaveBeenLastCalledWith([1800, 3600]);
  });

  it("非法输入：就地提示并保留输入，不静默丢弃", async () => {
    const { onChange, amount, unit, add } = setup([]);
    for (const bad of ["-5", "2.5", "abc", "1e3"]) {
      await addCustom(amount, unit, add, bad);
      expect(screen.getByText(t.errors.reminderAmountInvalid)).toBeTruthy();
      expect((amount as HTMLInputElement).value).toBe(bad);
      expect(onChange).not.toHaveBeenCalled();
    }
    // 空值直接点添加
    await userEvent.clear(amount);
    fireEvent.click(add);
    expect(screen.getByText(t.errors.reminderAmountRequired)).toBeTruthy();
    expect(onChange).not.toHaveBeenCalled();
  });

  it("0 分钟提示使用“截止时”预设", async () => {
    const { onChange, amount, unit, add } = setup([]);
    await addCustom(amount, unit, add, "0");
    expect(screen.getByText(t.errors.reminderAmountZero)).toBeTruthy();
    expect((amount as HTMLInputElement).value).toBe("0");
    expect(onChange).not.toHaveBeenCalled();
  });

  it("60 天边界：60 天通过，61 天与超大数值拒绝", async () => {
    const { onChange, amount, unit, add } = setup([]);
    await addCustom(amount, unit, add, "60", "day");
    expect(onChange).toHaveBeenCalledWith([60 * 86400]);

    await addCustom(amount, unit, add, "61", "day");
    expect(screen.getByText(t.errors.reminderAmountTooLarge)).toBeTruthy();
    expect((amount as HTMLInputElement).value).toBe("61");

    await addCustom(amount, unit, add, "99999999999999999999", "day");
    expect(screen.getByText(t.errors.reminderAmountInvalid)).toBeTruthy();
    expect(onChange).toHaveBeenCalledTimes(1);
  });

  it("Enter 只添加自定义提醒；输入法组合态（keyCode 229）不触发", async () => {
    const { onChange, amount } = setup([]);
    await userEvent.type(amount, "25");
    expect(onChange).not.toHaveBeenCalled();

    fireEvent.keyDown(amount, { key: "Enter", keyCode: 229 });
    expect(onChange).not.toHaveBeenCalled();

    fireEvent.keyDown(amount, { key: "Enter" });
    expect(onChange).toHaveBeenCalledWith([1500]);
  });

  it("非预设值回显为自定义标签，附带移除按钮", () => {
    setup([1500]);
    expect(screen.getByText("提前 25 分钟")).toBeTruthy();
    expect(screen.getByLabelText(`${t.task.reminderRemove} 提前 25 分钟`)).toBeTruthy();
  });

  it("移除一个自定义提醒后其他提醒保留", async () => {
    const { onChange } = setup([600, 1500, 3600]);
    await userEvent.click(screen.getByLabelText(`${t.task.reminderRemove} 提前 25 分钟`));
    expect(onChange).toHaveBeenCalledWith([600, 3600]);
  });

  it("草稿回调：输入未添加为 true，添加或清空后为 false", async () => {
    const { onDraftChange, amount, add } = setup([]);
    await userEvent.type(amount, "25");
    expect(onDraftChange).toHaveBeenLastCalledWith(true);

    await userEvent.clear(amount);
    expect(onDraftChange).toHaveBeenLastCalledWith(false);

    await userEvent.type(amount, "25");
    fireEvent.click(add);
    expect(onDraftChange).toHaveBeenLastCalledWith(false);
    expect((amount as HTMLInputElement).value).toBe("");
  });

  it("禁用时输入行不可用，且残留草稿被清空", async () => {
    const onDraftChange = vi.fn();
    const onChange = vi.fn();
    function Harness({ disabled }: { disabled: boolean }) {
      const [offsets, setOffsets] = useState<number[]>([]);
      return (
        <ReminderPicker
          offsets={offsets}
          onChange={(next) => {
            onChange(next);
            setOffsets(next);
          }}
          disabled={disabled}
          onDraftChange={onDraftChange}
        />
      );
    }
    const { rerender } = render(<Harness disabled={false} />);
    const amount = screen.getByLabelText(t.task.reminderAmountLabel);
    await userEvent.type(amount, "25");
    expect(onDraftChange).toHaveBeenLastCalledWith(true);

    rerender(<Harness disabled />);
    expect((amount as HTMLInputElement).disabled).toBe(true);
    expect((amount as HTMLInputElement).value).toBe("");
    expect(onDraftChange).toHaveBeenLastCalledWith(false);
    expect((screen.getByRole("button", { name: t.task.reminderAdd }) as HTMLButtonElement).disabled).toBe(true);
  });
});
