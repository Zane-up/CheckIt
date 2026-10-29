//! 数据库层集成测试：持久化、恢复、分类关系、设置、几何。

use crate::database::Database;
use crate::models::task::{Priority, TaskInput};
use crate::models::SettingsPatch;

fn db() -> Database {
    let db = Database::open_in_memory().unwrap();
    db.ensure_default_categories().unwrap();
    db
}

#[test]
fn window_mode_geometry_and_anchor_roll_back_together_on_write_failure() {
    let db = db();
    let before = [
        ("widget_mode".into(), "expanded".into()),
        ("widget_px_h".into(), "440".into()),
        ("widget_dock_x".into(), "1880".into()),
        ("widget_dock_y".into(), "200".into()),
    ];
    db.set_setting_entries(&before).unwrap();
    db.conn
        .lock()
        .execute_batch(
            "CREATE TRIGGER fail_window_commit BEFORE INSERT ON app_settings
         WHEN NEW.key = 'widget_dock_y' BEGIN SELECT RAISE(ABORT, 'injected disk failure'); END;",
        )
        .unwrap();
    assert!(db
        .set_setting_entries(&[
            ("widget_mode".into(), "docked".into()),
            ("widget_px_h".into(), "520".into()),
            ("widget_dock_x".into(), "0".into()),
            ("widget_dock_y".into(), "300".into()),
        ])
        .is_err());
    for (key, value) in before {
        assert_eq!(db.get_setting_value(&key).unwrap(), value);
    }
}

fn input(title: &str, due: Option<&str>, reminders: Vec<i64>) -> TaskInput {
    TaskInput {
        title: title.into(),
        description: "测试描述".into(),
        category_id: None,
        priority: Priority::High,
        due_at: due.map(|s| s.into()),
        reminders,
    }
}

#[test]
fn create_and_read_back_task() {
    let db = db();
    let task = db
        .create_task(&input("数据持久化", Some("2026-11-01T10:00:00Z"), vec![3600, 600]))
        .unwrap();
    assert_eq!(task.title, "数据持久化");
    assert_eq!(task.priority, Priority::High);
    assert_eq!(task.reminders, vec![600, 3600]);
    assert!(task.due_at.as_deref() == Some("2026-11-01T10:00:00Z"));
}

#[test]
fn empty_title_rejected() {
    let db = db();
    let err = db.create_task(&input("   ", None, vec![]));
    assert!(err.is_err());
}

#[test]
fn invalid_due_format_rejected() {
    let db = db();
    let err = db.create_task(&input("非法时间", Some("明天下午"), vec![]));
    assert!(err.is_err());
}

#[test]
fn sql_injection_attempt_is_stored_literally() {
    let db = db();
    let task = db
        .create_task(&input("'); DROP TABLE tasks; --", None, vec![]))
        .unwrap();
    // 参数绑定下注入内容按字面存储，表未受影响
    assert_eq!(task.title, "'); DROP TABLE tasks; --");
    assert!(db.list_tasks().unwrap().len() >= 1);
}

#[test]
fn update_task_reflects_changes() {
    let db = db();
    let task = db.create_task(&input("原任务", None, vec![])).unwrap();
    let updated = db
        .update_task(task.id, &input("改后任务", Some("2026-12-01T08:00:00Z"), vec![1800]))
        .unwrap();
    assert_eq!(updated.title, "改后任务");
    assert_eq!(updated.reminders, vec![1800]);
}

/// 自定义提醒提前量（非预设秒值）：文件数据库保存后重开，原样保留。
#[test]
fn custom_reminder_offsets_roundtrip_file_db() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("custom_reminders.db");
    {
        let db = Database::open(&path).unwrap();
        db.ensure_default_categories().unwrap();
        let task = db
            .create_task(&input("自定义提醒", Some("2026-11-01T10:00:00Z"), vec![1500, 3600, 0]))
            .unwrap();
        assert_eq!(task.reminders, vec![0, 1500, 3600]);
    }
    let db2 = Database::open(&path).unwrap();
    let tasks = db2.list_tasks().unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].reminders, vec![0, 1500, 3600]);
}

