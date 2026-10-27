/**
 * Vitest 全局环境准备（仅测试使用）。
 * - 每个用例后卸载 React 树，避免监听器/定时器泄漏
 * - jsdom 缺少的浏览器 API 提供最小实现
 */

import { afterEach } from "vitest";
import { cleanup } from "@testing-library/react";

// React 19 的 act 环境标记
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  cleanup();
});

if (!window.matchMedia) {
  Object.defineProperty(window, "matchMedia", {
    writable: true,
    value: (query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    }),
  });
}

if (!globalThis.ResizeObserver) {
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof ResizeObserver;
}

if (!window.scrollTo) {
  window.scrollTo = () => {};
}
