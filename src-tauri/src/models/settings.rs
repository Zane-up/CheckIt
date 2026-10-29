//! 应用设置模型。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AppSettings {
    #[serde(default = "default_true")]
    pub auto_backup_enabled: bool,
    #[serde(default = "default_backup_keep")]
    pub auto_backup_keep: i64,
    #[serde(default = "default_language")]
    pub language: String,
    /// dark | light | system
    pub theme: String,
    pub glass_enabled: bool,
    /// 0.05 ~ 0.6
    pub glass_opacity: f64,
    pub always_on_top: bool,
    pub close_to_tray: bool,
    pub notifications_enabled: bool,
    /// 旧版本升级后默认关闭；可独立于系统通知开启。
    #[serde(default)]
    pub popup_reminders_enabled: bool,
    pub default_reminder_offsets: Vec<i64>,
    pub widget_display_count: i64,
    pub launch_at_login: bool,
    /// 拖到屏幕边缘时收纳为圆圈（默认开启）
    #[serde(default = "default_true")]
    pub edge_dock_enabled: bool,
}

/// 旧库 / 旧前端载荷缺少该键时默认开启
fn default_true() -> bool {
    true
}

fn default_backup_keep()->i64 { 7 }

fn default_language() -> String { "zh-CN".into() }

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            auto_backup_enabled: true,
            auto_backup_keep: 7,
            language: default_language(),
            theme: "dark".into(),
            glass_enabled: true,
            glass_opacity: 0.35,
            always_on_top: true,
            close_to_tray: true,
            notifications_enabled: true,
            popup_reminders_enabled: false,
            default_reminder_offsets: vec![3600, 600],
            widget_display_count: 8,
            launch_at_login: false,
            edge_dock_enabled: true,
        }
    }
}

impl AppSettings {
    pub fn reminders_enabled(&self) -> bool {
        self.notifications_enabled || self.popup_reminders_enabled
    }

    /// 校验并夹紧取值范围，防止持久化非法配置。
    pub fn sanitized(mut self) -> Self {
        self.auto_backup_keep=self.auto_backup_keep.clamp(3,30);
        if self.language != "en" { self.language = default_language(); }
        self.theme = match self.theme.as_str() {
            "light" => "light".into(),
            "system" => "system".into(),
            _ => "dark".into(),
        };
        self.glass_opacity = self.glass_opacity.clamp(0.05, 0.6);
        self.widget_display_count = self.widget_display_count.clamp(3, 20);
        self.default_reminder_offsets = crate::models::task::normalize_reminder_offsets(
            &self.default_reminder_offsets,
        );
        self
    }
}

/// 设置补丁：只覆盖前端传入的字段（Option = 不修改）。
///
/// 补丁必须由数据库层在**同一个受保护操作**内完成"读取当前值 → 应用 → 保存"，
/// 否则并发写入（例如托盘置顶切换与设置面板保存）会互相覆盖未修改的字段。
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SettingsPatch {
    pub auto_backup_enabled: Option<bool>,
    pub auto_backup_keep: Option<i64>,
    pub language: Option<String>,
    pub theme: Option<String>,
    pub glass_enabled: Option<bool>,
    pub glass_opacity: Option<f64>,
    pub always_on_top: Option<bool>,
    pub close_to_tray: Option<bool>,
    pub notifications_enabled: Option<bool>,
    pub popup_reminders_enabled: Option<bool>,
    pub default_reminder_offsets: Option<Vec<i64>>,
    pub widget_display_count: Option<i64>,
    pub launch_at_login: Option<bool>,
    pub edge_dock_enabled: Option<bool>,
}

impl SettingsPatch {
    pub fn apply_to(&self, s: &mut AppSettings) {
        if let Some(v)=self.auto_backup_enabled{s.auto_backup_enabled=v;}
        if let Some(v)=self.auto_backup_keep{s.auto_backup_keep=v;}
        if let Some(v) = &self.language { s.language = v.clone(); }
        if let Some(v) = &self.theme {
            s.theme = v.clone();
        }
        if let Some(v) = self.glass_enabled {
            s.glass_enabled = v;
        }
        if let Some(v) = self.glass_opacity {
            s.glass_opacity = v;
        }
        if let Some(v) = self.always_on_top {
            s.always_on_top = v;
        }
        if let Some(v) = self.close_to_tray {
            s.close_to_tray = v;
        }
        if let Some(v) = self.notifications_enabled {
            s.notifications_enabled = v;
        }
        if let Some(v) = self.popup_reminders_enabled {
            s.popup_reminders_enabled = v;
        }
        if let Some(v) = &self.default_reminder_offsets {
            s.default_reminder_offsets = v.clone();
        }
        if let Some(v) = self.widget_display_count {
            s.widget_display_count = v;
        }
        if let Some(v) = self.launch_at_login {
            s.launch_at_login = v;
        }
        if let Some(v) = self.edge_dock_enabled {
            s.edge_dock_enabled = v;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_settings_keep_popup_disabled_and_each_channel_can_work_alone() {
        let mut legacy = serde_json::to_value(AppSettings::default()).unwrap();
        legacy.as_object_mut().unwrap().remove("popup_reminders_enabled");
        let mut settings: AppSettings = serde_json::from_value(legacy).unwrap();
        assert!(!settings.popup_reminders_enabled);
        assert!(settings.reminders_enabled());
        settings.notifications_enabled = false;
        assert!(!settings.reminders_enabled());
        settings.popup_reminders_enabled = true;
        assert!(settings.reminders_enabled());
    }

    #[test]
    fn language_defaults_and_validation_are_backward_compatible() {
        let mut legacy=serde_json::to_value(AppSettings::default()).unwrap();
        legacy.as_object_mut().unwrap().remove("language");
        let settings:AppSettings=serde_json::from_value(legacy).unwrap();
        assert_eq!(settings.language,"zh-CN");
        let mut invalid=settings.clone();invalid.language="unsupported".into();
        assert_eq!(invalid.sanitized().language,"zh-CN");
    }

    #[test]
    fn sanitize_clamps_opacity() {
        let mut s = AppSettings::default();
        s.glass_opacity = 1.5;
        s.widget_display_count = 100;
        let s = s.sanitized();
        assert_eq!(s.glass_opacity, 0.6);
        assert_eq!(s.widget_display_count, 20);
    }

    #[test]
    fn sanitize_theme_fallback() {
        let mut s = AppSettings::default();
        s.theme = "blue".into();
        assert_eq!(s.sanitized().theme, "dark");
    }
}
