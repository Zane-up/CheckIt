use super::*;

#[derive(Serialize)]
pub struct DeletedTask {
    pub task: Task,
    pub deleted_at: String,
}

impl Database {
    pub fn trash_task(&self, id: i64, now: DateTime<Utc>) -> DbResult<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let changed=tx.execute("INSERT OR IGNORE INTO task_trash(task_id,deleted_at) SELECT id,?2 FROM tasks WHERE id=?1",rusqlite::params![id,format_iso(now)])?;
        if changed == 0 {
            return Err(DbError::NotFound);
        }
        tx.execute("UPDATE reminder_events SET state='cancelled',updated_at=?2 WHERE task_id=?1 AND state IN ('pending','failed')",rusqlite::params![id,format_iso(now)])?;
        tx.execute("UPDATE popup_reminders SET dismissed=1 WHERE event_id IN (SELECT id FROM reminder_events WHERE task_id=?1)",[id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn list_deleted_tasks(&self, now: DateTime<Utc>) -> DbResult<Vec<DeletedTask>> {
        let conn = self.conn.lock();
        let cutoff = format_iso(now - chrono::Duration::days(7));
        let mut stmt = conn.prepare(
            "SELECT task_id,deleted_at FROM task_trash WHERE julianday(deleted_at)>julianday(?1)",
        )?;
        let dates = stmt
            .query_map([&cutoff], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<std::collections::HashMap<_, _>>>()?;
        let tasks=self.load_tasks_including_trash(&conn,"WHERE id IN (SELECT task_id FROM task_trash WHERE julianday(deleted_at)>julianday(?1))",&[&cutoff])?;
        let mut result: Vec<_> = tasks
            .into_iter()
            .map(|task| DeletedTask {
                deleted_at: dates[&task.id].clone(),
                task,
            })
            .collect();
        result.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at));
        Ok(result)
    }

    pub fn restore_task(&self, id: i64, now: DateTime<Utc>) -> DbResult<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let deleted: String = tx.query_row(
            "SELECT deleted_at FROM task_trash WHERE task_id=?1",
            [id],
            |r| r.get(0),
        )?;
        if parse_rfc3339_local(&deleted)? <= now - chrono::Duration::days(7) {
            return Err(DbError::Validation("该任务已超过七天恢复期限".into()));
        }
        let task = self
            .load_tasks_including_trash(&tx, "WHERE id=?1", &[&id])?
            .pop()
            .ok_or(DbError::NotFound)?;
        if let Some(due) = &task.due_at {
            let due_time = parse_rfc3339_local(due)?;
            for offset in task.reminders {
                if due_time - chrono::Duration::seconds(offset) <= now {
                    tx.execute("INSERT OR IGNORE INTO reminder_events(task_id,offset_seconds,due_at_snapshot,state,attempts,created_at,updated_at) VALUES(?1,?2,?3,'skipped',0,?4,?4)",rusqlite::params![id,offset,due,format_iso(now)])?;
                } else {
                    tx.execute("DELETE FROM reminder_events WHERE task_id=?1 AND offset_seconds=?2 AND due_at_snapshot=?3 AND state IN ('pending','failed','cancelled')",rusqlite::params![id,offset,due])?;
                }
            }
        }
        tx.execute("DELETE FROM task_trash WHERE task_id=?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn purge_task_trash(&self, now: DateTime<Utc>) -> DbResult<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let cutoff = format_iso(now - chrono::Duration::days(7));
        tx.execute("DELETE FROM reminder_events WHERE task_id IN (SELECT task_id FROM task_trash WHERE julianday(deleted_at)<=julianday(?1))",[&cutoff])?;
        tx.execute("DELETE FROM tasks WHERE id IN (SELECT task_id FROM task_trash WHERE julianday(deleted_at)<=julianday(?1))",[&cutoff])?;
        tx.commit()?;
        Ok(())
    }
}
fn parse_rfc3339_local(value: &str) -> DbResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| DbError::Validation(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reminders::ReminderEngine;
    #[test]
    fn upgrading_v5_preserves_active_task_and_saves_preupgrade_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("live.db");
        let now = Utc::now();
        let db = Database::open(&path).unwrap();
        let original = task(&db, now);
        db.conn
            .lock()
            .execute_batch("DROP TABLE task_trash; DELETE FROM schema_migrations WHERE version=6;")
            .unwrap();
        drop(db);
        let db = Database::open(&path).unwrap();
        let restored = db.get_task(original.id).unwrap();
        assert_eq!(restored.title, original.title);
        assert_eq!(restored.reminders, original.reminders);
        assert!(db.list_deleted_tasks(now).unwrap().is_empty());
        let copies = std::fs::read_dir(dir.path().join("backups"))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(copies.len(), 1);
        assert!(copies[0]
            .file_name()
            .to_string_lossy()
            .ends_with("-before-upgrade.db"));
    }
    fn task(db: &Database, now: DateTime<Utc>) -> Task {
        db.create_task(&TaskInput {
            title: "保留内容".into(),
            description: "原始描述".into(),
            category_id: None,
            priority: Priority::High,
            due_at: Some(format_iso(now + chrono::Duration::minutes(10))),
            reminders: vec![0, 900],
        })
        .unwrap()
    }
    #[test]
    fn trash_survives_restart_and_preserves_contents_and_reminder_history() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("test.db");
        let now = Utc::now();
        let db = Database::open(&p).unwrap();
        let task = task(&db, now);
        db.trash_task(task.id, now).unwrap();
        assert!(db.list_tasks().unwrap().is_empty());
        assert!(db.get_task(task.id).is_err());
        assert!(db.set_task_status(task.id, true).is_err());
        drop(db);
        let db = Database::open(&p).unwrap();
        assert_eq!(db.list_deleted_tasks(now).unwrap().len(), 1);
        db.restore_task(task.id, now + chrono::Duration::seconds(10))
            .unwrap();
        let restored = db.get_task(task.id).unwrap();
        assert_eq!(restored.description, task.description);
        assert_eq!(restored.reminders, task.reminders);
        let engine = ReminderEngine::new(db.clone());
        assert!(engine
            .run_once(now + chrono::Duration::seconds(10))
            .unwrap()
            .is_empty());
        assert_eq!(
            engine
                .run_once(now + chrono::Duration::minutes(10))
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn seven_day_boundary_purges_only_deleted_tasks() {
        let now = Utc::now();
        let db = Database::open_in_memory().unwrap();
        let deleted = task(&db, now);
        let kept = task(&db, now);
        db.trash_task(deleted.id, now).unwrap();
        assert_eq!(
            db.list_deleted_tasks(now + chrono::Duration::days(7) - chrono::Duration::seconds(1))
                .unwrap()
                .len(),
            1
        );
        assert!(db
            .restore_task(deleted.id, now + chrono::Duration::days(7))
            .is_err());
        db.purge_task_trash(now + chrono::Duration::days(7))
            .unwrap();
        assert!(db.list_deleted_tasks(now).unwrap().is_empty());
        assert_eq!(db.get_task(kept.id).unwrap().id, kept.id);
    }
    #[test]
    fn deleting_dismisses_popup_and_restoring_keeps_submitted_event_deduplicated() {
        let db = Database::open_in_memory().unwrap();
        let now = Utc::now();
        let task = task(&db, now);
        let due = now + chrono::Duration::minutes(10);
        let action = ReminderEngine::new(db.clone())
            .run_once(due)
            .unwrap()
            .into_iter()
            .find(|a| a.offset_seconds == 0)
            .unwrap();
        db.enqueue_popup(
            &action,
            &crate::notification::NotificationContent {
                title: "CheckIt".into(),
                body: "提醒".into(),
            },
        )
        .unwrap();
        db.set_reminder_state(
            task.id,
            0,
            task.due_at.as_ref().unwrap(),
            "submitted",
            false,
            due,
        )
        .unwrap();
        db.trash_task(task.id, due).unwrap();
        assert!(db.list_popup_reminders().unwrap().is_empty());
        db.restore_task(task.id, due + chrono::Duration::seconds(10))
            .unwrap();
        assert!(ReminderEngine::new(db.clone())
            .run_once(due + chrono::Duration::seconds(10))
            .unwrap()
            .is_empty());
        assert!(db.list_popup_reminders().unwrap().is_empty());
        db.trash_task(task.id, due).unwrap();
        db.purge_task_trash(due + chrono::Duration::days(7))
            .unwrap();
        let conn = db.conn.lock();
        assert!(!conn
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .exists([])
            .unwrap());
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM reminder_events WHERE task_id=?1",
                [task.id],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
}
