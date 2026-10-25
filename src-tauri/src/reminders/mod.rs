//! 提醒调度引擎。
//!
//! 核心设计（可靠性要求）：
//! 1. 基于真实绝对时间判断：每次扫描都重新计算 due_at - offset 与当前时间的差值，
//!    不依赖任何增量计数器，因此系统休眠恢复 / 长时间后台 / 重启后都能正确补查。
//! 2. 幂等去重：以 SQLite reminder_events 表的 (task_id, offset_seconds) 唯一约束为准，
//!    触发前先 INSERT OR IGNORE，插入成功才通知 —— 轮询、重启、窗口刷新都不会重复弹通知。
//! 3. 补发窗口：错过的提醒若超过 MISSED_GRACE（15 分钟）则静默标记为已触发，
//!    避免休眠唤醒 / 重启后连续弹出大量历史通知；窗口内的照常提醒。

use chrono::{DateTime, Utc};

use crate::database::{Database, DbResult};
use crate::models::task::TaskStatus;

/// 错过提醒的静默补记窗口：超过该时长的提醒只记录不弹通知。
pub const MISSED_GRACE_SECS: i64 = 15 * 60;

/// 单条提醒动作
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReminderAction {
    /// 弹出系统通知
    Notify { task_id: i64, offset_seconds: i64 },
    /// 错过太久，仅静默记录
    MarkMissed { task_id: i64, offset_seconds: i64 },
}

/// 提醒引擎：持有数据库引用，可反复运行（纯逻辑，可测试）。
pub struct ReminderEngine {
    db: Database,
}

impl ReminderEngine {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 数据库访问（供调度线程读取任务详情 / 设置）
    pub fn db(&self) -> &Database {
        &self.db
    }

    /// 扫描一次全部任务，返回本轮应执行的提醒动作。
    ///
    /// - 已完成任务跳过（不再触发任何提醒）
    /// - 未来才触发的提醒跳过
    /// - 已触发过（reminder_events 有记录）的跳过 —— 幂等
    /// - 错过时间在 MISSED_GRACE 内 → Notify；超出 → MarkMissed
    pub fn run_once(&self, now: DateTime<Utc>) -> DbResult<Vec<ReminderAction>> {
        let tasks = self.db.list_schedulable_tasks()?;
        let mut actions = Vec::new();

        for task in tasks {
            if task.status != TaskStatus::Pending {
                continue;
            }
            let due_at = match task.due_at.as_deref().and_then(parse_rfc3339) {
                Some(d) => d,
                None => continue,
            };

            for &offset in &task.reminders {
                let scheduled = due_at - chrono::Duration::seconds(offset);
                if scheduled > now {
                    // 还没到触发时间
                    continue;
                }
                // 幂等检查 + 抢占记录（唯一约束保证并发下也只成功一次）
                if !self.db.record_reminder_fired(task.id, offset)? {
                    continue;
                }
                let late_secs = (now - scheduled).num_seconds();
                if late_secs > MISSED_GRACE_SECS {
                    actions.push(ReminderAction::MarkMissed { task_id: task.id, offset_seconds: offset });
                } else {
                    actions.push(ReminderAction::Notify { task_id: task.id, offset_seconds: offset });
                }
            }
        }

        Ok(actions)
    }
}

/// 解析 RFC3339 UTC 时间字符串
pub fn parse_rfc3339(s: &str) -> Option<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