/// 自定义默认提醒：补丁含重复与越界值时按 normalize 保存，重开后保留。
#[test]
fn custom_default_reminder_offsets_persist_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings_custom.db");
    {
        let db = Database::open(&path).unwrap();
        db.ensure_default_categories().unwrap();
        let patch = SettingsPatch {
            default_reminder_offsets: Some(vec![1500, 3600, 1500, 61 * 86400]),
            ..Default::default()
        };
        let s = db.apply_settings_patch(&patch).unwrap();
        assert_eq!(s.default_reminder_offsets, vec![1500, 3600]);
    }
    let db2 = Database::open(&path).unwrap();
    assert_eq!(
        db2.load_settings().unwrap().default_reminder_offsets,
        vec![1500, 3600]
    );
}

#[test]
fn delete_task_removes_reminders() {
    let db = db();
    let task = db
        .create_task(&input("待删除", Some("2026-12-01T08:00:00Z"), vec![3600]))
        .unwrap();
    db.delete_task(task.id).unwrap();
    assert!(db.get_task(task.id).is_err());
}

#[test]
fn complete_and_restore() {
    let db = db();
    let task = db.create_task(&input("完成测试", None, vec![])).unwrap();

    let completed = db.set_task_status(task.id, true).unwrap();
    assert_eq!(completed.status, crate::models::TaskStatus::Completed);
    assert!(completed.completed_at.is_some());

    let restored = db.set_task_status(task.id, false).unwrap();
    assert_eq!(restored.status, crate::models::TaskStatus::Pending);
    assert!(restored.completed_at.is_none());
}

#[test]
fn categories_default_seeded() {
    let db = db();
    let cats = db.list_categories().unwrap();
    let names: Vec<&str> = cats.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["学习", "工作", "会议", "生活", "其他"]);
}

#[test]
fn delete_category_keeps_tasks() {
    let db = db();
    let cat = db.create_category("临时分类", "#ff0000").unwrap();
    let task = db
        .create_task(&input("归属任务", None, vec![]))
        .unwrap();
    let task = db
        .update_task(
            task.id,
            &TaskInput {
                title: "归属任务".into(),
                description: "".into(),
                category_id: Some(cat.id),
                priority: Priority::Medium,
                due_at: None,
                reminders: vec![],
            },
        )
        .unwrap();
    assert_eq!(task.category_id, Some(cat.id));

    // 删除分类后任务保留且变为未分类
    db.delete_category(cat.id).unwrap();
    let task = db.get_task(task.id).unwrap();
    assert_eq!(task.category_id, None);
}

#[test]
fn settings_roundtrip() {
    let db = db();
    let mut settings = db.load_settings().unwrap();
    assert_eq!(settings.theme, "dark"); // 默认值
    settings.theme = "light".into();
    settings.glass_opacity = 0.5;
    settings.widget_display_count = 12;
    settings.default_reminder_offsets = vec![86400, 3600];
    db.save_settings(&settings).unwrap();

    let reloaded = db.load_settings().unwrap();
    assert_eq!(reloaded.theme, "light");
    assert_eq!(reloaded.glass_opacity, 0.5);
    assert_eq!(reloaded.widget_display_count, 12);
    assert_eq!(reloaded.default_reminder_offsets, vec![3600, 86400]);
}

#[test]
fn window_geometry_roundtrip() {
    let db = db();
    assert!(db.load_window_geometry("widget").is_none());
    db.save_window_geometry("widget", 120.0, 88.0, 340.0, 440.0)
        .unwrap();
    let geo = db.load_window_geometry("widget").unwrap();
    assert_eq!(geo, (120.0, 88.0, 340.0, 440.0));
}

// ===== 窗口模式 / 圆圈锚点 / 临时高度持久化 =====

