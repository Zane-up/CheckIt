//! 提醒调度引擎。
//!
//! 可靠性声明（诚实边界）：
//!
//! - 提供 **at-least-once** 语义，不承诺 exactly-once。
//! - 崩溃窗口：系统通知已受理、但 `submitted` 尚未写回数据库时进程崩溃，
//!   重启后该 `pending` 行会按崩溃恢复规则再次提交，可能产生重复通知。
//!   该窗口无法用本地 SQLite 事务消除（跨进程/跨系统 API），因此明确保留而不隐藏。
//!
//! 设计：
//!
//! 1. **绝对时间重算**：每次扫描都用同一 `now` 重新计算 due_at - offset；
//!    休眠恢复、系统时间调整、进程重启自然正确。
//! 2. **快照去重键**：reminder_events 以 (task_id, offset_seconds, due_at_snapshot)
//!    为唯一约束。任务改期产生新快照键；旧键动作在领取与发送前均校验失效。
//! 3. **先领取后提交**：领取 = 插入 state='pending'（queued）行（事务内）。
//!    后台提交返回后：系统 API 受理 → `submitted`；失败 → `failed`（attempts+1）。
//!    进程在提交前崩溃遗留的 pending 超过 90s 后由下一轮扫描恢复。
//! 4. **统一有效性校验**（新提醒、崩溃恢复、失败重试、发送前共用同一 `classify`）：
//!    - 任务仍为 pending
//!    - 截止时间仍等于 due_at_snapshot
//!    - 对应 offset 仍在任务 reminders 中
//!    - 至少一种提醒方式仍开启
//!    - 距离计划时刻未超过 15 分钟补发宽限
//! 5. **状态语义**：
//!    - pending   = 已排队待提交（queued，可崩溃恢复）
//!    - submitted = 底层系统 API 已受理（不等于用户看到横幅）
//!    - failed    = 底层提交失败；未达上限且未超宽限时按 30s 间隔重试
//!    - skipped   = 静默终止（通知关闭 / 超过补发宽限）
//!    - cancelled = 动作失效（完成/删除/改期/移除 offset）
//! 6. **通知适配层**：`NotificationTransport` 同步返回 WinRT/系统 API 的真实提交结果，
//!    避免 tauri-plugin-notification 异步 `let _ = show()` 丢弃错误的问题。
//!    审核接口的调用链与语义见 `crate::notification`。

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::database::{Database, DbError, DbResult};
use crate::models::task::{Task, TaskStatus};
use crate::notification::{NotificationContent, NotificationTransport};

/// 错过提醒的静默补记窗口：超过该时长的提醒只记录不通知。
pub const MISSED_GRACE_SECS: i64 = 15 * 60;

/// pending 行被视为"崩溃遗留"的最小年龄；小于该年龄的 pending 属于本轮正常在途。
pub const PENDING_STALE_SECS: i64 = 90;

/// failed 行两次重试之间的最小间隔。
pub const RETRY_BACKOFF_SECS: i64 = 30;

/// failed 行的最大提交尝试次数（含首次）。
pub const MAX_ATTEMPTS: i32 = 3;

/// 提醒动作：需要（重新）尝试提交通知
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReminderAction {
    pub task_id: i64,
    pub offset_seconds: i64,
    pub due_at_snapshot: String,
    /// true = 本轮新领取；false = 崩溃恢复 / 失败重试
    pub is_new_claim: bool,
}

/// 动作有效性分类（新领取、恢复、发送前共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionValidity {
    /// 可以提交
    Valid,
    /// 任务/offset/快照已失效 → cancelled
    Cancelled,
    /// 通知关闭或超出补发宽限 → skipped
    SilentSkip,
    /// 尚未到计划时刻（时钟回拨等）→ 保持 pending，等待下轮
    Deferred,
}

/// 一次发送尝试的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryOutcome {
    /// 所选提醒渠道已受理（不代表用户看到或确认）
    Submitted { title: String },
    /// 底层提交失败，已记录 failed（attempts+1）
    Failed { error: String },
    /// 静默跳过，未调用系统通知
    Skipped,
    /// 动作已失效，未调用系统通知
    Cancelled,
    /// 尚未到计划时刻，未调用系统通知
    Deferred,
}

/// 可注入时钟：扫描可以共用一次快照，每条动作提交前后必须重新取时。
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// 生产时钟：唯一允许直接读取系统时间的位置。
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// 固定时钟：保留给确定性测试入口。
#[cfg(test)]
struct FixedClock(DateTime<Utc>);