/// 生成提醒正文里的剩余时间描述（与前端 countdown 文案一致）
pub fn remaining_text(due_at: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = (due_at - now).num_seconds();
    let abs = secs.abs();
    let prefix = if secs < 0 { "已逾期 " } else { "" };

    if abs < 60 {
        format!("{prefix}{abs}秒").trim_start().to_string()
    } else if abs < 3600 {
        format!("{prefix}{}分钟", abs / 60)
    } else if abs < 86400 {
        format!("{prefix}{}小时{}分钟", abs / 3600, (abs % 3600) / 60)
    } else {
        format!("{prefix}{}天{}小时", abs / 86400, (abs % 86400) / 3600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::task::{Priority, TaskInput};

    fn db() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.ensure_default_categories().unwrap();
        db
    }

    fn input(title: &str, due: Option<&str>, reminders: Vec<i64>) -> TaskInput {
        TaskInput {
            title: title.into(),
            description: String::new(),
            category_id: None,
            priority: Priority::Medium,
            due_at: due.map(|s| s.into()),
            reminders,
        }
    }

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc::now() + chrono::Duration::seconds(secs)
    }

    #[test]
    fn fires_when_due_time_reached() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        // 截止在 59 分钟后，配置提前 1 小时提醒 => 调度时刻在 1 分钟前，已到期且在宽限窗口内
        let due = ts(3540);
        let task = db
            .create_task(&input("提交作业", Some(&due.to_rfc3339()), vec![3600]))
            .unwrap();

        let actions = engine.run_once(Utc::now()).unwrap();
        assert_eq!(
            actions,
            vec![ReminderAction::Notify { task_id: task.id, offset_seconds: 3600 }]
        );
    }

    #[test]
    fn no_fire_before_scheduled_time() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        // 截止在 3 小时后，提前 1 小时提醒 => 2 小时后才触发
        let due = ts(3 * 3600);
        db.create_task(&input("未到期任务", Some(&due.to_rfc3339()), vec![3600]))
            .unwrap();

        let actions = engine.run_once(Utc::now()).unwrap();
        assert!(actions.is_empty());
    }

    #[test]
    fn idempotent_no_double_fire() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = ts(600);
        db.create_task(&input("幂等任务", Some(&due.to_rfc3339()), vec![3600]))
            .unwrap();

        let first = engine.run_once(Utc::now()).unwrap();
        assert_eq!(first.len(), 1);
        // 第二次 / 第三次扫描不应再次触发（轮询、窗口重开、重启模拟）
        assert!(engine.run_once(Utc::now()).unwrap().is_empty());
        assert!(engine.run_once(Utc::now()).unwrap().is_empty());
    }

    #[test]
    fn completed_task_stops_reminders() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = ts(3540);
        let task = db
            .create_task(&input("完成任务测试", Some(&due.to_rfc3339()), vec![3600]))
            .unwrap();
        db.set_task_status(task.id, true).unwrap();

        assert!(engine.run_once(Utc::now()).unwrap().is_empty());
    }

    #[test]
    fn restore_pending_rearms_future_reminders() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        // 截止在 2 小时后：提前 1 小时提醒（未来触发）+ 提前 3 小时提醒（已到触发时间）
        let due = ts(2 * 3600);
        let task = db
            .create_task(&input("恢复任务", Some(&due.to_rfc3339()), vec![3600, 3 * 3600]))
            .unwrap();

        // 第一次扫描：3小时提前的提醒触发
        let actions = engine.run_once(Utc::now()).unwrap();
        assert_eq!(actions.len(), 1);

        // 完成任务 —— 停止一切提醒
        db.set_task_status(task.id, true).unwrap();
        assert!(engine.run_once(Utc::now()).unwrap().is_empty());

        // 恢复为待办：已过触发时间的提醒不会重复弹（事件仍记录），
        // 但未来触发的提醒（提前 1 小时）仍然有效
        db.set_task_status(task.id, false).unwrap();
        let mid = engine.run_once(Utc::now()).unwrap();
        assert!(mid.is_empty(), "恢复后不应立即重弹已触发提醒");
        // 模拟 1 小时过去
        let later = Utc::now() + chrono::Duration::seconds(3600 + 60);
        let actions = engine.run_once(later).unwrap();
        assert_eq!(
            actions,
            vec![ReminderAction::Notify { task_id: task.id, offset_seconds: 3600 }]
        );
    }

    #[test]
    fn due_change_recomputes_reminders() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due_old = ts(1800); // 30 分钟后
        let task = db
            .create_task(&input("改期任务", Some(&due_old.to_rfc3339()), vec![3600]))
            .unwrap();

        // 旧截止的提醒已触发
        assert_eq!(engine.run_once(Utc::now()).unwrap().len(), 1);

        // 修改截止时间为 2 天后：旧触发记录被清除，新提醒在未来触发
        let due_new = ts(2 * 86400);
        let updated = db
            .update_task(
                task.id,
                &input("改期任务", Some(&due_new.to_rfc3339()), vec![3600]),
            )
            .unwrap();
        assert_eq!(updated.due_at, Some(due_new.to_rfc3339()));

        assert!(engine.run_once(Utc::now()).unwrap().is_empty());
        // 模拟接近新截止（提前 1 小时窗口内）
        let near = Utc::now() + chrono::Duration::seconds(2 * 86400 - 3500);
        let actions = engine.run_once(near).unwrap();
        assert_eq!(
            actions,
            vec![ReminderAction::Notify { task_id: task.id, offset_seconds: 3600 }]
        );
    }

    #[test]
    fn missed_reminder_beyond_grace_is_silent() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        // 截止在 1 小时后，提前 1 天提醒 => 调度时刻在 23 小时前（错过远超宽限窗口）
        let due = ts(3600);
        db.create_task(&input("错过任务", Some(&due.to_rfc3339()), vec![86400]))
            .unwrap();

        let actions = engine.run_once(Utc::now()).unwrap();
        assert!(matches!(actions[0], ReminderAction::MarkMissed { .. }));
        // 静默标记后不再重复
        assert!(engine.run_once(Utc::now()).unwrap().is_empty());
    }

    #[test]
    fn missed_within_grace_still_notifies() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        // 调度时刻在 10 分钟前（错过但在 15 分钟宽限内）
        let due = ts(3600 - 600);
        db.create_task(&input("宽限任务", Some(&due.to_rfc3339()), vec![3600]))
            .unwrap();

        let actions = engine.run_once(Utc::now()).unwrap();
        assert!(matches!(actions[0], ReminderAction::Notify { .. }));
    }

    #[test]
    fn restart_recovers_pending_reminders() {
        // 模拟重启：新开引擎实例（同一数据库），未触发的提醒仍然生效
        let db = db();
        let due = ts(3540); // 59 分钟后
        let task = db
            .create_task(&input("重启任务", Some(&due.to_rfc3339()), vec![3600]))
            .unwrap();

        // "重启"：新建引擎
        let engine = ReminderEngine::new(db.clone());
        let actions = engine.run_once(Utc::now()).unwrap();
        assert_eq!(
            actions,
            vec![ReminderAction::Notify { task_id: task.id, offset_seconds: 3600 }]
        );
    }

    #[test]
    fn multiple_offsets_fire_independently() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        // 截止在 20 分钟后；提前 30 分钟（已到期）与提前 10 分钟（未来）都配置
        let due = ts(1200);
        let task = db
            .create_task(&input("多级提醒", Some(&due.to_rfc3339()), vec![1800, 600]))
            .unwrap();

        let actions = engine.run_once(Utc::now()).unwrap();
        assert_eq!(
            actions,
            vec![ReminderAction::Notify { task_id: task.id, offset_seconds: 1800 }]
        );

        // 模拟 11 分钟后：提前 10 分钟的提醒到期
        let later = Utc::now() + chrono::Duration::seconds(660);
        let actions = engine.run_once(later).unwrap();
        assert_eq!(
            actions,
            vec![ReminderAction::Notify { task_id: task.id, offset_seconds: 600 }]
        );
    }

    #[test]
    fn reminder_fires_at_deadline_for_zero_offset() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = ts(-30); // 截止时刻已过 30 秒
        let task = db
            .create_task(&input("截止提醒", Some(&due.to_rfc3339()), vec![0]))
            .unwrap();

        let actions = engine.run_once(Utc::now()).unwrap();
        assert_eq!(
            actions,
            vec![ReminderAction::Notify { task_id: task.id, offset_seconds: 0 }]
        );
    }

    #[test]
    fn remaining_text_formatting() {
        let now = Utc::now();
        assert_eq!(remaining_text(now + chrono::Duration::seconds(45), now), "45秒");
        assert_eq!(remaining_text(now + chrono::Duration::seconds(300), now), "5分钟");
        assert_eq!(
            remaining_text(now + chrono::Duration::seconds(3 * 3600 + 20 * 60), now),
            "3小时20分钟"
        );
        assert_eq!(
            remaining_text(now + chrono::Duration::seconds(2 * 86400 + 5 * 3600), now),
            "2天5小时"
        );
        assert_eq!(remaining_text(now - chrono::Duration::seconds(90), now), "已逾期 1分钟");
    }
}
