//! SQLite 数据库模块：连接管理、迁移、CRUD。
//!
//! 设计要点：
//! - 数据库文件位于操作系统应用数据目录（tauri > app_data_dir），绝不写安装目录
//! - 使用 WAL 模式支持多窗口并发读写
//! - 全部查询使用参数绑定，杜绝 SQL 注入
//! - 日期统一存储 RFC3339 UTC 字符串
//! - 多步业务写入（任务+提醒、设置批量键）包裹在显式事务中，失败整体回滚
//! - reminder_events 是提醒去重 + 状态机表（pending/submitted/failed/skipped/cancelled），
//!   以 (task_id, offset_seconds, due_at_snapshot) 为唯一键：改期后旧记录自然失效

use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use rusqlite::{Connection, Row};
use serde::{Deserialize, Serialize};

use crate::models::task::{normalize_reminder_offsets, Priority, Task, TaskInput, TaskStatus};
use crate::models::{AppSettings, Category, SettingsPatch};

pub mod error;
mod popup;
pub use popup::PopupReminder;

pub use error::DbError;

pub type DbResult<T> = Result<T, DbError>;

#[cfg(test)]
mod tests;

/// 提醒事件状态（精确语义见 docs/DATABASE.md）：
/// - pending   : 已领取、等待后台提交（即 queued；进程崩溃后按 stale 规则恢复）
/// - submitted : 底层系统通知 API 已受理（不是"用户已看到横幅"）
/// - failed    : 底层提交失败；attempts < 上限且未超宽限时可重试
/// - skipped   : 静默终止（通知关闭 / 超过 15 分钟补发宽限），不再发送
/// - cancelled : 动作失效（任务完成、删除、改期或提醒 offset 被移除）
pub const REMINDER_STATES: [&str; 5] = ["pending", "submitted", "failed", "skipped", "cancelled"];

/// 全局共享的数据库连接（内部可变性 + 互斥）
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

/// 一次性加载的完整应用状态
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AppState {
    pub tasks: Vec<Task>,
    pub categories: Vec<Category>,
    pub settings: AppSettings,
}

