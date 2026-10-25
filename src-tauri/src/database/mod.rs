//! SQLite 数据库模块：连接管理、迁移、CRUD。
//!
//! 设计要点：
//! - 数据库文件位于操作系统应用数据目录（tauri > app_data_dir），绝不写安装目录
//! - 使用 WAL 模式支持多窗口并发读写
//! - 全部查询使用参数绑定，杜绝 SQL 注入
//! - 日期统一存储 RFC3339 UTC 字符串

use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::{Connection, Row};
use serde::{Deserialize, Serialize};

use crate::models::task::{normalize_reminder_offsets, Priority, Task, TaskInput, TaskStatus};
use crate::models::{AppSettings, Category};

pub mod error;

pub use error::DbError;

pub type DbResult<T> = Result<T, DbError>;

#[cfg(test)]
mod tests;

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

    /// 版本化迁移。schema_migrations 记录已应用版本。
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

        Ok(())
    }

    /// 初始化默认分类（仅当分类表为空时）。
    pub fn ensure_default_categories(&self) -> DbResult<()> {
        let conn = self.conn.lock();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0))?;
        if count > 0 {
            return Ok(());
        }
        let now = now_iso();
        let defaults = [
            ("学习", "#6366f1", 0),
            ("工作", "#0ea5e9", 1),
            ("会议", "#f59e0b", 2),
            ("生活", "#10b981", 3),
            ("其他", "#64748b", 4),
        ];
        for (name, color, order) in defaults {
            conn.execute(
                "INSERT INTO categories (name, color, sort_order, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
                rusqlite::params![name, color, order, now],
            )?;
        }
        Ok(())
    }

    // ===== 任务 =====

    fn row_to_task(row: &Row) -> rusqlite::Result<Task> {
        Ok(Task {
            id: row.get("id")?,
            title: row.get("title")?,
            description: row.get("description")?,
            category_id: row.get("category_id")?,
            priority: Priority::parse(&row.get::<String, _>("priority")?).unwrap_or(Priority::Medium),
            status: match row.get::<String, _>("status")?.as_str() {
                "completed" => TaskStatus::Completed,
                _ => TaskStatus::Pending,
            },
            due_at: row.get("due_at")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
            completed_at: row.get("completed_at")?,
            reminders: Vec::new(),
        })
    }

    fn load_reminders(conn: &Connection, task_id: i64) -> Vec<i64> {
        let mut stmt = match conn.prepare(
            "SELECT offset_seconds FROM reminders WHERE task_id = ?1 ORDER BY offset_seconds",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map([task_id], |r| r.get::<_, i64>(0));
        match rows {
            Ok(iter) => iter.filter_map(|r| r.ok()).collect(),
            Err(_) => Vec::new(),
        }
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
        let tasks: Vec<Task> = stmt
            .query_map(params, Self::row_to_task)?
            .filter_map(|r| r.ok())
            .collect();
        // 附加提醒配置
        Ok(tasks
            .into_iter()
            .map(|mut t| {
                t.reminders = Self::load_reminders(conn, t.id);
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

    pub fn create_task(&self, input: &TaskInput) -> DbResult<Task> {
        let title = input.title.trim();
        if title.is_empty() {
            return Err(DbError::Validation("任务名称不能为空".into()));
        }
        validate_due(&input.due_at)?;
        let reminders = normalize_reminder_offsets(&input.reminders);
        let now = now_iso();

        let conn = self.conn.lock();
        conn.execute(
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
        let id = conn.last_insert_rowid();
        for &offset in &reminders {
            conn.execute(
                "INSERT INTO reminders (task_id, offset_seconds) VALUES (?1, ?2)",
                rusqlite::params![id, offset],
            )?;
        }
        drop(conn);
        self.get_task(id)
    }

    pub fn update_task(&self, id: i64, input: &TaskInput) -> DbResult<Task> {
        let title = input.title.trim();
        if title.is_empty() {
            return Err(DbError::Validation("任务名称不能为空".into()));
        }
        validate_due(&input.due_at)?;
        let reminders = normalize_reminder_offsets(&input.reminders);
        let now = now_iso();

        let conn = self.conn.lock();
        // 截止时间变更后需要重新计算提醒：清除旧的触发记录，让未来的提醒可以再次触发
        let old_due: Option<String> = conn
            .query_row("SELECT due_at FROM tasks WHERE id = ?1", [id], |r| r.get(0))
            .map_err(|_| DbError::NotFound)?;
        if old_due != input.due_at {
            conn.execute("DELETE FROM reminder_events WHERE task_id = ?1", [id])?;
        }
        conn.execute(
            "UPDATE tasks SET title = ?1, description = ?2, category_id = ?3, priority = ?4, due_at = ?5, updated_at = ?6
             WHERE id = ?7",
            rusqlite::params![title, input.description.trim(), input.category_id, input.priority.as_str(), input.due_at, now, id],
        )?;
        // 重置提醒配置
        conn.execute("DELETE FROM reminders WHERE task_id = ?1", [id])?;
        for &offset in &reminders {
            conn.execute(
                "INSERT INTO reminders (task_id, offset_seconds) VALUES (?1, ?2)",
                rusqlite::params![id, offset],
            )?;
        }
        drop(conn);
        self.get_task(id)
    }

    /// 设置任务完成状态。
    /// 已触发的提醒记录保留（事件只在其调度时刻已过后才会产生，
    /// 因此恢复为待办后：已过期的提醒不会重复弹出，未来的提醒照常触发）。
    pub fn set_task_status(&self, id: i64, completed: bool) -> DbResult<Task> {
        let now = now_iso();
        let conn = self.conn.lock();
        let affected = conn.execute(
            "UPDATE tasks SET status = ?1, completed_at = ?2, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![
                if completed { "completed" } else { "pending" },
                if completed { Some(&now) } else { None::<String> },
                id
            ],
        )?;
        if affected == 0 {
            return Err(DbError::NotFound);
        }
        drop(conn);
        self.get_task(id)
    }

    /// 删除任务（提醒配置级联删除）。
    pub fn delete_task(&self, id: i64) -> DbResult<()> {
        let conn = self.conn.lock();
        let affected = conn.execute("DELETE FROM tasks WHERE id = ?1", [id])?;
        if affected == 0 {
            return Err(DbError::NotFound);
        }
        Ok(())
    }

    // ===== 提醒事件（幂等记录）=====

    /// 记录提醒触发；返回 false 表示该提醒已触发过（幂等）。
    pub fn record_reminder_fired(&self, task_id: i64, offset_seconds: i64) -> DbResult<bool> {
        let conn = self.conn.lock();
        let affected = conn.execute(
            "INSERT OR IGNORE INTO reminder_events (task_id, offset_seconds, fired_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![task_id, offset_seconds, now_iso()],
        )?;
        Ok(affected > 0)
    }

    /// 判断某个 (task, offset) 是否已经触发过。
    pub fn reminder_already_fired(&self, task_id: i64, offset_seconds: i64) -> DbResult<bool> {
        let conn = self.conn.lock();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM reminder_events WHERE task_id = ?1 AND offset_seconds = ?2",
            rusqlite::params![task_id, offset_seconds],
            |r| r.get(0),
        )?;
        Ok(count > 0)
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
            .filter_map(|r| r.ok())
            .collect();
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
        for row in rows.filter_map(|r| r.ok()) {
            let (key, value) = row;
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
                "widget_display_count" => {
                    if let Ok(v) = value.parse() {
                        settings.widget_display_count = v
                    }
                }
                "launch_at_login" => settings.launch_at_login = value == "true",
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

    pub fn save_settings(&self, settings: &AppSettings) -> DbResult<()> {
        let s = settings.clone().sanitized();
        let conn = self.conn.lock();
        let entries: [(&str, String); 9] = [
            ("theme", s.theme.clone()),
            ("glass_enabled", s.glass_enabled.to_string()),
            ("glass_opacity", s.glass_opacity.to_string()),
            ("always_on_top", s.always_on_top.to_string()),
            ("close_to_tray", s.close_to_tray.to_string()),
            ("notifications_enabled", s.notifications_enabled.to_string()),
            ("widget_display_count", s.widget_display_count.to_string()),
            ("launch_at_login", s.launch_at_login.to_string()),
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

    // ===== 窗口几何 =====

    /// 读取窗口几何（key 如 widget_x / widget_y / widget_w / widget_h）
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
        let x = get(&format!("{prefix}_x"))?;
        let y = get(&format!("{prefix}_y"))?;
        let w = get(&format!("{prefix}_w"))?;
        let h = get(&format!("{prefix}_h"))?;
        Some((x, y, w, h))
    }

    pub fn save_window_geometry(&self, prefix: &str, x: f64, y: f64, w: f64, h: f64) -> DbResult<()> {
        let conn = self.conn.lock();
        let entries = [
            (format!("{prefix}_x"), x),
            (format!("{prefix}_y"), y),
            (format!("{prefix}_w"), w),
            (format!("{prefix}_h"), h),
        ];
        for (key, value) in entries {
            conn.execute(
                "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                rusqlite::params![key, value.to_string()],
            )?;
        }
        Ok(())
    }
}

/// 当前 UTC 时间的 RFC3339 字符串
pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
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
