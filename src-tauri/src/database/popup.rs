use super::*;
use crate::notification::NotificationContent;
use crate::reminders::ReminderAction;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PopupReminder {
    pub id: i64,
    pub task_id: i64,
    pub title: String,
    pub body: String,
    pub due_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        task::{Priority, TaskInput},
        SettingsPatch,
    };
    use crate::notification::NotificationTransport;
    use crate::reminders::{DeliveryOutcome, ReminderEngine};

    fn input() -> TaskInput {
        TaskInput {
            title: "弹窗验收".into(),
            description: String::new(),
            category_id: None,
            priority: Priority::Medium,
            due_at: Some("2030-01-01T00:00:00Z".into()),
            reminders: vec![0],
        }
    }

    fn enqueue(db: &Database, title: &str) -> ReminderAction {
        let mut data = input();
        data.title = title.into();
        let task = db.create_task(&data).unwrap();
        let action = ReminderEngine::new(db.clone())
            .run_once(
                DateTime::parse_from_rfc3339(data.due_at.as_ref().unwrap())
                    .unwrap()
                    .with_timezone(&Utc),
            )
            .unwrap()
            .into_iter()
            .find(|a| a.task_id == task.id)
            .unwrap();
        db.enqueue_popup(
            &action,
            &NotificationContent {
                title: "CheckIt".into(),
                body: title.into(),
            },
        )
        .unwrap();
        action
    }

    #[test]
    fn popup_only_mode_is_scheduled_and_settings_survive_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = Database::open(&path).unwrap();
        assert!(!db.load_settings().unwrap().popup_reminders_enabled);
        db.apply_settings_patch(&SettingsPatch {
            notifications_enabled: Some(false),
            popup_reminders_enabled: Some(true),
            ..Default::default()
        })
        .unwrap();
        let action = enqueue(&db, "重启后保留");
        let unread = db.list_popup_reminders().unwrap();
        assert_eq!(unread.len(), 1);
        // 重试相同事件不叠加弹窗。
        db.enqueue_popup(
            &action,
            &NotificationContent {
                title: "CheckIt".into(),
                body: "重复".into(),
            },
        )
        .unwrap();
        assert_eq!(db.list_popup_reminders().unwrap(), unread);
        drop(db);
        let reopened = Database::open(&path).unwrap();
        let settings = reopened.load_settings().unwrap();
        assert!(settings.popup_reminders_enabled && !settings.notifications_enabled);
        assert_eq!(reopened.list_popup_reminders().unwrap(), unread);
        reopened.dismiss_popup_reminders(&[unread[0].id]).unwrap();
        reopened
            .enqueue_popup(
                &action,
                &NotificationContent {
                    title: "CheckIt".into(),
                    body: "不能再弹".into(),
                },
            )
            .unwrap();
        assert!(reopened.list_popup_reminders().unwrap().is_empty());
    }

    #[test]
    fn dismissing_visible_snapshot_preserves_new_arrival() {
        let db = Database::open_in_memory().unwrap();
        enqueue(&db, "第一条");
        let first = db.list_popup_reminders().unwrap()[0].id;
        let second = enqueue(&db, "第二条");
        db.dismiss_popup_reminders(&[first]).unwrap();
        let remaining = db.list_popup_reminders().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].task_id, second.task_id);
    }

    #[test]
    fn completing_one_popup_preserves_other_tasks_and_invalidated_popup_cannot_complete() {
        let db=Database::open_in_memory().unwrap();
        let first=enqueue(&db,"第一项");let second=enqueue(&db,"第二项");
        let id=db.list_popup_reminders().unwrap().iter().find(|r|r.task_id==first.task_id).unwrap().id;
        db.complete_popup_task(id).unwrap();
        assert_eq!(db.get_task(first.task_id).unwrap().status,TaskStatus::Completed);
        assert_eq!(db.list_popup_reminders().unwrap().len(),1);
        assert_eq!(db.list_popup_reminders().unwrap()[0].task_id,second.task_id);
        let stale=db.list_popup_reminders().unwrap()[0].id;
        let mut changed=input();changed.due_at=Some("2030-01-02T00:00:00Z".into());
        db.update_task(second.task_id,&changed).unwrap();
        assert!(db.complete_popup_task(stale).is_err());
        assert_eq!(db.get_task(second.task_id).unwrap().status,TaskStatus::Pending);
    }

    #[test]
    fn snooze_survives_restart_and_resume_and_invalidated_task_stays_hidden() {
        let dir = tempfile::tempdir().unwrap();
        let path=dir.path().join("test.db");
        let db=Database::open(&path).unwrap();
        let action=enqueue(&db,"稍后提醒");
        let now=DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z").unwrap().with_timezone(&Utc);
        let id=db.list_popup_reminders_at(now).unwrap()[0].id;
        db.snooze_popup_reminders(&[id],5,now).unwrap();
        drop(db);
        let db=Database::open(&path).unwrap();
        assert!(db.list_popup_reminders_at(now+chrono::Duration::minutes(4)).unwrap().is_empty());
        assert_eq!(db.list_popup_reminders_at(now+chrono::Duration::minutes(5)).unwrap().len(),1);
        assert_eq!(db.list_popup_reminders_at(now+chrono::Duration::hours(10)).unwrap().len(),1);
        assert!(db.snooze_popup_reminders(&[id],0,now).is_err());
        db.set_task_status(action.task_id,true).unwrap();
        assert!(db.list_popup_reminders_at(now+chrono::Duration::hours(10)).unwrap().is_empty());
    }

    #[test]
    fn invalidated_tasks_disappear_from_popup_queue() {
        for change in ["complete", "delete", "reschedule", "remove_offset"] {
            let db = Database::open_in_memory().unwrap();
            let action = enqueue(&db, change);
            match change {
                "complete" => {
                    db.set_task_status(action.task_id, true).unwrap();
                }
                "delete" => {
                    db.delete_task(action.task_id).unwrap();
                }
                _ => {
                    let mut data = input();
                    if change == "reschedule" {
                        data.due_at = Some("2030-01-02T00:00:00Z".into());
                    } else {
                        data.reminders.clear();
                    }
                    db.update_task(action.task_id, &data).unwrap();
                }
            }
            assert!(db.list_popup_reminders().unwrap().is_empty(), "{change}");
        }
    }

    struct QueueTransport<'a> {
        db: &'a Database,
        action: &'a ReminderAction,
    }
    impl NotificationTransport for QueueTransport<'_> {
        fn submit(&self, content: &NotificationContent) -> Result<(), String> {
            self.db
                .enqueue_popup(self.action, content)
                .map_err(|e| e.to_string())
        }
    }
    #[test]
    fn popup_delivery_retains_scheduler_deduplication_and_both_off_is_silent() {
        let now = DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let db = Database::open_in_memory().unwrap();
        db.apply_settings_patch(&SettingsPatch {
            notifications_enabled: Some(false),
            popup_reminders_enabled: Some(true),
            ..Default::default()
        })
        .unwrap();
        db.create_task(&input()).unwrap();
        let engine = ReminderEngine::new(db.clone());
        let action = engine.run_once(now).unwrap().remove(0);
        let result = engine
            .deliver(
                &action,
                now,
                &QueueTransport {
                    db: &db,
                    action: &action,
                },
            )
            .unwrap();
        assert!(matches!(result, DeliveryOutcome::Submitted { .. }));
        assert_eq!(db.list_popup_reminders().unwrap().len(), 1);
        assert!(engine
            .run_once(now + chrono::Duration::seconds(10))
            .unwrap()
            .is_empty());
        let off = Database::open_in_memory().unwrap();
        off.update_settings_atomic(|s| s.notifications_enabled = false)
            .unwrap();
        off.create_task(&input()).unwrap();
        assert!(ReminderEngine::new(off.clone())
            .run_once(now)
            .unwrap()
            .is_empty());
        assert!(off.list_popup_reminders().unwrap().is_empty());
    }
}