/// 圆圈模式与锚点独立于正常展开几何：写模式不改变 widget_px_* 四键。
#[test]
fn dock_mode_does_not_touch_expanded_geometry() {
    let db = db();
    db.save_window_geometry("widget", 10.0, 20.0, 340.0, 200.0)
        .unwrap();

    // 收纳为圆圈：写模式 + 锚点
    db.set_setting_entries(&[
        ("widget_mode".to_string(), "docked".to_string()),
        ("widget_dock_edge".to_string(), "right".to_string()),
        ("widget_dock_x".to_string(), "1880".to_string()),
        ("widget_dock_y".to_string(), "480".to_string()),
    ])
    .unwrap();

    assert_eq!(
        db.load_window_geometry("widget"),
        Some((10.0, 20.0, 340.0, 200.0))
    );
    assert_eq!(db.get_setting_value("widget_mode").as_deref(), Some("docked"));
    assert_eq!(
        db.get_setting_value("widget_dock_edge").as_deref(),
        Some("right")
    );
    assert_eq!(db.get_setting_value("widget_dock_x").as_deref(), Some("1880"));
}

/// 旧库没有任何模式键：读取应回退 Expanded（兼容旧数据）。
#[test]
fn missing_mode_key_falls_back_to_expanded() {
    let db = db();
    assert!(db.get_setting_value("widget_mode").is_none());
    let mode = db
        .get_setting_value("widget_mode")
        .map(|s| crate::dock::WidgetMode::parse(&s))
        .unwrap_or(crate::dock::WidgetMode::Expanded);
    assert_eq!(mode, crate::dock::WidgetMode::Expanded);
}

/// 设置项 edge_dock_enabled：默认开启、可关闭、补丁只改该字段。
#[test]
fn edge_dock_setting_defaults_on_and_patch_persists() {
    let db = db();
    assert!(db.load_settings().unwrap().edge_dock_enabled);

    // 注意：save_settings_inner 会把"当前结构体全部字段"写回，
    // 因此 patch 必须应用于**刚从库里读出的完整设置**（与 update_settings 命令一致）。
    let mut patch = SettingsPatch::default();
    patch.edge_dock_enabled = Some(false);
    let mut current = db.load_settings().unwrap();
    patch.apply_to(&mut current);
    db.save_settings(&current).unwrap();
    assert!(!db.load_settings().unwrap().edge_dock_enabled);

    // 后续只改主题的补丁必须保留"关闭"状态
    let mut patch = SettingsPatch::default();
    patch.theme = Some("light".into());
    let mut current = db.load_settings().unwrap();
    patch.apply_to(&mut current);
    db.save_settings(&current).unwrap();
    let reloaded = db.load_settings().unwrap();
    assert!(!reloaded.edge_dock_enabled, "关闭状态必须被保留");
    assert_eq!(reloaded.theme, "light");
}

/// 正常高度 200 → 快速添加临时 320（不写库）→ 保持打开 → 退出重启仍为 200。
#[test]
fn temp_quick_add_height_never_overwrites_expanded_height() {
    use crate::dock::should_persist_expanded;
    use crate::dock::WidgetMode;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("checkit.db");

    {
        let db = Database::open(&path).unwrap();
        db.ensure_default_categories().unwrap();
        // 用户把窗口缩到 200 高
        db.save_window_geometry("widget", 100.0, 100.0, 300.0, 200.0)
            .unwrap();

        // 打开快速添加：窗口临时抬到 320。此时 500ms 防抖保存会触发，
        // 但 mode=Expanded 且 temp_height=Some(320) → 必须跳过保存
        let mode = WidgetMode::Expanded;
        let temp = Some(320.0);
        assert!(
            !should_persist_expanded(mode, temp),
            "临时高度期间不得写回正常几何"
        );

        // 面板保持打开远超 500ms：故意模拟错误的写入尝试被规则拦下
        // （这里不直接调用 save_window_geometry，验证的是判定规则）
        assert_eq!(
            db.load_window_geometry("widget"),
            Some((100.0, 100.0, 300.0, 200.0))
        );
    }

    // 重启：全新连接读取
    let db2 = Database::open(&path).unwrap();
    let (_, _, w, h) = db2.load_window_geometry("widget").unwrap();
    assert_eq!((w, h), (300.0, 200.0), "重启后正常展开高度仍为 200");
}

