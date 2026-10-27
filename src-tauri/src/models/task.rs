//! 任务模型：字段与前端 Task 接口对应。

use serde::{Deserialize, Serialize};

pub const PRIORITIES: [&str; 3] = ["high", "medium", "low"];
pub const STATUSES: [&str; 2] = ["pending", "completed"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    High,
    Medium,
    Low,
}

impl Priority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Priority::High => "high",
            Priority::Medium => "medium",
            Priority::Low => "low",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "high" => Some(Priority::High),
            "medium" => Some(Priority::Medium),
            "low" => Some(Priority::Low),
            _ => None,
        }
    }

    /// 优先级排序权重（high < medium < low）
    pub fn rank(&self) -> i32 {
        match self {
            Priority::High => 0,
            Priority::Medium => 1,
            Priority::Low => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Completed,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Pending => "pending",
            TaskStatus::Completed => "completed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub category_id: Option<i64>,
    pub priority: Priority,
    pub status: TaskStatus,
    /// RFC3339 UTC；None 表示无截止时间
    pub due_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    /// 该任务配置的提醒提前秒数列表（0 = 截止时提醒）
    pub reminders: Vec<i64>,
}

/// 前端创建/编辑任务的载荷
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TaskInput {
    pub title: String,
    pub description: String,
    pub category_id: Option<i64>,
    pub priority: Priority,
    pub due_at: Option<String>,
    pub reminders: Vec<i64>,
}

/// 规范化提醒偏移：去重、去负数、去超过 60 天的值、升序。
/// 与前端 normalizeOffsets 语义一致。
pub fn normalize_reminder_offsets(offsets: &[i64]) -> Vec<i64> {
    let mut out: Vec<i64> = offsets
        .iter()
        .copied()
        .filter(|&o| o >= 0 && o <= 60 * 86400)
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_dedup_and_sorts() {
        assert_eq!(normalize_reminder_offsets(&[3600, 600, 3600, 0]), vec![0, 600, 3600]);
    }

    #[test]
    fn normalize_filters_invalid() {
        assert!(normalize_reminder_offsets(&[-5, 61 * 86400]).is_empty());
    }

    /// 60 天为闭边界：恰好 60 天保留，超出 1 秒被滤除（自定义输入范围依据）。
    #[test]
    fn normalize_boundary_sixty_days() {
        assert_eq!(normalize_reminder_offsets(&[60 * 86400]), vec![60 * 86400]);
        assert!(normalize_reminder_offsets(&[60 * 86400 + 1]).is_empty());
    }

    #[test]
    fn priority_roundtrip() {
        for p in PRIORITIES {
            let parsed = Priority::parse(p).unwrap();
            assert_eq!(parsed.as_str(), p);
        }
        assert!(Priority::parse("urgent").is_none());
    }
}