#[cfg(test)]
impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
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

    /// 扫描一轮，返回需要尝试提交通知的动作列表。
    ///
    /// 新提醒、崩溃遗留 pending、失败重试全部先用同一个 `classify` 校验；
    /// 不满足条件的行直接落到 skipped/cancelled，绝不产生发送动作。
    pub fn run_once(&self, now: DateTime<Utc>) -> DbResult<Vec<ReminderAction>> {
        // 通知开关与本轮所有有效性判断共用同一份设置快照。
        let notify_on = self.db.load_settings()?.reminders_enabled();

        let mut actions = Vec::new();

        // —— 第一阶段：领取新到期的提醒 ——
        for task in self.db.list_schedulable_tasks()? {
            if task.status != TaskStatus::Pending {
                continue;
            }
            let due_str = match task.due_at.as_deref() {
                Some(d) => d,
                None => continue,
            };
            let Some(due_at) = parse_rfc3339(due_str) else {
                continue;
            };

            for &offset in &task.reminders {
                let scheduled = due_at - chrono::Duration::seconds(offset);
                if scheduled > now {
                    continue;
                }
                let late_secs = (now - scheduled).num_seconds();
                let quiet = late_secs > MISSED_GRACE_SECS || !notify_on;
                if let Some(needs_notify) =
                    self.db
                        .claim_reminder(task.id, offset, due_str, quiet, now)?
                {
                    if needs_notify {
                        actions.push(ReminderAction {
                            task_id: task.id,
                            offset_seconds: offset,
                            due_at_snapshot: due_str.to_string(),
                            is_new_claim: true,
                        });
                    }
                }
            }
        }

        // —— 第二阶段：恢复（崩溃遗留 pending / 失败重试） ——
        let recoverable = self.db.recoverable_reminders(
            PENDING_STALE_SECS,
            RETRY_BACKOFF_SECS,
            MAX_ATTEMPTS,
            now,
        )?;
        if !recoverable.is_empty() {
            let ids: Vec<i64> = recoverable.iter().map(|r| r.task_id).collect();
            let tasks: HashMap<i64, Task> = self
                .db
                .list_tasks_by_ids(&ids)?
                .into_iter()
                .map(|t| (t.id, t))
                .collect();

            for row in recoverable {
                let validity = Self::classify(
                    tasks.get(&row.task_id),
                    row.offset_seconds,
                    &row.due_at_snapshot,
                    notify_on,
                    now,
                );
                match validity {
                    ActionValidity::Valid => actions.push(ReminderAction {
                        task_id: row.task_id,
                        offset_seconds: row.offset_seconds,
                        due_at_snapshot: row.due_at_snapshot,
                        is_new_claim: false,
                    }),
                    ActionValidity::Cancelled => self.db.set_reminder_state(
                        row.task_id,
                        row.offset_seconds,
                        &row.due_at_snapshot,
                        "cancelled",
                        false,
                        now,
                    )?,
                    ActionValidity::SilentSkip => self.db.set_reminder_state(
                        row.task_id,
                        row.offset_seconds,
                        &row.due_at_snapshot,
                        "skipped",
                        false,
                        now,
                    )?,
                    // 时间回拨等导致尚未到期：保持 pending/failed，等待后续扫描。
                    ActionValidity::Deferred => {}
                }
            }
        }

        Ok(actions)
    }

    /// 发送前二次校验：任务仍在、offset 仍配置、通知仍开启、未超宽限。
    pub fn action_still_valid(
        &self,
        action: &ReminderAction,
        now: DateTime<Utc>,
    ) -> DbResult<bool> {
        Ok(matches!(
            self.evaluate_action(action, now)?,
            ActionValidity::Valid
        ))
    }

    /// 只读评估动作有效性（触发数据库读取，不修改状态）。
    pub fn evaluate_action(
        &self,
        action: &ReminderAction,
        now: DateTime<Utc>,
    ) -> DbResult<ActionValidity> {
        let notify_on = self.db.load_settings()?.reminders_enabled();
        let task = match self.db.get_task(action.task_id) {
            Ok(t) => Some(t),
            Err(DbError::NotFound) => None,
            Err(e) => return Err(e),
        };
        Ok(Self::classify(
            task.as_ref(),
            action.offset_seconds,
            &action.due_at_snapshot,
            notify_on,
            now,
        ))
    }

    /// 统一有效性判定。新领取、崩溃恢复、失败重试、发送前全部使用此函数。
    fn classify(
        task: Option<&Task>,
        offset_seconds: i64,
        due_at_snapshot: &str,
        notify_on: bool,
        now: DateTime<Utc>,
    ) -> ActionValidity {
        let Some(task) = task else {
            // 任务已删除
            return ActionValidity::Cancelled;
        };
        if task.status != TaskStatus::Pending
            || task.due_at.as_deref() != Some(due_at_snapshot)
            || !task.reminders.contains(&offset_seconds)
        {
            // 完成 / 改期 / 移除对应提醒 offset
            return ActionValidity::Cancelled;
        }
        if !notify_on {
            return ActionValidity::SilentSkip;
        }
        let Some(due_at) = parse_rfc3339(due_at_snapshot) else {
            return ActionValidity::Cancelled;
        };
        let scheduled = due_at - chrono::Duration::seconds(offset_seconds);
        let late_secs = (now - scheduled).num_seconds();
        if late_secs < 0 {
            return ActionValidity::Deferred;
        }
        if late_secs > MISSED_GRACE_SECS {
            return ActionValidity::SilentSkip;
        }
        ActionValidity::Valid
    }

    /// 确定性测试入口：以固定时间作为整个提交过程的时钟。
    ///
    /// 生产调度必须使用 [`Self::deliver_with_clock`]；本方法仅保留给
    /// 不推进时钟的回归测试，避免测试之外再次出现“沿用旧扫描时间”的路径。
    #[cfg(test)]
    pub fn deliver(
        &self,
        action: &ReminderAction,
        now: DateTime<Utc>,
        transport: &dyn NotificationTransport,
    ) -> DbResult<DeliveryOutcome> {
        self.deliver_with_clock(action, &FixedClock(now), transport)
    }

    /// 执行一次发送尝试，并把真实结果接入状态机。
    ///
    /// 调用方应传入后台系统通知适配器；测试可注入失败实现。
    /// **提交前**用 `clock.now()` 重新校验宽限与有效性，**提交完成后**再次取时，
    /// 作为 submitted/failed 的 updated_at（重试间隔从实际失败完成时间计算）。
    /// 系统调用一旦开始无法撤回；若调用期间超过宽限，仍会记录真实提交结果，
    /// 下一轮再按宽限策略静默，不承诺 exactly-once。
    pub fn deliver_with_clock(
        &self,
        action: &ReminderAction,
        clock: &dyn Clock,
        transport: &dyn NotificationTransport,
    ) -> DbResult<DeliveryOutcome> {
        let started_at = clock.now();
        let notify_on = self.db.load_settings()?.reminders_enabled();
        let task = match self.db.get_task(action.task_id) {
            Ok(t) => Some(t),
            Err(DbError::NotFound) => None,
            Err(e) => return Err(e),
        };
        let validity = Self::classify(
            task.as_ref(),
            action.offset_seconds,
            &action.due_at_snapshot,
            notify_on,
            started_at,
        );

        match validity {
            ActionValidity::Cancelled => {
                self.db.set_reminder_state(
                    action.task_id,
                    action.offset_seconds,
                    &action.due_at_snapshot,
                    "cancelled",
                    false,
                    started_at,
                )?;
                Ok(DeliveryOutcome::Cancelled)
            }
            ActionValidity::SilentSkip => {
                self.db.set_reminder_state(
                    action.task_id,
                    action.offset_seconds,
                    &action.due_at_snapshot,
                    "skipped",
                    false,
                    started_at,
                )?;
                Ok(DeliveryOutcome::Skipped)
            }
            ActionValidity::Deferred => Ok(DeliveryOutcome::Deferred),
            ActionValidity::Valid => {
                let task = task.expect("Valid 分支必然存在任务");
                let Some(due_at) = task.due_at.as_deref().and_then(parse_rfc3339) else {
                    // 理论上不可达：classify 已解析成功。
                    self.db.set_reminder_state(
                        action.task_id,
                        action.offset_seconds,
                        &action.due_at_snapshot,
                        "cancelled",
                        false,
                        started_at,
                    )?;
                    return Ok(DeliveryOutcome::Cancelled);
                };

                let content = NotificationContent {
                    title: "CheckIt".to_string(),
                    body: notification_body_localized(&task.title, due_at, started_at, &self.db.load_settings()?.language),
                };

                match transport.submit(&content) {
                    Ok(()) => {
                        // 状态时间戳使用系统调用完成时刻，而不是扫描快照。
                        let completed_at = clock.now();
                        self.db.set_reminder_state(
                            action.task_id,
                            action.offset_seconds,
                            &action.due_at_snapshot,
                            "submitted",
                            false,
                            completed_at,
                        )?;
                        Ok(DeliveryOutcome::Submitted { title: task.title })
                    }
                    Err(error) => {
                        log::error!(
                            "系统通知提交失败（task={} offset={}）: {error}",
                            action.task_id,
                            action.offset_seconds
                        );
                        // 失败时间取实际完成时刻，下一次重试的 30s 退避从这里起算。
                        let completed_at = clock.now();
                        self.db.set_reminder_state(
                            action.task_id,
                            action.offset_seconds,
                            &action.due_at_snapshot,
                            "failed",
                            true,
                            completed_at,
                        )?;
                        Ok(DeliveryOutcome::Failed { error })
                    }
                }
            }
        }
    }
}