/// 圆圈状态退出重启：正常展开尺寸仍正确，模式为 docked。
#[test]
fn docked_restart_keeps_expanded_geometry() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("checkit.db");

    {
        let db = Database::open(&path).unwrap();
        db.ensure_default_categories().unwrap();
        db.save_window_geometry("widget", 50.0, 60.0, 340.0, 440.0)
            .unwrap();
        // 收纳：写模式与锚点，不改几何
        db.set_setting_entries(&[
            ("widget_mode".to_string(), "docked".to_string()),
            ("widget_dock_edge".to_string(), "left".to_string()),
            ("widget_dock_x".to_string(), "0".to_string()),
            ("widget_dock_y".to_string(), "300".to_string()),
        ])
        .unwrap();
        // 圆圈尺寸（40×40）绝不能被写成正常几何
        assert_eq!(
            db.load_window_geometry("widget"),
            Some((50.0, 60.0, 340.0, 440.0))
        );
    }

    let db2 = Database::open(&path).unwrap();
    assert_eq!(
        db2.load_window_geometry("widget"),
        Some((50.0, 60.0, 340.0, 440.0))
    );
    assert_eq!(db2.get_setting_value("widget_mode").as_deref(), Some("docked"));
    assert_eq!(
        db2.get_setting_value("widget_dock_edge").as_deref(),
        Some("left")
    );
}

/// 重启恢复：文件数据库**连接真正关闭（drop）后**重新打开，数据一致。
#[test]
fn restart_recovers_data_from_file_db() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("checkit.db");

    // 第一次"运行"：作用域结束时 db 被 drop，SQLite 连接真正关闭
    {
        let db = Database::open(&path).unwrap();
        db.ensure_default_categories().unwrap();
        db.create_task(&input("跨重启任务", Some("2026-11-15T09:00:00Z"), vec![600]))
            .unwrap();
        let mut settings = db.load_settings().unwrap();
        settings.theme = "light".into();
        db.save_settings(&settings).unwrap();
    }

    // 第二次"运行"：全新连接
    let db2 = Database::open(&path).unwrap();
    let state = db2.get_app_state().unwrap();
    assert_eq!(state.tasks.len(), 1);
    assert_eq!(state.tasks[0].title, "跨重启任务");
    assert_eq!(state.tasks[0].reminders, vec![600]);
    assert_eq!(state.categories.len(), 5);
    assert_eq!(state.settings.theme, "light");
}

/// 迁移幂等：重复打开不重复建表、不丢数据。
#[test]
fn migration_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("checkit.db");
    {
        let db = Database::open(&path).unwrap();
        db.create_task(&input("迁移幂等", None, vec![])).unwrap();
    }
    {
        let db = Database::open(&path).unwrap();
        assert_eq!(db.list_tasks().unwrap().len(), 1);
    }
}

/// 事务原子性：任务创建中途失败（触发器制造提醒插入错误）→ 整体回滚，不留半成品任务。
#[test]
fn create_task_rolls_back_on_midway_failure() {
    let db = db();
    // 用触发器模拟第二条提醒写入失败（如磁盘错误/约束冲突）
    {
        let conn = db.lock_conn_for_test();
        conn.execute_batch(
            "CREATE TRIGGER test_fail_offset_600 BEFORE INSERT ON reminders
             WHEN NEW.offset_seconds = 600
             BEGIN SELECT RAISE(ABORT, 'simulated failure'); END;",
        )
        .unwrap();
    }

    let result = db.create_task(&input("半成品任务", Some("2030-01-01T00:00:00Z"), vec![3600, 600]));
    assert!(result.is_err(), "中途失败必须返回错误");

    // 回滚验证：任务、提醒均不存在
    let tasks = db.list_tasks().unwrap();
    assert!(tasks.iter().all(|t| t.title != "半成品任务"), "失败的任务不能落库");
    {
        let conn = db.lock_conn_for_test();
        let reminders: i64 = conn
            .query_row("SELECT COUNT(*) FROM reminders", [], |r| r.get(0))
            .unwrap();
        drop(conn);
        assert_eq!(reminders, 0, "失败的提醒配置不能落库");
    }
}

