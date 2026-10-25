/**
 * 应用入口：同一前端包服务两个窗口，按窗口 label 渲染对应界面。
 * - widget  : 桌面悬浮小组件（主界面）
 * - manager : 完整任务管理窗口
 */

import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { NowProvider } from "@/hooks/useNow";
import { AppDataProvider } from "@/hooks/useAppData";
import { ThemeProvider } from "@/hooks/useTheme";
import WidgetApp from "@/components/widget/WidgetApp";
import ManagerApp from "@/components/manager/ManagerApp";

export default function App() {
  const [label, setLabel] = useState<string | null>(null);

  useEffect(() => {
    setLabel(getCurrentWindow().label);
  }, []);

  if (label === null) {
    // 窗口 label 尚未就绪：渲染空白避免闪烁
    return <div className="h-full w-full" />;
  }

  return (
    <AppDataProvider>
      <ThemeProvider>
        <NowProvider>{label === "manager" ? <ManagerApp /> : <WidgetApp />}</NowProvider>
      </ThemeProvider>
    </AppDataProvider>
  );
}