impl Database {
    /// 打开（或创建）数据库并执行迁移。
    pub fn open(path: &Path) -> DbResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| DbError::Io(format!("创建数据目录失败: {e}")))?;
        }
        let conn = Connection::open(path)
            .map_err(|e| DbError::Sqlite(format!("打开数据库失败: {e}")))?;
        Self::init_connection(conn, path)
    }

    /// 内存数据库（测试用）
    pub fn open_in_memory() -> DbResult<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| DbError::Sqlite(format!("打开内存数据库失败: {e}")))?;
        Self::init_connection(conn, Path::new(":memory:"))
    }

    fn init_connection(conn: Connection, path: &Path) -> DbResult<Self> {
        // WAL 提升并发性能；busy_timeout 避免多窗口写入时立即报错
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_millis(2000))?;

        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate(path)?;
        Ok(db)
    }

    /// 版本化迁移。schema_migrations 记录已应用版本，逐个顺序执行。
    fn migrate(&self, path: &Path) -> DbResult<()> {
        let conn = self.conn.lock();

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            );",
        )?;

        let current: i64 = conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )?;

        // —— 迁移 v1：初始表结构 ——
        if current < 1 {
            conn.execute_batch(
                "BEGIN;
                CREATE TABLE categories (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL UNIQUE,
                    color TEXT NOT NULL DEFAULT '#6366f1',
                    sort_order INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE TABLE tasks (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    title TEXT NOT NULL,
                    description TEXT NOT NULL DEFAULT '',
                    category_id INTEGER REFERENCES categories(id) ON DELETE SET NULL,
                    priority TEXT NOT NULL DEFAULT 'medium'
                        CHECK (priority IN ('high','medium','low')),
                    status TEXT NOT NULL DEFAULT 'pending'
                        CHECK (status IN ('pending','completed')),
                    due_at TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    completed_at TEXT
                );
                CREATE INDEX idx_tasks_status_due ON tasks(status, due_at);
                CREATE INDEX idx_tasks_category ON tasks(category_id);
                CREATE INDEX idx_tasks_due_at ON tasks(due_at);

                CREATE TABLE reminders (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                    offset_seconds INTEGER NOT NULL CHECK (offset_seconds >= 0),
                    UNIQUE (task_id, offset_seconds)
                );
                CREATE INDEX idx_reminders_task ON reminders(task_id);

                CREATE TABLE reminder_events (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    task_id INTEGER NOT NULL,
                    offset_seconds INTEGER NOT NULL,
                    fired_at TEXT NOT NULL,
                    UNIQUE (task_id, offset_seconds)
                );
                CREATE INDEX idx_reminder_events_task ON reminder_events(task_id);

                CREATE TABLE app_settings (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );

                INSERT INTO schema_migrations (version, applied_at)
                VALUES (1, strftime('%Y-%m-%dT%H:%M:%fZ','now'));
                COMMIT;",
            )?;
            log::info!("数据库迁移 v1 已应用 ({})", path.display());
        }

        // —— 迁移 v2：reminder_events 升级为 due_at 快照 + 状态机 ——
        // 背景：v1 先记后发导致崩溃漏发无法恢复；(task_id, offset) 键在改期后互相抑制。
        // v2 键为 (task_id, offset_seconds, due_at_snapshot)，改期产生新键、旧键自然失效；
        // state 区分 pending/sent/failed/skipped/cancelled，pending 崩溃后可恢复重试。
        // 旧数据兼容：历史行视为已提交（sent），snapshot 回填任务当前 due_at，
        // 对未改期的任务保持与 v1 完全一致的去重效果。
        // 注意：条件是 current < 2（新建库 current=0 但 v1 刚在本次打开中建好，也需应用 v2）。
        if current < 2 {
            conn.execute_batch(
                "BEGIN;
                CREATE TABLE reminder_events_v2 (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    task_id INTEGER NOT NULL,
                    offset_seconds INTEGER NOT NULL,
                    due_at_snapshot TEXT NOT NULL,
                    state TEXT NOT NULL DEFAULT 'pending'
                        CHECK (state IN ('pending','sent','failed','skipped','cancelled')),
                    attempts INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    UNIQUE (task_id, offset_seconds, due_at_snapshot)
                );

                INSERT INTO reminder_events_v2
                    (task_id, offset_seconds, due_at_snapshot, state, attempts, created_at, updated_at)
                SELECT e.task_id, e.offset_seconds, COALESCE(t.due_at, ''), 'sent', 0, e.fired_at, e.fired_at
                FROM reminder_events e LEFT JOIN tasks t ON t.id = e.task_id;

                DROP TABLE reminder_events;
                ALTER TABLE reminder_events_v2 RENAME TO reminder_events;
                CREATE INDEX idx_reminder_events_task ON reminder_events(task_id);
                CREATE INDEX idx_reminder_events_state ON reminder_events(state);

                INSERT INTO schema_migrations (version, applied_at)
                VALUES (2, strftime('%Y-%m-%dT%H:%M:%fZ','now'));
                COMMIT;",
            )?;
            log::info!("数据库迁移 v2（提醒状态机）已应用 ({})", path.display());
        }

        // —— 迁移 v3：sent → submitted（语义精确化） ——
        // 背景：旧的 state='sent' 实际只表示 notify-rust 提交成功（OS 受理），
        // 但名称容易被误解为"用户已收到"。v3 起使用 submitted 明确"系统 API 已受理"。
        // 同时保留 failed/skipped/cancelled/pending；历史数据逐行迁移，不删除任何事件记录。
        // 需要重建表以更新 CHECK 约束（SQLite 不支持直接修改 CHECK）。
        if current < 3 {
            conn.execute_batch(
                "BEGIN;
                CREATE TABLE reminder_events_v3 (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    task_id INTEGER NOT NULL,
                    offset_seconds INTEGER NOT NULL,
                    due_at_snapshot TEXT NOT NULL,
                    state TEXT NOT NULL DEFAULT 'pending'
                        CHECK (state IN ('pending','submitted','failed','skipped','cancelled')),
                    attempts INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    UNIQUE (task_id, offset_seconds, due_at_snapshot)
                );

                INSERT INTO reminder_events_v3
                    (id, task_id, offset_seconds, due_at_snapshot, state, attempts, created_at, updated_at)
                SELECT id, task_id, offset_seconds, due_at_snapshot,
                       CASE state WHEN 'sent' THEN 'submitted' ELSE state END,
                       attempts, created_at, updated_at
                FROM reminder_events;

                DROP TABLE reminder_events;
                ALTER TABLE reminder_events_v3 RENAME TO reminder_events;
                CREATE INDEX idx_reminder_events_task ON reminder_events(task_id);
                CREATE INDEX idx_reminder_events_state ON reminder_events(state);

                INSERT INTO schema_migrations (version, applied_at)
                VALUES (3, strftime('%Y-%m-%dT%H:%M:%fZ','now'));
                COMMIT;",
            )?;
            log::info!("数据库迁移 v3（submitted 语义）已应用 ({})", path.display());
        }

        // 弹窗收件箱：先持久化再展示，未确认的提醒重启后仍可恢复。
        if current < 4 {
            conn.execute_batch(
                "BEGIN;
                CREATE TABLE popup_reminders (
                    event_id INTEGER PRIMARY KEY REFERENCES reminder_events(id) ON DELETE CASCADE,
                    body TEXT NOT NULL,
                    dismissed INTEGER NOT NULL DEFAULT 0 CHECK (dismissed IN (0, 1))
                );
                INSERT INTO schema_migrations (version, applied_at)
                VALUES (4, strftime('%Y-%m-%dT%H:%M:%fZ','now'));
                COMMIT;",
            )?;
        }

        Ok(())
    }

    /// 初始化默认分类（仅当分类表为空时）。
    pub fn ensure_default_categories(&self) -> DbResult<()> {
        let mut conn = self.conn.lock();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0))?;
        if count > 0 {
            return Ok(());
        }
        let now = now_iso();
        let tx = conn.transaction()?;
        let defaults = [
            ("学习", "#6366f1", 0),
            ("工作", "#0ea5e9", 1),
            ("会议", "#f59e0b", 2),
            ("生活", "#10b981", 3),
            ("其他", "#64748b", 4),
        ];
        for (name, color, order) in defaults {
            tx.execute(
                "INSERT INTO categories (name, color, sort_order, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
                rusqlite::params![name, color, order, now],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    // ===== 任务 =====

    fn row_to_task(row: &Row) -> rusqlite::Result<Task> {
        let priority: String = row.get("priority")?;
        let status: String = row.get("status")?;
        Ok(Task {
            id: row.get("id")?,
            title: row.get("title")?,
            description: row.get("description")?,
            category_id: row.get("category_id")?,
            priority: Priority::parse(&priority).unwrap_or(Priority::Medium),
            status: if status == "completed" {
                TaskStatus::Completed
            } else {
                TaskStatus::Pending
            },
            due_at: row.get("due_at")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
            completed_at: row.get("completed_at")?,
            reminders: Vec::new(),
        })
    }

    /// 批量加载多个任务的提醒配置（单次查询，避免逐任务 N+1）。
    /// 读取失败显式传播，不再吞错返回空。
    fn load_reminders_batch(conn: &Connection, task_ids: &[i64]) -> DbResult<std::collections::HashMap<i64, Vec<i64>>> {
        let mut map = std::collections::HashMap::with_capacity(task_ids.len());
        if task_ids.is_empty() {
            return Ok(map);
        }
        let placeholders = task_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT task_id, offset_seconds FROM reminders WHERE task_id IN ({placeholders}) ORDER BY task_id, offset_seconds"
        );
        let mut stmt = conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> =
            task_ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
        let rows = stmt.query_map(params.as_slice(), |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (task_id, offset) = row?;
            map.entry(task_id).or_default().push(offset);
        }
        Ok(map)
    }

    fn load_tasks_where(
        &self,
        conn: &Connection,
        where_clause: &str,
        params: &[&dyn rusqlite::ToSql],
    ) -> DbResult<Vec<Task>> {
        let sql = format!(
            "SELECT id, title, description, category_id, priority, status, due_at, created_at, updated_at, completed_at
             FROM tasks {where_clause} ORDER BY
             CASE WHEN status = 'pending' THEN 0 ELSE 1 END,
             due_at IS NULL, due_at ASC, id ASC"
        );
        let mut stmt = conn.prepare(&sql)?;
        // 行解析失败立即返回错误，不静默丢行
        let tasks: Vec<Task> = stmt
            .query_map(params, Self::row_to_task)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let ids: Vec<i64> = tasks.iter().map(|t| t.id).collect();
        let mut reminder_map = Self::load_reminders_batch(conn, &ids)?;
        Ok(tasks
            .into_iter()
            .map(|mut t| {
                t.reminders = reminder_map.remove(&t.id).unwrap_or_default();
                t
            })
            .collect())
    }

    pub fn get_app_state(&self) -> DbResult<AppState> {
        let conn = self.conn.lock();
        Ok(AppState {
            tasks: self.load_tasks_where(&conn, "", &[])?,
            categories: Self::list_categories_inner(&conn)?,
            settings: Self::load_settings_inner(&conn)?,
        })
    }

    pub fn list_tasks(&self) -> DbResult<Vec<Task>> {
        let conn = self.conn.lock();
        self.load_tasks_where(&conn, "", &[])
    }

    pub fn list_tasks_by_ids(&self, ids: &[i64]) -> DbResult<Vec<Task>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.conn.lock();
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!("WHERE id IN ({placeholders})");
        let params: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
        self.load_tasks_where(&conn, &sql, &params)
    }

    /// 需要调度提醒的待完成任务（有截止时间 + 有提醒配置）
    pub fn list_schedulable_tasks(&self) -> DbResult<Vec<Task>> {
        Ok(self
            .list_tasks()?
            .into_iter()
            .filter(|t| t.status == TaskStatus::Pending && t.due_at.is_some() && !t.reminders.is_empty())
            .collect())
    }

    pub fn get_task(&self, id: i64) -> DbResult<Task> {
        let conn = self.conn.lock();
        let tasks = self.load_tasks_where(&conn, "WHERE id = ?1", &[&id])?;
        tasks.into_iter().next().ok_or(DbError::NotFound)
    }

    /// 创建任务 + 提醒配置（单事务：任一步失败整体回滚，不留半成品任务）。
    pub fn create_task(&self, input: &TaskInput) -> DbResult<Task> {
        let title = input.title.trim();
        if title.is_empty() {
            return Err(DbError::Validation("任务名称不能为空".into()));
        }
        validate_due(&input.due_at)?;
        let reminders = normalize_reminder_offsets(&input.reminders);
        let now = now_iso();

        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO tasks (title, description, category_id, priority, status, due_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?6, ?6)",
            rusqlite::params![
                title,
                input.description.trim(),
                input.category_id,
                input.priority.as_str(),
                input.due_at,
                now
            ],
        )?;
        let id = tx.last_insert_rowid();
        for &offset in &reminders {
            tx.execute(
                "INSERT INTO reminders (task_id, offset_seconds) VALUES (?1, ?2)",
                rusqlite::params![id, offset],
            )?;
        }
        tx.commit()?;
        drop(conn);
        self.get_task(id)
    }

    /// 更新任务 + 替换提醒配置（单事务）。
    ///
    /// 不再删除 reminder_events：v2 起事件以 due_at 快照为键，
    /// 改期产生新的快照键、旧键对应的 pending 动作会在发送前校验中自然失效。
    pub fn update_task(&self, id: i64, input: &TaskInput) -> DbResult<Task> {
        let title = input.title.trim();
        if title.is_empty() {
            return Err(DbError::Validation("任务名称不能为空".into()));
        }
        validate_due(&input.due_at)?;
        let reminders = normalize_reminder_offsets(&input.reminders);
        let now = now_iso();

        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        // 校验任务存在（在事务内完成，避免与并发删除竞态）
        let exists: bool = tx
            .query_row("SELECT COUNT(*) FROM tasks WHERE id = ?1", [id], |r| {
                r.get::<_, i64>(0)
            })
            .map(|c| c > 0)?;
        if !exists {
            return Err(DbError::NotFound);
        }
        tx.execute(
            "UPDATE tasks SET title = ?1, description = ?2, category_id = ?3, priority = ?4, due_at = ?5, updated_at = ?6
             WHERE id = ?7",
            rusqlite::params![title, input.description.trim(), input.category_id, input.priority.as_str(), input.due_at, now, id],
        )?;
        tx.execute("DELETE FROM reminders WHERE task_id = ?1", [id])?;
        for &offset in &reminders {
            tx.execute(
                "INSERT INTO reminders (task_id, offset_seconds) VALUES (?1, ?2)",
                rusqlite::params![id, offset],
            )?;
        }
        tx.commit()?;
        drop(conn);
        self.get_task(id)
    }

    /// 设置任务完成状态。
    /// 已触发/已记录的提醒事件保留（快照键天然区分，恢复待办后未来的提醒照常触发）。
    pub fn set_task_status(&self, id: i64, completed: bool) -> DbResult<Task> {
        let now = now_iso();
        let conn = self.conn.lock();
        let affected = conn.execute(
            "UPDATE tasks SET status = ?1, completed_at = ?2, updated_at = ?3 WHERE id = ?4",
            rusqlite::params![
                if completed { "completed" } else { "pending" },
                if completed { Some(&now) } else { None },
                now,
                id
            ],
        )?;
        if affected == 0 {
            return Err(DbError::NotFound);
        }
        drop(conn);
        self.get_task(id)
    }

    /// 删除任务（提醒配置级联删除；reminder_events 保留历史不影响去重——任务删除后不会再有新事件）。
    pub fn delete_task(&self, id: i64) -> DbResult<()> {
        let conn = self.conn.lock();
        let affected = conn.execute("DELETE FROM tasks WHERE id = ?1", [id])?;
        if affected == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }

    // ===== 提醒事件（去重 + 状态机）=====

    /// 领取一个到期提醒：INSERT OR IGNORE (task_id, offset, due_snapshot) 行。
    /// 返回 Some(是否需要通知) 表示首次领取成功；None 表示已存在（已领取过）。
    /// grace 内 → pending（queued，待提交）；超宽限/通知关闭 → skipped（静默标记）。
    ///
    /// `now` 由调用方（统一时钟）传入，扫描、恢复与状态时间戳使用同一时间来源。
    pub fn claim_reminder(
        &self,
        task_id: i64,
        offset_seconds: i64,
        due_at_snapshot: &str,
        quiet: bool,
        now: DateTime<Utc>,
    ) -> DbResult<Option<bool>> {
        let conn = self.conn.lock();
        let now = format_iso(now);
        let state = if quiet { "skipped" } else { "pending" };
        let affected = conn.execute(
            "INSERT OR IGNORE INTO reminder_events
                (task_id, offset_seconds, due_at_snapshot, state, attempts, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?5)",
            rusqlite::params![task_id, offset_seconds, due_at_snapshot, state, now],
        )?;
        if affected == 0 {
            return Ok(None);
        }
        // 返回“是否为待通知领取”（skipped 也算首次，但不需要通知）
        Ok(Some(!quiet))
    }

    /// 标记事件状态（pending→submitted/failed/skipped/cancelled、failed→重试等）。
    /// `now` 同时用于 updated_at，保证崩溃检测/重试间隔与扫描时钟一致。
    pub fn set_reminder_state(
        &self,
        task_id: i64,
        offset_seconds: i64,
        due_at_snapshot: &str,
        state: &str,
        bump_attempts: bool,
        now: DateTime<Utc>,
    ) -> DbResult<()> {
        debug_assert!(REMINDER_STATES.contains(&state));
        let conn = self.conn.lock();
        let affected = conn.execute(
            "UPDATE reminder_events SET state = ?4,
                attempts = CASE WHEN ?5 THEN attempts + 1 ELSE attempts END,
                updated_at = ?6
             WHERE task_id = ?1 AND offset_seconds = ?2 AND due_at_snapshot = ?3",
            rusqlite::params![
                task_id,
                offset_seconds,
                due_at_snapshot,
                state,
                bump_attempts,
                format_iso(now)
            ],
        )?;
        if affected == 0 {
            log::warn!(
                "提醒事件不存在，状态更新被忽略: task={task_id} offset={offset_seconds} state={state}"
            );
        }
        Ok(())
    }

    /// 取回需要（重）尝试的行（全部与调用方传入的同一 `now` 比较）：
    /// - pending 且已停留超过 stale_secs（进程在提交前崩溃的痕迹）
    /// - failed 且 attempts < max_attempts，且距上次失败已过 retry_backoff_secs
    /// 调用方还需在发送前做统一有效性校验（任务/offset/开关/宽限）。
    pub fn recoverable_reminders(
        &self,
        stale_secs: i64,
        retry_backoff_secs: i64,
        max_attempts: i32,
        now: DateTime<Utc>,
    ) -> DbResult<Vec<ReminderEventRow>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT task_id, offset_seconds, due_at_snapshot, state, attempts,
                    strftime('%s', updated_at) AS upd
             FROM reminder_events
             WHERE (state = 'pending' AND ?1 - upd >= ?2)
                OR (state = 'failed' AND attempts < ?3 AND ?1 - upd >= ?4)",
        )?;
        let rows = stmt
            .query_map(
                rusqlite::params![
                    now.timestamp(),
                    stale_secs,
                    max_attempts,
                    retry_backoff_secs
                ],
                row_to_event,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    // ===== 分类 =====

    fn row_to_category(row: &Row) -> rusqlite::Result<Category> {
        Ok(Category {
            id: row.get("id")?,
            name: row.get("name")?,
            color: row.get("color")?,
            sort_order: row.get("sort_order")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }

    fn list_categories_inner(conn: &Connection) -> DbResult<Vec<Category>> {
        let mut stmt = conn.prepare(
            "SELECT id, name, color, sort_order, created_at, updated_at FROM categories ORDER BY sort_order, id",
        )?;
        let cats = stmt
            .query_map([], Self::row_to_category)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(cats)
    }

    pub fn list_categories(&self) -> DbResult<Vec<Category>> {
        let conn = self.conn.lock();
        Self::list_categories_inner(&conn)
    }

    pub fn create_category(&self, name: &str, color: &str) -> DbResult<Category> {
        let name = name.trim();
        if name.is_empty() {
            return Err(DbError::Validation("分类名称不能为空".into()));
        }
        validate_color(color)?;
        let now = now_iso();
        let conn = self.conn.lock();
        let max_order: i64 =
            conn.query_row("SELECT COALESCE(MAX(sort_order), -1) FROM categories", [], |r| r.get(0))?;
        conn.execute(
            "INSERT INTO categories (name, color, sort_order, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
            rusqlite::params![name, color, max_order + 1, now],
        )?;
        let id = conn.last_insert_rowid();
        drop(conn);
        self.list_categories()?
            .into_iter()
            .find(|c| c.id == id)
            .ok_or(DbError::NotFound)
    }

    pub fn update_category(&self, id: i64, name: &str, color: &str) -> DbResult<Category> {
        let name = name.trim();
        if name.is_empty() {
            return Err(DbError::Validation("分类名称不能为空".into()));
        }
        validate_color(color)?;
        let now = now_iso();
        let conn = self.conn.lock();
        let affected = conn.execute(
            "UPDATE categories SET name = ?1, color = ?2, updated_at = ?3 WHERE id = ?4",
            rusqlite::params![name, color, now, id],
        )?;
        if affected == 0 {
            return Err(DbError::NotFound);
        }
        drop(conn);
        self.list_categories()?
            .into_iter()
            .find(|c| c.id == id)
            .ok_or(DbError::NotFound)
    }

    /// 删除分类；其下任务通过外键 ON DELETE SET NULL 自动变为未分类。
    pub fn delete_category(&self, id: i64) -> DbResult<()> {
        let conn = self.conn.lock();
        let affected = conn.execute("DELETE FROM categories WHERE id = ?1", [id])?;
        if affected == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }

    // ===== 设置 =====

    fn load_settings_inner(conn: &Connection) -> DbResult<AppSettings> {
        let mut settings = AppSettings::default();
        let mut stmt = conn.prepare("SELECT key, value FROM app_settings")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (key, value) = row?;
            match key.as_str() {
                "theme" => settings.theme = value,
                "glass_enabled" => settings.glass_enabled = value == "true",
                "glass_opacity" => {
                    if let Ok(v) = value.parse() {
                        settings.glass_opacity = v
                    }
                }
                "always_on_top" => settings.always_on_top = value == "true",
                "close_to_tray" => settings.close_to_tray = value == "true",
                "notifications_enabled" => settings.notifications_enabled = value == "true",
                "popup_reminders_enabled" => settings.popup_reminders_enabled = value == "true",
                "widget_display_count" => {
                    if let Ok(v) = value.parse() {
                        settings.widget_display_count = v
                    }
                }
                "launch_at_login" => settings.launch_at_login = value == "true",
                "edge_dock_enabled" => settings.edge_dock_enabled = value == "true",
                "default_reminder_offsets" => {
                    if let Ok(v) = serde_json::from_str(&value) {
                        settings.default_reminder_offsets = v
                    }
                }
                _ => {}
            }
        }
        Ok(settings.sanitized())
    }

    pub fn load_settings(&self) -> DbResult<AppSettings> {
        let conn = self.conn.lock();
        Self::load_settings_inner(&conn)
    }

    /// 写入全部设置键（调用方必须已处于事务中并由其提交/回滚）。
    fn save_settings_inner(conn: &Connection, settings: &AppSettings) -> DbResult<()> {
        let s = settings.clone().sanitized();
        let entries: [(&str, String); 11] = [
            ("theme", s.theme.clone()),
            ("glass_enabled", s.glass_enabled.to_string()),
            ("glass_opacity", s.glass_opacity.to_string()),
            ("always_on_top", s.always_on_top.to_string()),
            ("close_to_tray", s.close_to_tray.to_string()),
            ("notifications_enabled", s.notifications_enabled.to_string()),
            ("popup_reminders_enabled", s.popup_reminders_enabled.to_string()),
            ("widget_display_count", s.widget_display_count.to_string()),
            ("launch_at_login", s.launch_at_login.to_string()),
            ("edge_dock_enabled", s.edge_dock_enabled.to_string()),
            (
                "default_reminder_offsets",
                serde_json::to_string(&s.default_reminder_offsets).unwrap_or_else(|_| "[]".into()),
            ),
        ];
        for (key, value) in entries {
            conn.execute(
                "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                rusqlite::params![key, value],
            )?;
        }
        Ok(())
    }

    /// 批量保存设置（单事务：任一键写入失败整体回滚，不留半套设置）。
    pub fn save_settings(&self, settings: &AppSettings) -> DbResult<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        Self::save_settings_inner(&tx, settings)?;
        tx.commit()?;
        Ok(())
    }

    /// **原子设置补丁**：在同一个互斥锁/事务内完成"读取当前值 → 应用修改 → 保存"。
    ///
    /// 这是设置写入的唯一底层入口。任何"先 load_settings 再 save_settings"的组合
    /// 都会在当前值与保存之间释放锁，导致并发补丁互相覆盖未修改字段。
    pub fn update_settings_atomic<F>(&self, apply: F) -> DbResult<AppSettings>
    where
        F: FnOnce(&mut AppSettings),
    {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let mut settings = Self::load_settings_inner(&tx)?;
        apply(&mut settings);
        let settings = settings.sanitized();
        Self::save_settings_inner(&tx, &settings)?;
        tx.commit()?;
        Ok(settings)
    }

    /// 应用设置补丁（只覆盖 patch 中出现的字段）。
    pub fn apply_settings_patch(&self, patch: &SettingsPatch) -> DbResult<AppSettings> {
        self.update_settings_atomic(|settings| patch.apply_to(settings))
    }

    /// 原子翻转置顶设置。窗口调用由调用方在锁外完成。
    pub fn toggle_always_on_top(&self) -> DbResult<AppSettings> {
        self.update_settings_atomic(|settings| settings.always_on_top = !settings.always_on_top)
    }

    // ===== 窗口几何（物理像素存储，跨 DPI 一致）=====

    /// 读取单个设置键的原始字符串值（不存在或读取失败返回 None）。
    pub fn get_setting_value(&self, key: &str) -> Option<String> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            [key],
            |r| r.get::<_, String>(0),
        )
        .ok()
    }

    /// 批量写入任意设置键（单事务；用于窗口模式 / 圆圈锚点等非 AppSettings 字段）。
    pub fn set_setting_entries(&self, entries: &[(String, String)]) -> DbResult<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        for (key, value) in entries {
            tx.execute(
                "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                rusqlite::params![key, value],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// 读取窗口几何（物理像素；v2 键名带 _px 后缀，与旧逻辑像素键区分）
    pub fn load_window_geometry(&self, prefix: &str) -> Option<(f64, f64, f64, f64)> {
        let conn = self.conn.lock();
        let get = |key: &str| -> Option<f64> {
            conn.query_row(
                "SELECT value FROM app_settings WHERE key = ?1",
                [key],
                |r| r.get::<_, String>(0),
            )
            .ok()?
            .parse()
            .ok()
        };
        let x = get(&format!("{prefix}_px_x"))?;
        let y = get(&format!("{prefix}_px_y"))?;
        let w = get(&format!("{prefix}_px_w"))?;
        let h = get(&format!("{prefix}_px_h"))?;
        Some((x, y, w, h))
    }

    /// 保存窗口几何（物理像素；单事务写入四个键）
    pub fn save_window_geometry(&self, prefix: &str, x: f64, y: f64, w: f64, h: f64) -> DbResult<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let entries = [
            (format!("{prefix}_px_x"), x),
            (format!("{prefix}_px_y"), y),
            (format!("{prefix}_px_w"), w),
            (format!("{prefix}_px_h"), h),
        ];
        for (key, value) in entries {
            tx.execute(
                "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                rusqlite::params![key, value.to_string()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

/// 提醒事件行（恢复扫描用）
#[derive(Debug, Clone)]
pub struct ReminderEventRow {
    pub task_id: i64,
    pub offset_seconds: i64,
    pub due_at_snapshot: String,
    pub state: String,
    pub attempts: i32,
}

fn row_to_event(row: &Row) -> rusqlite::Result<ReminderEventRow> {
    Ok(ReminderEventRow {
        task_id: row.get("task_id")?,
        offset_seconds: row.get("offset_seconds")?,
        due_at_snapshot: row.get("due_at_snapshot")?,
        state: row.get("state")?,
        attempts: row.get("attempts")?,
    })
}

/// 指定 UTC 时间的 RFC3339 字符串（提醒状态机使用调用方时钟）
pub fn format_iso(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// 当前 UTC 时间的 RFC3339 字符串（一般 CRUD 使用）
pub fn now_iso() -> String {
    format_iso(Utc::now())
}

/// 当前 Unix 秒（整数）
pub fn unix_now() -> i64 {
    Utc::now().timestamp()
}

/// 测试辅助：暴露内部连接（仅测试可见）
#[cfg(test)]
impl Database {
    pub(crate) fn lock_conn_for_test(&self) -> parking_lot::MutexGuard<'_, Connection> {
        self.conn.lock()
    }
}

/// 校验 due_at 为合法 RFC3339 且可解析为时间。
fn validate_due(due: &Option<String>) -> DbResult<()> {
    if let Some(s) = due {
        chrono::DateTime::parse_from_rfc3339(s)
            .map_err(|_| DbError::Validation(format!("无效的截止时间格式: {s}")))?;
    }
    Ok(())
}

/// 校验颜色格式 #RRGGBB
fn validate_color(color: &str) -> DbResult<()> {
    let bytes = color.as_bytes();
    let hex_ok =
        color.len() == 7 && bytes[0] == b'#' && bytes[1..].iter().all(|b| b.is_ascii_hexdigit());
    if !hex_ok {
        return Err(DbError::Validation(format!("无效的颜色格式: {color}")));
    }
    Ok(())
}