/// 事务原子性：任务更新中途失败 → 旧标题 / 旧截止 / 旧提醒配置 / 去重记录完整保留。
#[test]
fn update_task_rolls_back_preserving_old_state() {
    let db = db();
    let due_old = "2030-01-01T00:00:00Z";
    let task = db
        .create_task(&input("原标题", Some(due_old), vec![1800]))
        .unwrap();
    // 领取一条事件记录（模拟已触发历史）
    db.claim_reminder(task.id, 1800, due_old, false, chrono::Utc::now())
        .unwrap();

    {
        let conn = db.lock_conn_for_test();
        conn.execute_batch(
            "CREATE TRIGGER test_fail_update BEFORE INSERT ON reminders
             WHEN NEW.offset_seconds = 600
             BEGIN SELECT RAISE(ABORT, 'simulated failure'); END;",
        )
        .unwrap();
    }

    let result = db.update_task(
        task.id,
        &input("新标题", Some("2031-06-01T00:00:00Z"), vec![600]),
    );
    assert!(result.is_err());

    let after = db.get_task(task.id).unwrap();
    assert_eq!(after.title, "原标题");
    assert_eq!(after.due_at.as_deref(), Some(due_old));
    assert_eq!(after.reminders, vec![1800]);
    assert!(db.test_event_state(task.id, 1800, due_old).is_some(), "去重记录必须保留");
}

/// v1 → 当前版本迁移：手工构造 v1 库（含旧版 reminder_events），迁移后数据与去重语义保留。
#[test]
fn migrates_v1_database_to_current_preserving_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v1.db");

    // —— 手工构造 v1 结构与数据 ——
    {
        let raw = rusqlite::Connection::open(&path).unwrap();
        raw.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
             CREATE TABLE categories (
                id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE,
                color TEXT NOT NULL DEFAULT '#6366f1', sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
             CREATE TABLE tasks (
                id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                category_id INTEGER REFERENCES categories(id) ON DELETE SET NULL,
                priority TEXT NOT NULL DEFAULT 'medium',
                status TEXT NOT NULL DEFAULT 'pending',
                due_at TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, completed_at TEXT);
             CREATE TABLE reminders (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                offset_seconds INTEGER NOT NULL, UNIQUE (task_id, offset_seconds));
             CREATE TABLE reminder_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT, task_id INTEGER NOT NULL,
                offset_seconds INTEGER NOT NULL, fired_at TEXT NOT NULL,
                UNIQUE (task_id, offset_seconds));
             CREATE TABLE app_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO schema_migrations VALUES (1, '2026-01-01T00:00:00Z');
             INSERT INTO categories (name, color, sort_order, created_at, updated_at)
                VALUES ('学习', '#6366f1', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
             INSERT INTO tasks (title, status, due_at, created_at, updated_at)
                VALUES ('旧任务', 'pending', '2030-01-01T00:00:00Z', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
             INSERT INTO reminders (task_id, offset_seconds) VALUES (1, 3600);
             INSERT INTO reminder_events (task_id, offset_seconds, fired_at)
                VALUES (1, 3600, '2026-01-02T00:00:00Z');
             INSERT INTO app_settings VALUES ('theme', 'light');",
        )
        .unwrap();
    }

    // —— 打开触发 v2、v3 迁移 ——
    let db = Database::open(&path).unwrap();
    let state = db.get_app_state().unwrap();
    assert_eq!(state.tasks.len(), 1);
    assert_eq!(state.tasks[0].title, "旧任务");
    assert_eq!(state.tasks[0].reminders, vec![3600]);
    assert_eq!(state.settings.theme, "light");

    // 旧事件行迁移为 submitted，快照回填当前 due → 引擎不会对同一 (task, offset) 重复触发
    assert_eq!(
        db.test_event_state(1, 3600, "2030-01-01T00:00:00Z"),
        Some(("submitted".into(), 0))
    );
    // 引擎语义验证：迁移后的 submitted 行阻止重复触发
    {
        let engine = crate::reminders::ReminderEngine::new(db.clone());
        assert!(
            engine.run_once(chrono::Utc::now()).unwrap().is_empty(),
            "迁移后的历史 submitted 事件必须保持去重效果"
        );
    }
    // 新表结构就位
    let conn = db.lock_conn_for_test();
    let cols: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('reminder_events')
             WHERE name IN ('due_at_snapshot','state','attempts','updated_at')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let version: i64 = conn
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r.get(0))
        .unwrap();
    drop(conn);
    assert_eq!(cols, 4, "v2 新列必须全部存在");
    assert_eq!(version, 5, "必须记录最新迁移版本");

    // 迁移幂等：再次打开不重复迁移
    drop(db);
    let db2 = Database::open(&path).unwrap();
    assert_eq!(db2.list_tasks().unwrap().len(), 1);
}

