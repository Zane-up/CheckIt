import { describe, expect, it } from "vitest";
import { BUBBLE_COUNT_CAP, bubbleTooltip, formatBubbleCount } from "@/utils/bubble";

describe("formatBubbleCount", () => {
  it("0 显示 0", () => {
    expect(formatBubbleCount(0)).toBe("0");
  });

  it("个位数与两位数原样显示", () => {
    expect(formatBubbleCount(1)).toBe("1");
    expect(formatBubbleCount(9)).toBe("9");
    expect(formatBubbleCount(42)).toBe("42");
    expect(formatBubbleCount(99)).toBe("99");
  });

  it("超过 99 显示 99+", () => {
    expect(formatBubbleCount(100)).toBe("99+");
    expect(formatBubbleCount(1234)).toBe("99+");
    expect(formatBubbleCount(1090)).toBe(`${BUBBLE_COUNT_CAP}+`);
  });

  it("负数与非法值按 0 处理（不显示 -1 之类）", () => {
    expect(formatBubbleCount(-3)).toBe("0");
    expect(formatBubbleCount(Number.NaN)).toBe("0");
    expect(formatBubbleCount(Number.POSITIVE_INFINITY)).toBe("0");
  });
});

describe("bubbleTooltip", () => {
  it("显示完整数量，不做 99+ 截断", () => {
    expect(bubbleTooltip(0)).toContain("暂无");
    expect(bubbleTooltip(7)).toContain("7");
    expect(bubbleTooltip(1234)).toContain("1234");
    expect(bubbleTooltip(1234)).not.toContain("99+");
  });
});
