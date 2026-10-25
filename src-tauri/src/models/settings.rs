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
