//! 应用设置模型。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AppSettings {
    /// dark | light | system
    pub theme: String,
    pub glass_enabled: bool,
    /// 0.05 ~ 0.6
    pub glass_opacity: f64,
    pub always_on_top: bool,
    pub close_to_tray: bool,
    pub notifications_enabled: bool,
    pub default_reminder_offsets: Vec<i64>,
    pub widget_display_count: i64,
    pub launch_at_login: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            glass_enabled: true,
            glass_opacity: 0.35,
            always_on_top: true,
            close_to_tray: true,
            notifications_enabled: true,
            default_reminder_offsets: vec![3600, 600],
            widget_display_count: 8,
            launch_at_login: false,
        }
    }
}

impl AppSettings {
    /// 校验并夹紧取值范围，防止持久化非法配置。
    pub fn sanitized(mut self) -> Self {
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
    pub theme: Option<String>,
    pub glass_enabled: Option<bool>,
    pub glass_opacity: Option<f64>,
    pub always_on_top: Option<bool>,
    pub close_to_tray: Option<bool>,
    pub notifications_enabled: Option<bool>,
    pub default_reminder_offsets: Option<Vec<i64>>,
    pub widget_display_count: Option<i64>,
    pub launch_at_login: Option<bool>,
}

impl SettingsPatch {
    pub fn apply_to(&self, s: &mut AppSettings) {
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
        if let Some(v) = &self.default_reminder_offsets {
            s.default_reminder_offsets = v.clone();
        }
        if let Some(v) = self.widget_display_count {
            s.widget_display_count = v;
        }
        if let Some(v) = self.launch_at_login {
            s.launch_at_login = v;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
