/**
 * 全局共享时钟：整个应用只创建一个 1 秒间隔的定时器，
 * 所有倒计时组件通过 Context 读取同一 now 值，
 * 避免为每个任务创建独立高频定时器。
 */

import { createContext, useContext, useEffect, useState, type ReactNode } from "react";

const NowContext = createContext<number>(Date.now());

export function NowProvider({ children }: { children: ReactNode }) {
  // 每秒读取真实系统时间（而非递减缓存值），天然处理系统时间调整/休眠恢复
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    // 窗口从后台恢复可见时立即刷新一次（休眠/挂起后 setInoterval 可能漂移）
    const refresh = () => setNow(Date.now());
    document.addEventListener("visibilitychange", refresh);
    window.addEventListener("focus", refresh);
    return () => {
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", refresh);
      window.removeEventListener("focus", refresh);
    };
  }, []);

  return <NowContext.Provider value={now}>{children}</NowContext.Provider>;
}

export function useNow(): number {
  return useContext(NowContext);
}
