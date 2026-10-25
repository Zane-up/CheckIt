/**
 * 主题上下文：将设置中的 theme / glass 参数应用到文档根节点。
 * - theme=dark|light|system：system 模式监听 prefers-color-scheme
 * - glass 参数写入 CSS 变量，供玻璃面板统一消费
 */

import { createContext, useContext, useEffect, useMemo, type ReactNode } from "react";
import { useAppData } from "@/hooks/useAppData";

interface ThemeValue {
  theme: "dark" | "light";
  mode: "dark" | "light" | "system";
  glassEnabled: boolean;
  glassOpacity: number;
}

const ThemeContext = createContext<ThemeValue>({
  theme: "dark",
  mode: "dark",
  glassEnabled: true,
  glassOpacity: 0.35,
});

function systemPrefersDark(): boolean {
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const { state } = useAppData();
  const { theme: mode, glass_enabled, glass_opacity } = state.settings;

  const effective = mode === "system" ? (systemPrefersDark() ? "dark" : "light") : mode;

  useEffect(() => {
    if (mode !== "system") return;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => document.documentElement.classList.toggle("dark", mq.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, [mode]);

  useEffect(() => {
    document.documentElement.classList.toggle("dark", effective === "dark");
  }, [effective]);

  useEffect(() => {
    const root = document.documentElement;
    root.classList.toggle("no-glass", !glass_enabled);
    root.style.setProperty("--glass-opacity", String(glass_opacity));
  }, [glass_enabled, glass_opacity]);

  const value = useMemo(
    () => ({ theme: effective, mode, glassEnabled: glass_enabled, glassOpacity: glass_opacity }),
    [effective, mode, glass_enabled, glass_opacity],
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme(): ThemeValue {
  return useContext(ThemeContext);
}