/// 解析 RFC3339 UTC 时间字符串
pub fn parse_rfc3339(s: &str) -> Option<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

/// 生成通知正文。
/// - 未到期：「任务」X 后截止
/// - 已到期（含刚到期）：「任务」已逾期 X —— 刚截止时也用逾期表述，
///   避免"已逾期 30秒后截止"这类拼接错误
pub fn notification_body(title: &str, due_at: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = (due_at - now).num_seconds();
    let name = truncate_chars(title, 40);
    if secs > 0 {
        format!("「{name}」{}后截止", duration_text(secs))
    } else {
        format!("「{name}」已逾期 {}", duration_text(-secs))
    }
}

pub fn notification_body_localized(title: &str, due_at: DateTime<Utc>, now: DateTime<Utc>, language: &str) -> String {
    if language != "en" { return notification_body(title, due_at, now); }
    let secs=(due_at-now).num_seconds();
    let value=secs.unsigned_abs();
    let duration=if value<60 {format!("{value}s")} else if value<3600 {format!("{}m",value/60)}
        else if value<86400 {format!("{}h {}m",value/3600,value%3600/60)} else {format!("{}d {}h",value/86400,value%86400/3600)};
    let name=truncate_chars(title,40);
    if secs>0 {format!("“{name}” is due in {duration}")} else {format!("“{name}” is overdue by {duration}")}
}