/// v2 → v3 迁移：sent → submitted，其余状态 / id / attempts / 时间戳逐行保留。
#[test]
fn migrates_v2_events_to_v3_submitted_without_data_loss() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v2.db");

    {
        let raw = rusqlite::Connection::open(&path).unwrap();
        raw.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
             CREATE TABLE categories (
                id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE,
                color TEXT NOT NULL DEFAULT '#6366f1', sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
             CREATE TABLE tasks (
                id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                category_id INTEGER REFERENCES categories(id) ON DELETE SET NULL,
                priority TEXT NOT NULL DEFAULT 'medium',
                status TEXT NOT NULL DEFAULT 'pending',
                due_at TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, completed_at TEXT);
             CREATE TABLE reminders (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                offset_seconds INTEGER NOT NULL, UNIQUE (task_id, offset_seconds));
             CREATE TABLE reminder_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id INTEGER NOT NULL,
                offset_seconds INTEGER NOT NULL,
                due_at_snapshot TEXT NOT NULL,
                state TEXT NOT NULL DEFAULT 'pending'
                    CHECK (state IN ('pending','sent','failed','skipped','cancelled')),
                attempts INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE (task_id, offset_seconds, due_at_snapshot));
             CREATE TABLE app_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO schema_migrations VALUES (1, '2026-01-01T00:00:00Z');
             INSERT INTO schema_migrations VALUES (2, '2026-02-01T00:00:00Z');
             INSERT INTO tasks (title, due_at, created_at, updated_at)
                VALUES ('v2 任务', '2030-01-01T00:00:00Z', '2026-02-01T00:00:00Z', '2026-02-01T00:00:00Z');
             INSERT INTO reminder_events
                (id, task_id, offset_seconds, due_at_snapshot, state, attempts, created_at, updated_at)
                VALUES
                (10, 1, 3600, '2030-01-01T00:00:00Z', 'sent',    0, '2026-02-02T00:00:00Z', '2026-02-02T00:00:00Z'),
                (11, 1, 600,  '2030-01-01T00:00:00Z', 'failed',  2, '2026-02-03T00:00:00Z', '2026-02-03T00:10:00Z'),
                (12, 1, 0,    '2030-01-01T00:00:00Z', 'pending', 0, '2026-02-04T00:00:00Z', '2026-02-04T00:00:00Z'),
                (13, 1, 1800, '2030-01-01T00:00:00Z', 'skipped', 1, '2026-02-05T00:00:00Z', '2026-02-05T00:00:00Z'),
                (14, 1, 7200, '2030-01-01T00:00:00Z', 'cancelled', 0, '2026-02-06T00:00:00Z', '2026-02-06T00:00:00Z');",
        )
        .unwrap();
    }

    let db = Database::open(&path).unwrap();
    // 通读全部行，确认状态映射与字段均未丢失
    {
        let conn = db.lock_conn_for_test();
        let mut stmt = conn
            .prepare(
                "SELECT id, offset_seconds, state, attempts, created_at, updated_at
                 FROM reminder_events ORDER BY id",
            )
            .unwrap();
        let rows: Vec<(i64, i64, String, i64, String, String)> = stmt
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![
                (10, 3600, "submitted".into(), 0, "2026-02-02T00:00:00Z".into(), "2026-02-02T00:00:00Z".into()),
                (11, 600, "failed".into(), 2, "2026-02-03T00:00:00Z".into(), "2026-02-03T00:10:00Z".into()),
                (12, 0, "pending".into(), 0, "2026-02-04T00:00:00Z".into(), "2026-02-04T00:00:00Z".into()),
                (13, 1800, "skipped".into(), 1, "2026-02-05T00:00:00Z".into(), "2026-02-05T00:00:00Z".into()),
                (14, 7200, "cancelled".into(), 0, "2026-02-06T00:00:00Z".into(), "2026-02-06T00:00:00Z".into()),
            ]
        );

        // 新 CHECK 约束生效：sent 已不被接受
        let insert_sent = conn.execute(
            "INSERT INTO reminder_events
             (task_id, offset_seconds, due_at_snapshot, state, created_at, updated_at)
             VALUES (1, 99, '2030-01-01T00:00:00Z', 'sent', 'x', 'x')",
            [],
        );
        assert!(insert_sent.is_err(), "v3 必须拒绝旧状态 sent");

        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 5);
    }

    // 迁移幂等
    drop(db);
    let db2 = Database::open(&path).unwrap();
    assert_eq!(db2.list_tasks().unwrap().len(), 1);
}

