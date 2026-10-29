import { tr, useI18n } from "@/i18n";
/**
 * 设置面板：外观 / 行为 / 提醒 / 数据。
 * 所有修改即时持久化到 SQLite（updateSettings）。
 */

import { useEffect, useState } from "react";
import { useAppData } from "@/hooks/useAppData";
import { t } from "@/i18n";
import type { ThemeMode } from "@/types";
import { ReminderPicker } from "@/components/ui/ReminderPicker";
import { api } from "@/services/api";
import UpdateChecker from "./UpdateChecker";
import BackupPanel from "./BackupPanel";

function Toggle({ checked, onChange, disabled, label }: { checked: boolean; onChange: (v: boolean) => void; disabled?: boolean; label?: string }) {
  useI18n();
  return (
    <button
      type="button"
      role="switch"
      aria-label={label}
      aria-checked={checked}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={`relative h-[20px] w-[36px] shrink-0 rounded-full transition-colors disabled:opacity-40 ${
        checked ? "bg-[var(--btn-primary-bg)]" : "bg-[var(--surface-active)]"
      }`}
    >
      <span
        className="absolute top-[2px] h-4 w-4 rounded-full bg-white shadow transition-all"
        style={{ left: checked ? 18 : 2 }}
      />
    </button>
  );
}