/// 秒数 → 简短时长文本（通知用，不含前缀）
pub fn duration_text(secs: i64) -> String {
    if secs < 60 {
        format!("{secs}秒")
    } else if secs < 3600 {
        format!("{}分钟", secs / 60)
    } else if secs < 86400 {
        format!("{}小时{}分钟", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}天{}小时", secs / 86400, (secs % 86400) / 3600)
    }
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{truncated}…")
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    impl Database {
        /// 测试辅助：把某条事件行的 updated_at 设为指定时刻（与注入时钟保持一致）。
        pub fn test_set_event_updated_at(
            &self,
            task_id: i64,
            offset_seconds: i64,
            due_at_snapshot: &str,
            at: DateTime<Utc>,
        ) -> DbResult<()> {
            let conn = self.lock_conn_for_test();
            conn.execute(
                "UPDATE reminder_events SET updated_at = ?4
                 WHERE task_id = ?1 AND offset_seconds = ?2 AND due_at_snapshot = ?3",
                rusqlite::params![
                    task_id,
                    offset_seconds,
                    due_at_snapshot,
                    crate::database::format_iso(at)
                ],
            )?;
            Ok(())
        }

        /// 测试辅助：读取事件行状态
        pub fn test_event_state(
            &self,
            task_id: i64,
            offset_seconds: i64,
            due_at_snapshot: &str,
        ) -> Option<(String, i32)> {
            let conn = self.lock_conn_for_test();
            conn.query_row(
                "SELECT state, attempts FROM reminder_events
                 WHERE task_id = ?1 AND offset_seconds = ?2 AND due_at_snapshot = ?3",
                rusqlite::params![task_id, offset_seconds, due_at_snapshot],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i32>(1)?)),
            )
            .ok()
        }

        /// 测试辅助：读取事件行 updated_at（验证提交完成时间与重试退避基准）
        pub fn test_event_updated_at(
            &self,
            task_id: i64,
            offset_seconds: i64,
            due_at_snapshot: &str,
        ) -> Option<String> {
            let conn = self.lock_conn_for_test();
            conn.query_row(
                "SELECT updated_at FROM reminder_events
                 WHERE task_id = ?1 AND offset_seconds = ?2 AND due_at_snapshot = ?3",
                rusqlite::params![task_id, offset_seconds, due_at_snapshot],
                |r| r.get::<_, String>(0),
            )
            .ok()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    use chrono::{SecondsFormat, TimeZone};

    use super::*;
    use crate::models::task::{Priority, TaskInput};

    /// 明确注入的固定时钟：2030 年，与机器真实时间无关。
    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2030, 1, 1, 0, 0, 0).unwrap()
    }

    fn at(secs: i64) -> DateTime<Utc> {
        t0() + chrono::Duration::seconds(secs)
    }

    fn iso(secs: i64) -> String {
        at(secs).to_rfc3339_opts(SecondsFormat::Secs, true)
    }

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

    fn action_for(task_id: i64, offset: i64, due: &str, is_new: bool) -> ReminderAction {
        ReminderAction {
            task_id,
            offset_seconds: offset,
            due_at_snapshot: due.to_string(),
            is_new_claim: is_new,
        }
    }

    #[derive(Default)]
    struct FakeTransport {
        calls: Mutex<Vec<NotificationContent>>,
        fail_next: AtomicBool,
    }

    impl FakeTransport {
        fn set_fail_next(&self, fail: bool) {
            self.fail_next.store(fail, Ordering::SeqCst);
        }

        fn call_count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }

        fn last_content(&self) -> Option<NotificationContent> {
            self.calls.lock().unwrap().last().cloned()
        }
    }

    impl NotificationTransport for FakeTransport {
        fn submit(&self, content: &NotificationContent) -> Result<(), String> {
            self.calls.lock().unwrap().push(content.clone());
            if self.fail_next.swap(false, Ordering::SeqCst) {
                Err("模拟底层 WinRT Toast 提交失败".into())
            } else {
                Ok(())
            }
        }
    }

    /// 可由测试推进的时钟，用于模拟扫描后睡眠/提交阻塞。
    #[derive(Clone)]
    struct TestClock {
        now: std::sync::Arc<Mutex<DateTime<Utc>>>,
    }

    impl TestClock {
        fn new(now: DateTime<Utc>) -> Self {
            Self {
                now: std::sync::Arc::new(Mutex::new(now)),
            }
        }

        fn advance(&self, secs: i64) {
            *self.now.lock().unwrap() += chrono::Duration::seconds(secs);
        }
    }

    impl Clock for TestClock {
        fn now(&self) -> DateTime<Utc> {
            *self.now.lock().unwrap()
        }
    }

    /// 提交时推进时钟的通知传输，模拟系统调用阻塞/机器睡眠。
    struct ClockAdvancingTransport {
        clock: TestClock,
        advance_secs: i64,
        fail: bool,
        calls: std::sync::atomic::AtomicUsize,
    }

    impl ClockAdvancingTransport {
        fn new(clock: TestClock, advance_secs: i64, fail: bool) -> Self {
            Self {
                clock,
                advance_secs,
                fail,
                calls: std::sync::atomic::AtomicUsize::new(0),
            }
        }

        fn call_count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl NotificationTransport for ClockAdvancingTransport {
        fn submit(&self, _content: &NotificationContent) -> Result<(), String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            // 先完成本次“系统调用”，再推进时钟；随后所有状态时间戳都用新时间。
            self.clock.advance(self.advance_secs);
            if self.fail {
                Err("模拟提交完成时已超时并失败".into())
            } else {
                Ok(())
            }
        }
    }

    fn disable_notifications(db: &Database) {
        db.update_settings_atomic(|s| s.notifications_enabled = false)
            .unwrap();
    }

    #[test]
    fn fires_when_due_time_reached() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        // 截止在 59 分钟后，提前 1 小时提醒 => 调度时刻在 1 分钟前
        let due = iso(3540);
        let task = db
            .create_task(&input("提交作业", Some(&due), vec![3600]))
            .unwrap();

        let actions = engine.run_once(t0()).unwrap();
        assert_eq!(actions, vec![action_for(task.id, 3600, &due, true)]);
        assert_eq!(
            db.test_event_state(task.id, 3600, &due),
            Some(("pending".into(), 0)),
            "领取后为 queued(pending)，等待后台提交"
        );
    }

    #[test]
    fn no_fire_before_scheduled_time() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = iso(3 * 3600);
        db.create_task(&input("未到期任务", Some(&due), vec![3600]))
            .unwrap();

        assert!(engine.run_once(t0()).unwrap().is_empty());
        assert!(db.test_event_state(1, 3600, &due).is_none());
    }

    #[test]
    fn idempotent_no_double_fire() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = iso(2760); // scheduled = t0-840s，宽限内
        db.create_task(&input("幂等任务", Some(&due), vec![3600]))
            .unwrap();

        assert_eq!(engine.run_once(t0()).unwrap().len(), 1);
        assert!(engine.run_once(at(1)).unwrap().is_empty());
        assert!(engine.run_once(at(2)).unwrap().is_empty());
    }

    #[test]
    fn completed_task_stops_reminders() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = iso(3540);
        let task = db
            .create_task(&input("完成任务测试", Some(&due), vec![3600]))
            .unwrap();
        db.set_task_status(task.id, true).unwrap();

        assert!(engine.run_once(t0()).unwrap().is_empty());
    }

    #[test]
    fn restore_pending_rearms_future_reminders() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = iso(2 * 3600);
        let task = db
            .create_task(&input("恢复任务", Some(&due), vec![3600, 7800]))
            .unwrap();

        let actions = engine.run_once(t0()).unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].offset_seconds, 7800);

        db.set_task_status(task.id, true).unwrap();
        assert!(engine.run_once(at(1)).unwrap().is_empty());

        db.set_task_status(task.id, false).unwrap();
        assert!(engine.run_once(at(2)).unwrap().is_empty());
        // 到达提前 1 小时的计划时刻
        let actions = engine.run_once(at(3660)).unwrap();
        assert_eq!(actions, vec![action_for(task.id, 3600, &due, true)]);
    }

    #[test]
    fn due_change_recomputes_with_snapshot_keys() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due_old = iso(3540);
        let task = db
            .create_task(&input("改期任务", Some(&due_old), vec![3600]))
            .unwrap();

        assert_eq!(engine.run_once(t0()).unwrap().len(), 1);
        assert_eq!(
            db.test_event_state(task.id, 3600, &due_old),
            Some(("pending".into(), 0))
        );

        let due_new = iso(2 * 86400);
        db.update_task(task.id, &input("改期任务", Some(&due_new), vec![3600]))
            .unwrap();

        let stale = action_for(task.id, 3600, &due_old, true);
        assert!(!engine.action_still_valid(&stale, at(1)).unwrap());

        assert!(engine.run_once(at(1)).unwrap().is_empty());
        assert!(db.test_event_state(task.id, 3600, &due_old).is_some());

        let near = at(2 * 86400 - 3500);
        let actions = engine.run_once(near).unwrap();
        assert_eq!(actions, vec![action_for(task.id, 3600, &due_new, true)]);
    }

    /// 旧动作领取后移除对应提醒：发送前校验失败，不调用系统通知。
    #[test]
    fn removed_offset_invalidates_claimed_action() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = iso(2760);
        let task = db
            .create_task(&input("移除提醒", Some(&due), vec![3600]))
            .unwrap();
        let actions = engine.run_once(t0()).unwrap();
        assert_eq!(actions.len(), 1);

        // 同截止时间，但移除该 offset
        db.update_task(task.id, &input("移除提醒", Some(&due), vec![600]))
            .unwrap();

        let transport = FakeTransport::default();
        let outcome = engine.deliver(&actions[0], at(1), &transport).unwrap();
        assert_eq!(outcome, DeliveryOutcome::Cancelled);
        assert_eq!(transport.call_count(), 0, "失效动作不得提交系统通知");
        assert_eq!(
            db.test_event_state(task.id, 3600, &due),
            Some(("cancelled".into(), 0))
        );
    }

    /// 恢复路径同样检查 offset 是否仍在提醒配置中。
    #[test]
    fn removed_offset_is_cancelled_during_recovery() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = iso(3540);
        let task = db
            .create_task(&input("恢复移除", Some(&due), vec![3600]))
            .unwrap();
        engine.run_once(t0()).unwrap();
        db.update_task(task.id, &input("恢复移除", Some(&due), vec![]))
            .unwrap();

        // 越过 pending stale 阈值进入恢复阶段
        let actions = engine.run_once(at(PENDING_STALE_SECS)).unwrap();
        assert!(actions.is_empty());
        assert_eq!(
            db.test_event_state(task.id, 3600, &due),
            Some(("cancelled".into(), 0))
        );
    }

    /// 改期、完成、删除都会让已领取的旧动作失效。
    #[test]
    fn task_change_reschedule_complete_delete_invalidates_action() {
        let db = db();

        // 改期
        let engine = ReminderEngine::new(db.clone());
        let due = iso(3540);
        let t1 = db
            .create_task(&input("改期失效", Some(&due), vec![3600]))
            .unwrap();
        let a1 = engine.run_once(t0()).unwrap().remove(0);
        db.update_task(t1.id, &input("改期失效", Some(&iso(86400)), vec![3600]))
            .unwrap();
        let transport = FakeTransport::default();
        assert_eq!(
            engine.deliver(&a1, at(1), &transport).unwrap(),
            DeliveryOutcome::Cancelled
        );

        // 完成
        let t2 = db
            .create_task(&input("完成失效", Some(&iso(3541)), vec![3600]))
            .unwrap();
        let a2 = engine.run_once(at(1)).unwrap().into_iter().find(|a| a.task_id == t2.id).unwrap();
        db.set_task_status(t2.id, true).unwrap();
        assert_eq!(
            engine.deliver(&a2, at(2), &transport).unwrap(),
            DeliveryOutcome::Cancelled
        );

        // 删除
        let t3 = db
            .create_task(&input("删除失效", Some(&iso(3542)), vec![3600]))
            .unwrap();
        let a3 = engine.run_once(at(2)).unwrap().into_iter().find(|a| a.task_id == t3.id).unwrap();
        db.delete_task(t3.id).unwrap();
        assert_eq!(
            engine.deliver(&a3, at(3), &transport).unwrap(),
            DeliveryOutcome::Cancelled
        );

        assert_eq!(transport.call_count(), 0);
    }

    #[test]
    fn missed_reminder_beyond_grace_is_silent() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        // 调度时刻在 23 小时前
        let due = iso(3600);
        let task = db
            .create_task(&input("错过任务", Some(&due), vec![86400]))
            .unwrap();

        assert!(engine.run_once(t0()).unwrap().is_empty());
        assert_eq!(
            db.test_event_state(task.id, 86400, &due),
            Some(("skipped".into(), 0))
        );
        assert!(engine.run_once(at(1)).unwrap().is_empty());
    }

    #[test]
    fn missed_within_grace_still_notifies() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = iso(3600 - 600); // 计划时刻在 10 分钟前
        db.create_task(&input("宽限任务", Some(&due), vec![3600]))
            .unwrap();
        assert_eq!(engine.run_once(t0()).unwrap().len(), 1);
    }

    /// 宽限边界：恰好 15 分钟（含）内 → 通知；超过 1 秒 → 静默。
    #[test]
    fn grace_boundary_exact() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = iso(900);
        db.create_task(&input("边界A", Some(&due), vec![1800]))
            .unwrap();
        assert_eq!(engine.run_once(t0()).unwrap().len(), 1);

        let db2 = crate::database::Database::open_in_memory().unwrap();
        db2.ensure_default_categories().unwrap();
        let engine2 = ReminderEngine::new(db2.clone());
        let due2 = iso(899);
        db2.create_task(&input("边界B", Some(&due2), vec![1800]))
            .unwrap();
        assert!(engine2.run_once(t0()).unwrap().is_empty());
    }

    /// 崩溃恢复：pending 在 89s 时仍视为在途；90s 起恢复；使用同一注入时钟。
    #[test]
    fn crashed_pending_recovers_at_stale_boundary() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = iso(3540);
        let task = db
            .create_task(&input("崩溃恢复", Some(&due), vec![3600]))
            .unwrap();
        engine.run_once(t0()).unwrap();

        assert!(engine.run_once(at(89)).unwrap().is_empty());
        let actions = engine.run_once(at(PENDING_STALE_SECS)).unwrap();
        assert_eq!(actions.len(), 1);
        assert!(!actions[0].is_new_claim);

        let transport = FakeTransport::default();
        assert_eq!(
            engine.deliver(&actions[0], at(PENDING_STALE_SECS), &transport).unwrap(),
            DeliveryOutcome::Submitted {
                title: "崩溃恢复".into()
            }
        );
        assert_eq!(
            db.test_event_state(task.id, 3600, &due),
            Some(("submitted".into(), 0))
        );
    }

    /// 崩溃恢复超过 15 分钟宽限：静默 skipped，不提交。
    #[test]
    fn crashed_pending_beyond_grace_is_skipped() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = iso(3540);
        let task = db
            .create_task(&input("崩溃超宽限", Some(&due), vec![3600]))
            .unwrap();
        engine.run_once(t0()).unwrap();

        let actions = engine.run_once(at(MISSED_GRACE_SECS + 1)).unwrap();
        assert!(actions.is_empty());
        assert_eq!(
            db.test_event_state(task.id, 3600, &due),
            Some(("skipped".into(), 0))
        );
    }

    /// 扫描后时钟前进一小时：提交前重新取时，必须静默跳过且不调用通知发送器。
    #[test]
    fn stale_scan_time_beyond_grace_is_skipped_before_submit() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = iso(3540); // 计划时刻 = t0 - 60s，扫描时仍在宽限内
        let task = db
            .create_task(&input("扫描后睡眠", Some(&due), vec![3600]))
            .unwrap();
        let action = engine.run_once(t0()).unwrap().remove(0);

        let clock = TestClock::new(at(3600));
        let transport = ClockAdvancingTransport::new(clock.clone(), 0, false);
        let outcome = engine
            .deliver_with_clock(&action, &clock, &transport)
            .unwrap();

        assert_eq!(outcome, DeliveryOutcome::Skipped);
        assert_eq!(transport.call_count(), 0, "超宽限不得调用系统通知");
        assert_eq!(
            db.test_event_state(task.id, 3600, &due),
            Some(("skipped".into(), 0))
        );
    }

    /// 第一条通知提交期间时钟前进：后续动作必须使用更新后的时间重新校验，
    /// 不能继续沿用扫描快照而误发第二条。
    #[test]
    fn later_actions_use_updated_clock_after_first_submit_blocks() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = iso(3540);
        let first_task = db
            .create_task(&input("第一条", Some(&due), vec![3600]))
            .unwrap();
        let second_task = db
            .create_task(&input("第二条", Some(&due), vec![3600]))
            .unwrap();

        let actions = engine.run_once(t0()).unwrap();
        assert_eq!(actions.len(), 2);

        let clock = TestClock::new(t0());
        // 第一次提交完成后时钟前进一小时；第二次动作提交前应超宽限。
        let transport = ClockAdvancingTransport::new(clock.clone(), 3600, false);
        let mut outcomes = Vec::new();
        for action in &actions {
            outcomes.push((
                action.task_id,
                engine
                    .deliver_with_clock(action, &clock, &transport)
                    .unwrap(),
            ));
        }

        let first = outcomes
            .iter()
            .find(|(id, _)| *id == first_task.id)
            .map(|(_, outcome)| outcome.clone())
            .unwrap();
        let second = outcomes
            .iter()
            .find(|(id, _)| *id == second_task.id)
            .map(|(_, outcome)| outcome.clone())
            .unwrap();
        assert!(matches!(first, DeliveryOutcome::Submitted { .. }));
        assert_eq!(second, DeliveryOutcome::Skipped);
        assert_eq!(transport.call_count(), 1, "第二条不得提交");
        assert_eq!(
            db.test_event_state(first_task.id, 3600, &due),
            Some(("submitted".into(), 0))
        );
        assert_eq!(
            db.test_event_state(second_task.id, 3600, &due),
            Some(("skipped".into(), 0))
        );
    }

    /// 提交失败的状态时间戳必须使用失败完成时刻，而不是扫描开始时刻。
    #[test]
    fn failed_status_timestamp_uses_completion_time() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = iso(3540);
        let task = db
            .create_task(&input("慢失败", Some(&due), vec![3600]))
            .unwrap();
        let action = engine.run_once(t0()).unwrap().remove(0);

        let clock = TestClock::new(t0());
        // 系统调用耗时 100 秒后失败。
        let transport = ClockAdvancingTransport::new(clock.clone(), 100, true);
        let outcome = engine
            .deliver_with_clock(&action, &clock, &transport)
            .unwrap();
        assert!(matches!(outcome, DeliveryOutcome::Failed { .. }));
        assert_eq!(
            db.test_event_state(task.id, 3600, &due),
            Some(("failed".into(), 1))
        );
        assert_eq!(
            db.test_event_updated_at(task.id, 3600, &due),
            Some(crate::database::format_iso(at(100))),
            "failed.updated_at 应为实际失败完成时间"
        );
    }

    /// 下一次重试间隔必须从实际失败完成时间起算，而不是从扫描时间起算。
    #[test]
    fn retry_backoff_starts_from_actual_failure_time() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = iso(3540);
        let task = db
            .create_task(&input("慢失败后重试", Some(&due), vec![3600]))
            .unwrap();
        let action = engine.run_once(t0()).unwrap().remove(0);

        let clock = TestClock::new(t0());
        let slow_failure = ClockAdvancingTransport::new(clock.clone(), 100, true);
        engine
            .deliver_with_clock(&action, &clock, &slow_failure)
            .unwrap();
        assert_eq!(slow_failure.call_count(), 1);

        // 失败完成于 t0+100；30s 退避在 t0+130 才满足。
        assert!(engine.run_once(at(129)).unwrap().is_empty());
        let retry = engine.run_once(at(130)).unwrap();
        assert_eq!(retry.len(), 1);
        assert!(!retry[0].is_new_claim);

        let retry_clock = TestClock::new(at(130));
        let success = ClockAdvancingTransport::new(retry_clock.clone(), 0, false);
        assert!(matches!(
            engine
                .deliver_with_clock(&retry[0], &retry_clock, &success)
                .unwrap(),
            DeliveryOutcome::Submitted { .. }
        ));
        assert_eq!(
            db.test_event_state(task.id, 3600, &due),
            Some(("submitted".into(), 1))
        );
    }

    /// 底层通知提交失败可被观察：进入 failed，attempts+1，且遵守 30s 重试间隔。
    #[test]
    fn notification_submission_failure_enters_retry_with_backoff() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = iso(3540);
        let task = db
            .create_task(&input("底层失败", Some(&due), vec![3600]))
            .unwrap();
        let action = engine.run_once(t0()).unwrap().remove(0);

        let transport = FakeTransport::default();
        transport.set_fail_next(true);
        let outcome = engine.deliver(&action, t0(), &transport).unwrap();
        assert!(matches!(outcome, DeliveryOutcome::Failed { .. }));
        assert_eq!(transport.call_count(), 1);
        assert_eq!(
            db.test_event_state(task.id, 3600, &due),
            Some(("failed".into(), 1))
        );

        // 未到重试间隔：不产生动作
        assert!(engine.run_once(at(29)).unwrap().is_empty());
        // 到达重试间隔：产生重试动作
        let retry = engine.run_once(at(RETRY_BACKOFF_SECS)).unwrap();
        assert_eq!(retry.len(), 1);
        assert!(!retry[0].is_new_claim);
        // 本轮成功 → submitted，attempts 保留历史计数
        assert_eq!(
            engine.deliver(&retry[0], at(RETRY_BACKOFF_SECS), &transport).unwrap(),
            DeliveryOutcome::Submitted {
                title: "底层失败".into()
            }
        );
        assert_eq!(transport.call_count(), 2);
        assert_eq!(
            db.test_event_state(task.id, 3600, &due),
            Some(("submitted".into(), 1))
        );
    }

    /// 失败重试达到上限后不再尝试（attempts = 3）。
    #[test]
    fn failed_retry_stops_at_max_attempts() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = iso(3540);
        let task = db
            .create_task(&input("重试上限", Some(&due), vec![3600]))
            .unwrap();
        let action = engine.run_once(t0()).unwrap().remove(0);
        let transport = FakeTransport::default();

        let mut current = action;
        let mut now = t0();
        for expected_attempts in 1..=MAX_ATTEMPTS {
            transport.set_fail_next(true);
            assert!(matches!(
                engine.deliver(&current, now, &transport).unwrap(),
                DeliveryOutcome::Failed { .. }
            ));
            assert_eq!(
                db.test_event_state(task.id, 3600, &due),
                Some(("failed".into(), expected_attempts))
            );
            now = now + chrono::Duration::seconds(RETRY_BACKOFF_SECS);
            if expected_attempts < MAX_ATTEMPTS {
                current = engine.run_once(now).unwrap().remove(0);
            }
        }
        assert!(engine.run_once(now).unwrap().is_empty(), "达到上限后不得重试");
        assert_eq!(transport.call_count(), MAX_ATTEMPTS as usize);
    }

    /// 失败后关闭通知：恢复路径静默 skipped，不产生动作、不提交。
    #[test]
    fn notifications_disabled_after_failure_is_silent() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = iso(3540);
        let task = db
            .create_task(&input("失败后静音", Some(&due), vec![3600]))
            .unwrap();
        let action = engine.run_once(t0()).unwrap().remove(0);
        let transport = FakeTransport::default();
        transport.set_fail_next(true);
        engine.deliver(&action, t0(), &transport).unwrap();

        disable_notifications(&db);
        let actions = engine.run_once(at(RETRY_BACKOFF_SECS)).unwrap();
        assert!(actions.is_empty());
        assert_eq!(transport.call_count(), 1, "关闭通知后不得再次提交");
        assert_eq!(
            db.test_event_state(task.id, 3600, &due),
            Some(("skipped".into(), 1))
        );
    }

    /// 领取后、发送前关闭通知：deliver 直接 skipped。
    #[test]
    fn deliver_skips_when_notifications_turned_off_after_claim() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = iso(3540);
        let task = db
            .create_task(&input("发送前静音", Some(&due), vec![3600]))
            .unwrap();
        let action = engine.run_once(t0()).unwrap().remove(0);
        disable_notifications(&db);

        let transport = FakeTransport::default();
        assert_eq!(
            engine.deliver(&action, at(1), &transport).unwrap(),
            DeliveryOutcome::Skipped
        );
        assert_eq!(transport.call_count(), 0);
        assert_eq!(
            db.test_event_state(task.id, 3600, &due),
            Some(("skipped".into(), 0))
        );
    }

    /// 失败后超过宽限：按策略静默，不再重试。
    #[test]
    fn failed_beyond_grace_is_silent() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = iso(3540);
        let task = db
            .create_task(&input("失败超宽限", Some(&due), vec![3600]))
            .unwrap();
        let action = engine.run_once(t0()).unwrap().remove(0);
        let transport = FakeTransport::default();
        transport.set_fail_next(true);
        engine.deliver(&action, t0(), &transport).unwrap();

        let actions = engine.run_once(at(MISSED_GRACE_SECS + 1)).unwrap();
        assert!(actions.is_empty());
        assert_eq!(transport.call_count(), 1);
        assert_eq!(
            db.test_event_state(task.id, 3600, &due),
            Some(("skipped".into(), 1))
        );
    }

    /// 发送前超过宽限（领取后进程挂起很久）：静默 skipped。
    #[test]
    fn deliver_beyond_grace_is_skipped() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = iso(3540);
        let task = db
            .create_task(&input("发送超宽限", Some(&due), vec![3600]))
            .unwrap();
        let action = engine.run_once(t0()).unwrap().remove(0);

        let transport = FakeTransport::default();
        assert_eq!(
            engine
                .deliver(&action, at(MISSED_GRACE_SECS + 1), &transport)
                .unwrap(),
            DeliveryOutcome::Skipped
        );
        assert_eq!(transport.call_count(), 0);
        assert_eq!(
            db.test_event_state(task.id, 3600, &due),
            Some(("skipped".into(), 0))
        );
    }

    /// 跨重启持久去重：文件数据库，drop 连接后重开，已领取的不再重复触发。
    #[test]
    fn dedup_survives_process_restart_on_file_db() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("restart.db");
        let due = iso(3540);
        let task_id = {
            let db = Database::open(&path).unwrap();
            db.ensure_default_categories().unwrap();
            let t = db
                .create_task(&input("跨重启", Some(&due), vec![3600]))
                .unwrap();
            let engine = ReminderEngine::new(db.clone());
            assert_eq!(engine.run_once(t0()).unwrap().len(), 1);
            t.id
        };
        let db2 = Database::open(&path).unwrap();
        let engine2 = ReminderEngine::new(db2.clone());
        assert!(
            engine2.run_once(at(1)).unwrap().is_empty(),
            "重启后不应重复触发同一提醒"
        );
        assert!(db2.test_event_state(task_id, 3600, &due).is_some());
    }

    /// failed 状态跨重启保留，重启后仍遵守退避与上限。
    #[test]
    fn failed_state_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("retry.db");
        let due = iso(3540);
        {
            let db = Database::open(&path).unwrap();
            db.ensure_default_categories().unwrap();
            db.create_task(&input("跨重启重试", Some(&due), vec![3600]))
                .unwrap();
            let engine = ReminderEngine::new(db.clone());
            let action = engine.run_once(t0()).unwrap().remove(0);
            let transport = FakeTransport::default();
            transport.set_fail_next(true);
            engine.deliver(&action, t0(), &transport).unwrap();
        }
        let db2 = Database::open(&path).unwrap();
        let engine2 = ReminderEngine::new(db2.clone());
        assert!(engine2.run_once(at(29)).unwrap().is_empty());
        assert_eq!(engine2.run_once(at(30)).unwrap().len(), 1);
    }

    #[test]
    fn notifications_disabled_claims_as_skipped() {
        let db = db();
        disable_notifications(&db);
        let engine = ReminderEngine::new(db.clone());
        let due = iso(3540);
        let task = db
            .create_task(&input("静音任务", Some(&due), vec![3600]))
            .unwrap();

        assert!(engine.run_once(t0()).unwrap().is_empty());
        assert_eq!(
            db.test_event_state(task.id, 3600, &due),
            Some(("skipped".into(), 0))
        );
    }

    #[test]
    fn multiple_offsets_fire_independently() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = iso(1200);
        let task = db
            .create_task(&input("多级提醒", Some(&due), vec![1800, 600]))
            .unwrap();

        let actions = engine.run_once(t0()).unwrap();
        assert_eq!(actions, vec![action_for(task.id, 1800, &due, true)]);

        let actions = engine.run_once(at(660)).unwrap();
        assert_eq!(actions, vec![action_for(task.id, 600, &due, true)]);
    }

    #[test]
    fn reminder_fires_at_deadline_for_zero_offset() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = iso(-30);
        let task = db
            .create_task(&input("截止提醒", Some(&due), vec![0]))
            .unwrap();

        let actions = engine.run_once(t0()).unwrap();
        assert_eq!(actions, vec![action_for(task.id, 0, &due, true)]);
    }

    #[test]
    fn submitted_notification_content_contains_body() {
        let db = db();
        let engine = ReminderEngine::new(db.clone());
        let due = iso(3540);
        db.create_task(&input("通知内容", Some(&due), vec![3600]))
            .unwrap();
        let action = engine.run_once(t0()).unwrap().remove(0);
        let transport = FakeTransport::default();
        engine.deliver(&action, t0(), &transport).unwrap();
        let content = transport.last_content().unwrap();
        assert_eq!(content.title, "CheckIt");
        assert_eq!(content.body, "「通知内容」59分钟后截止");
    }

    #[test]
    fn notification_body_three_phases() {
        let now = t0();
        let due = now + chrono::Duration::seconds(90 * 60);
        assert_eq!(notification_body("任务A", due, now), "「任务A」1小时30分钟后截止");
        assert_eq!(notification_body("任务B", now, now), "「任务B」已逾期 0秒");
        let overdue = now - chrono::Duration::seconds(2 * 3600 + 5 * 60);
        assert_eq!(
            notification_body("任务C", overdue, now),
            "「任务C」已逾期 2小时5分钟"
        );
    }

    #[test]
    fn duration_text_formatting() {
        assert_eq!(duration_text(45), "45秒");
        assert_eq!(duration_text(300), "5分钟");
        assert_eq!(duration_text(3 * 3600 + 20 * 60), "3小时20分钟");
        assert_eq!(duration_text(2 * 86400 + 5 * 3600), "2天5小时");
    }
}

#[cfg(test)]
mod language_tests {
 use super::*;
 #[test]
 fn english_notification_uses_actual_due_time_and_preserves_user_title() {
 let now=Utc::now();let due=now+chrono::Duration::minutes(90);
 assert_eq!(notification_body_localized("学习",due,now,"en"),"“学习” is due in 1h 30m");
 assert_eq!(notification_body_localized("学习",now-chrono::Duration::seconds(30),now,"en"),"“学习” is overdue by 30s");
 }
}