/// 设置补丁只覆盖出现的字段，其余字段（含 Vec 配置）原样保留；非法值被夹紧。
#[test]
fn settings_patch_preserves_untouched_fields() {
    use crate::models::SettingsPatch;

    let db = db();
    let mut base = db.load_settings().unwrap();
    base.theme = "light".into();
    base.glass_opacity = 0.42;
    base.widget_display_count = 11;
    base.default_reminder_offsets = vec![86400, 600];
    db.save_settings(&base).unwrap();

    let updated = db
        .apply_settings_patch(&SettingsPatch {
            glass_opacity: Some(0.55),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(updated.glass_opacity, 0.55);
    assert_eq!(updated.theme, "light", "补丁未提供的字段不能回退默认值");
    assert_eq!(updated.widget_display_count, 11);
    assert_eq!(updated.default_reminder_offsets, vec![600, 86400]);
    assert_eq!(updated.always_on_top, base.always_on_top);

    // 非法值夹紧 / 回退
    let patched = db
        .apply_settings_patch(&SettingsPatch {
            theme: Some("blue".into()),
            glass_opacity: Some(9.0),
            widget_display_count: Some(1),
            default_reminder_offsets: Some(vec![-1, 7200, 7200]),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(patched.theme, "dark");
    assert_eq!(patched.glass_opacity, 0.6);
    assert_eq!(patched.widget_display_count, 3);
    assert_eq!(patched.default_reminder_offsets, vec![7200]);
}

/// 并发补丁 + 置顶翻转：两个线程交替操作，字段互不覆盖、无死锁。
#[test]
fn concurrent_settings_patch_and_pin_keep_all_fields() {
    use crate::models::SettingsPatch;
    use std::sync::Arc;

    let db = Arc::new(db());
    let mut base = db.load_settings().unwrap();
    base.theme = "light".into();
    base.glass_opacity = 0.3;
    base.widget_display_count = 9;
    db.save_settings(&base).unwrap();
    let base_pinned = base.always_on_top;

    let patch_db = db.clone();
    let patcher = std::thread::spawn(move || {
        for i in 0..100 {
            let opacity = 0.3 + (i % 5) as f64 * 0.01;
            patch_db
                .apply_settings_patch(&SettingsPatch {
                    glass_opacity: Some(opacity),
                    ..Default::default()
                })
                .unwrap();
        }
    });
    let pin_db = db.clone();
    let pinner = std::thread::spawn(move || {
        for _ in 0..100 {
            pin_db.toggle_always_on_top().unwrap();
        }
    });
    patcher.join().unwrap();
    pinner.join().unwrap();

    let final_settings = db.load_settings().unwrap();
    assert_eq!(final_settings.theme, "light");
    assert_eq!(final_settings.widget_display_count, 9);
    assert!((0.3..=0.34).contains(&final_settings.glass_opacity));
    assert_eq!(
        final_settings.always_on_top, base_pinned,
        "偶数次翻转应回到初始值，且不被补丁覆盖"
    );
}
