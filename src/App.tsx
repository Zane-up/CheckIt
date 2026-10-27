/**
 * 应用入口：同一前端包服务两个窗口，按窗口 label 渲染对应界面。
 * - widget  : 桌面悬浮小组件（主界面）
 * - manager : 完整任务管理窗口
 *
 * 数据加载失败时展示错误与重试入口，绝不把失败静默渲染成"空数据正常态"。
 */

import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { RotateCw } from "lucide-react";
import { NowProvider } from "@/hooks/useNow";
import { useAppData, AppDataProvider } from "@/hooks/useAppData";
import { ThemeProvider } from "@/hooks/useTheme";
import WidgetApp from "@/components/widget/WidgetApp";
import ManagerApp from "@/components/manager/ManagerApp";
import { t } from "@/i18n/zh";

/** 数据门卫：首次加载才允许占满全屏；已加载后刷新失败只显示可重试提示，保留界面与草稿 */
export function DataGate({ children }: { children: React.ReactNode }) {
  const { loading, hasData, error, syncError, reload, retrySync } = useAppData();

  if (loading && !hasData) {
    return (
      <div className="flex h-full w-full items-center justify-center rounded-2xl bg-[var(--surface-overlay)]">
        <div className="h-5 w-5 animate-spin rounded-full border-2 border-[var(--glass-border)] border-t-[var(--accent)]" />
      </div>
    );
  }

  // 仅有首屏失败（从未拿到数据）才替换整个界面
  if (!hasData) {
    return (
      <div className="flex h-full w-full flex-col items-center justify-center gap-3 rounded-2xl border border-[var(--glass-border)] bg-[var(--surface-overlay)] px-8 text-center">
        <div className="text-[13px] text-[var(--urgent-text)]">{t.errors.loadFailed}</div>
        <div className="max-w-[260px] text-[12px] break-all text-[var(--text-secondary)]">
          {error ?? "未能连接到本地数据"}
        </div>
        <button
          type="button"
          onClick={() => {
            retrySync();
            void reload();
          }}
          className="flex items-center gap-1.5 rounded-lg border border-[var(--glass-border)] px-3 py-1.5 text-xs transition-colors hover:bg-[var(--surface-hover)]"
        >
          <RotateCw size={13} />
          {t.errors.retry}
        </button>
      </div>
    );
  }

  const bannerMessage = error ? t.errors.refreshFailed : syncError ? t.errors.syncUnavailable : null;
  const bannerDetail = error ?? syncError;

  return (
    <>
      {children}
      {bannerMessage && (
        <div
          role="alert"
          data-testid="data-error-banner"
          className="fixed inset-x-2 bottom-2 z-[100] flex items-center gap-2 rounded-xl border border-[rgba(248,113,113,0.35)] bg-[var(--banner-bg)] px-3 py-2 shadow-lg backdrop-blur"
        >
          <div className="min-w-0 flex-1">
            <div className="truncate text-[12px] text-[var(--banner-text)]">{bannerMessage}</div>
            {bannerDetail && (
              <div
                className="truncate text-[11px] text-[var(--banner-muted)]"
                title={bannerDetail}
              >
                {bannerDetail}
              </div>
            )}
          </div>
          <button
            type="button"
            onClick={() => {
              if (syncError) retrySync();
              if (error) void reload();
            }}
            className="flex shrink-0 items-center gap-1 rounded-lg border border-[rgba(248,113,113,0.45)] px-2 py-1 text-[12px] text-[var(--banner-text)] transition-colors hover:bg-[rgba(248,113,113,0.12)]"
          >
            <RotateCw size={12} />
            {t.errors.retry}
          </button>
        </div>
      )}
    </>
  );
}

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
      <DataProviderInner label={label} />
    </AppDataProvider>
  );
}

/** Theme/Now 依赖数据设置；错误态下也要有主题变量可用，因此顺序：DataGate 内外皆可 */
function DataProviderInner({ label }: { label: string }) {
  return (
    <ThemeProvider>
      <NowProvider>
        <DataGate>{label === "manager" ? <ManagerApp /> : <WidgetApp />}</DataGate>
      </NowProvider>
    </ThemeProvider>
  );
}