impl Database {
    /// 仅完成仍对应当前有效提醒的任务；校验与修改在同一事务内，防止旧提醒误完成改期任务。
    pub fn complete_popup_task(&self, id:i64) -> DbResult<()> {
        let mut conn=self.conn.lock();let tx=conn.transaction()?;
        let task_id:i64=tx.query_row("SELECT t.id FROM popup_reminders p
          JOIN reminder_events e ON e.id=p.event_id JOIN tasks t ON t.id=e.task_id
          JOIN reminders r ON r.task_id=t.id AND r.offset_seconds=e.offset_seconds
          WHERE p.event_id=?1 AND p.dismissed=0 AND t.status='pending' AND t.due_at=e.due_at_snapshot",[id],|r|r.get(0))?;
        let now=now_iso();
        tx.execute("UPDATE tasks SET status='completed', completed_at=?2, updated_at=?2 WHERE id=?1",rusqlite::params![task_id,now])?;
        tx.execute("UPDATE popup_reminders SET dismissed=1 WHERE event_id IN (SELECT id FROM reminder_events WHERE task_id=?1)",[task_id])?;
        tx.commit()?;Ok(())
    }
    /// 同一事件只入箱一次；确认过的事件重试时不能重新弹出。
    pub fn enqueue_popup(
        &self,
        action: &ReminderAction,
        content: &NotificationContent,
    ) -> DbResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO popup_reminders (event_id, body)
             SELECT id, ?4 FROM reminder_events
             WHERE task_id = ?1 AND offset_seconds = ?2 AND due_at_snapshot = ?3
             ON CONFLICT(event_id) DO NOTHING",
            rusqlite::params![
                action.task_id,
                action.offset_seconds,
                action.due_at_snapshot,
                content.body
            ],
        )?;
        Ok(())
    }

    /// 已完成、删除、改期、移除提前量的提醒不再出现在收件箱。
    pub fn list_popup_reminders(&self) -> DbResult<Vec<PopupReminder>> {
        self.list_popup_reminders_at(Utc::now())
    }

    pub fn list_popup_reminders_at(&self, now: DateTime<Utc>) -> DbResult<Vec<PopupReminder>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT p.event_id, t.id, t.title, p.body, t.due_at
             FROM popup_reminders p
             JOIN reminder_events e ON e.id = p.event_id
             JOIN tasks t ON t.id = e.task_id
             JOIN reminders r ON r.task_id = t.id AND r.offset_seconds = e.offset_seconds
             WHERE p.dismissed = 0 AND t.status = 'pending' AND t.due_at = e.due_at_snapshot
             AND (p.snoozed_until IS NULL OR julianday(p.snoozed_until) <= julianday(?1))
             ORDER BY p.event_id",
        )?;
        let rows = stmt.query_map([now.to_rfc3339()], |row| {
            Ok(PopupReminder {
                id: row.get(0)?,
                task_id: row.get(1)?,
                title: row.get(2)?,
                body: row.get(3)?,
                due_at: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// 仅确认前端实际展示的 ID，避免点击期间到达的新提醒被一并清空。
    pub fn dismiss_popup_reminders(&self, ids: &[i64]) -> DbResult<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        for id in ids {
            tx.execute(
                "UPDATE popup_reminders SET dismissed = 1 WHERE event_id = ?1",
                [id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn snooze_popup_reminders(&self, ids: &[i64], minutes: i64, now: DateTime<Utc>) -> DbResult<()> {
        if ![5,10].contains(&minutes) || ids.len() > 10_000 || ids.is_empty() {
            return Err(DbError::Validation("请选择 5 或 10 分钟，并至少选择一条提醒".into()));
        }
        let until = (now + chrono::Duration::minutes(minutes)).to_rfc3339();
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        for id in ids {
            tx.execute("UPDATE popup_reminders SET snoozed_until=?2 WHERE event_id=?1 AND dismissed=0", rusqlite::params![id,until])?;
        }
        tx.commit()?;
        Ok(())
    }
}
