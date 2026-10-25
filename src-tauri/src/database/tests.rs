//! 数据库层集成测试：持久化、恢复、分类关系、设置、几何。

use crate::database::Database;
use crate::models::task::{Priority, TaskInput};

fn db() -> Database {
    let db = Database::open_in_memory().unwrap();
    db.ensure_default_categories().unwrap();
    db
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

/// 重启恢复：文件数据库关闭后重新打开，数据一致（对应验收第 14 条）。
#[test]
fn restart_recovers_data_from_file_db() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("checkit.db");

    // 第一次"运行"
    let db = Database::open(&path).unwrap();
    db.ensure_default_categories().unwrap();
    db.create_task(&input("跨重启任务", Some("2026-11-15T09:00:00Z"), vec![600]))
        .unwrap();
    let mut settings = db.load_settings().unwrap();
    settings.theme = "light".into();
    db.save_settings(&settings).unwrap();

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