function Row({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  useI18n();
  return (
    <div className="glass-subtle flex items-center justify-between gap-4 rounded-xl px-4 py-2.5">
      <div className="min-w-0">
        <div className="text-[13px]">{label}</div>
        {hint && <div className="mt-0.5 text-[12px] leading-snug text-[var(--text-muted)]">{hint}</div>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

export default function SettingsPanel() {
  useI18n();
  const { state, updateSettings } = useAppData();
  const s = state.settings;
  const [dbPath, setDbPath] = useState("");
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);

  useEffect(() => {
    void api.getDbPath().then(setDbPath).catch(() => setDbPath(""));
  }, []);

  const set = (patch: Parameters<typeof updateSettings>[0]) => {
    setSaveError(null);
    void updateSettings(patch).catch((e) => setSaveError(tr("保存设置失败，请重试：{0}", e instanceof Error ? e.message : String(e))));
  };

  return (
    <div className="mx-auto w-full max-w-xl overflow-y-auto px-6 py-5">
      <h1 className="mb-4 text-[15px] font-semibold">{t.settings.title}</h1>
      <div className="mb-4">
        <Row label="语言 / Language">
          <select aria-label="语言 / Language" value={s.language ?? "zh-CN"} onChange={e => set({language:e.target.value as "zh-CN"|"en"})} className="field-input !w-36">
            <option value="zh-CN">简体中文</option><option value="en">English</option>
          </select>
        </Row>
      </div>
      {saveError && <div role="alert" className="mb-3 text-sm text-[var(--urgent-text)]">{saveError}</div>}

      <section className="mb-5">
        <h2 className="mb-2 text-[11px] font-medium uppercase tracking-wider text-[var(--text-secondary)]">{t.settings.appearance}</h2>
        <div className="flex flex-col gap-1.5">
          <Row label={t.settings.theme}>
            <div className="flex gap-1">
              {(["dark", "light", "system"] as ThemeMode[]).map((m) => (
                <button
                  key={m}
                  type="button"
                  onClick={() => set({ theme: m })}
                  className={`rounded-lg border px-2.5 py-1 text-xs transition-all ${
                    s.theme === m
                      ? "border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent-text)]"
                      : "border-[var(--glass-border)] text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]"
                  }`}
                >
                  {m === "dark" ? t.settings.themeDark : m === "light" ? t.settings.themeLight : t.settings.themeSystem}
                </button>
              ))}
            </div>
          </Row>
          <Row label={t.settings.glass} hint={t.settings.glassHint}>
            <Toggle label={t.settings.glass} checked={s.glass_enabled} onChange={(v) => set({ glass_enabled: v })} />
          </Row>
          <Row label={t.settings.glassOpacity} hint={tr("悬浮面板始终使用不透明底色，以保持圆角完整和文字清晰。")}>
            <input
              type="range"
              aria-label={t.settings.glassOpacity}
              disabled={!s.glass_enabled}
              min={0.05}
              max={0.6}
              step={0.05}
              value={s.glass_opacity}
              onChange={(e) => set({ glass_opacity: Number(e.target.value) })}
              className="w-32 accent-[var(--accent)]"
            />
          </Row>
        </div>
      </section>

      <section className="mb-5">
        <h2 className="mb-2 text-[11px] font-medium uppercase tracking-wider text-[var(--text-secondary)]">{t.settings.behavior}</h2>
        <div className="flex flex-col gap-1.5">
          <Row label={t.settings.alwaysOnTop}>
            <Toggle checked={s.always_on_top} onChange={(v) => set({ always_on_top: v })} />
          </Row>
          <Row label={t.settings.closeToTray} hint={t.settings.closeToTrayHint}>
            <Toggle checked={s.close_to_tray} onChange={(v) => set({ close_to_tray: v })} />
          </Row>
          <Row label={t.settings.edgeDock} hint={t.settings.edgeDockHint}>
            <Toggle checked={s.edge_dock_enabled} onChange={(v) => set({ edge_dock_enabled: v })} />
          </Row>
          <Row label={t.settings.launchAtLogin}>
            <Toggle checked={s.launch_at_login} onChange={(v) => set({ launch_at_login: v })} />
          </Row>
        </div>
      </section>

      <section className="mb-5">
        <h2 className="mb-2 text-[11px] font-medium uppercase tracking-wider text-[var(--text-secondary)]">{t.settings.reminders}</h2>
        <div className="flex flex-col gap-1.5">
          <Row label={t.settings.notifications}>
            <Toggle label={t.settings.notifications} checked={s.notifications_enabled} onChange={(v) => set({ notifications_enabled: v })} />
          </Row>
          <Row label={t.settings.popupReminders} hint={t.settings.popupRemindersHint}>
            <Toggle label={t.settings.popupReminders} checked={s.popup_reminders_enabled} onChange={(v) => set({ popup_reminders_enabled: v })} />
          </Row>
          <div className="px-1 py-1">
            <button type="button" className="rounded-lg border border-[var(--glass-border)] px-3 py-1.5 text-xs hover:bg-[var(--surface-hover)]"
              onClick={() => {
                setPreviewError(null);
                void api.previewPopupReminder().catch((e) => setPreviewError(String(e)));
              }}>
              {t.settings.previewPopup}
            </button>
            {previewError && <div role="alert" className="mt-1 text-xs text-[var(--urgent-text)]">{previewError}</div>}
          </div>
          <Row label={t.settings.widgetCount}>
            <input
              type="number"
              min={3}
              max={20}
              value={s.widget_display_count}
              onChange={(e) => set({ widget_display_count: Math.min(20, Math.max(3, Number(e.target.value) || 8)) })}
              className="field-input !w-20 text-center"
            />
          </Row>
          <div className="glass-subtle rounded-xl px-4 py-2.5">
            <div className="mb-1 text-[13px]">{tr("新任务默认提醒")}</div>
            <div className="mb-2 text-[12px] leading-snug text-[var(--text-muted)]">{t.settings.defaultRemindersHint}</div>
            <ReminderPicker
              offsets={s.default_reminder_offsets}
              onChange={(offsets) => set({ default_reminder_offsets: offsets })}
            />
          </div>
        </div>
      </section>

      <section>
        <h2 className="mb-2 text-[11px] font-medium uppercase tracking-wider text-[var(--text-secondary)]">{t.settings.data}</h2>
        <div className="glass-subtle flex items-center justify-between gap-3 rounded-xl px-4 py-2.5">
          <div className="min-w-0">
            <div className="text-[13px]">{t.settings.dbPath}</div>
            <div className="selectable mt-0.5 truncate text-[12px] text-[var(--text-muted)]" title={dbPath}>
              {dbPath || "…"}
            </div>
          </div>
          <button
            type="button"
            className="shrink-0 text-xs text-[var(--accent-text)] hover:underline"
            onClick={() => void api.getDbPath().then((p) => navigator.clipboard.writeText(p))}
          >
            {tr("复制路径")}</button>
        </div>
      </section>

      <BackupPanel autoEnabled={s.auto_backup_enabled} keep={s.auto_backup_keep} onChange={set}/>
      <UpdateChecker />
    </div>
  );
}
