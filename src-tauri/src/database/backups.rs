use super::*;
use std::path::PathBuf;

/// SQLite 在线备份包含 WAL 中已提交的数据，失败文件不出现在可恢复列表中。
pub fn backup_connection(conn: &Connection, directory: &Path, reason: &str) -> DbResult<PathBuf> {
    std::fs::create_dir_all(directory).map_err(|e| DbError::Io(e.to_string()))?;
    let name = format!(
        "checkit-{}-{}.db",
        Utc::now().format("%Y%m%d-%H%M%S-%f"),
        reason
    );
    let destination = directory.join(name);
    let pending = destination.with_extension("partial");
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)
        .map_err(|e| DbError::Io(e.to_string()))?;
    conn.backup("main", &pending, None)?;
    // 备份是可单独复制的文件，不依赖 -wal/-shm 伴随文件。
    Connection::open(&pending)?.pragma_update(None, "journal_mode", "DELETE")?;
    // 同步后再发布完整备份；不覆盖已有备份。
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&pending)
        .and_then(|f| f.sync_all())
        .map_err(|e| DbError::Io(e.to_string()))?;
    std::fs::rename(&pending, &destination).map_err(|e| DbError::Io(e.to_string()))?;
    Ok(destination)
}

impl Database {
    pub fn create_backup(&self, directory: &Path, reason: &str) -> DbResult<PathBuf> {
        backup_connection(&self.conn.lock(), directory, reason)
    }

    pub fn restore_backup(&self, source: &Path, directory: &Path) -> DbResult<()> {
        let input =
            Connection::open_with_flags(source, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let check: String = input.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        let version: i64 =
            input.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
                r.get(0)
            })?;
        if check != "ok" || !(4..=6).contains(&version) {
            return Err(DbError::Validation(
                "备份损坏或数据库版本不兼容（支持 v0.2.0 及之后的当前结构）".into(),
            ));
        }
        if input.prepare("PRAGMA foreign_key_check")?.exists([])? {
            return Err(DbError::Validation("备份存在无效关联，已取消恢复".into()));
        }
        // 在临时内存库迁移旧备份，不修改用户选中的原文件。
        let mut staged = Connection::open_in_memory()?;
        rusqlite::backup::Backup::new(&input, &mut staged)?.run_to_completion(
            100,
            std::time::Duration::ZERO,
            None,
        )?;
        let validated = Self::init_connection(staged, Path::new(":memory:"))?;
        validated.get_app_state()?;
        let source_conn = validated.conn.lock();
        // 持同一连接锁完成保险备份与原子恢复，期间写入不能插入。
        let mut destination = self.conn.lock();
        backup_connection(&destination, directory, "before-restore")?;
        let restore = rusqlite::backup::Backup::new(&source_conn, &mut destination)?;
        restore.run_to_completion(100, std::time::Duration::ZERO, None)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wal_backup_restore_preserves_settings_events_and_creates_safety_copy() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("live.db")).unwrap();
        db.ensure_default_categories().unwrap();
        db.conn
            .lock()
            .execute(
                "INSERT INTO app_settings(key,value) VALUES('test','original')",
                [],
            )
            .unwrap();
        let backup = db
            .create_backup(&dir.path().join("backups"), "manual")
            .unwrap();
        db.conn
            .lock()
            .execute(
                "UPDATE app_settings SET value='changed' WHERE key='test'",
                [],
            )
            .unwrap();
        db.restore_backup(&backup, &dir.path().join("backups"))
            .unwrap();
        let value: String = db
            .conn
            .lock()
            .query_row("SELECT value FROM app_settings WHERE key='test'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(value, "original");
        assert_eq!(
            std::fs::read_dir(dir.path().join("backups"))
                .unwrap()
                .count(),
            2
        );
        assert_eq!(db.list_categories().unwrap().len(), 5);
    }
    #[test]
    fn invalid_backup_leaves_live_database_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open_in_memory().unwrap();
        db.ensure_default_categories().unwrap();
        let file = dir.path().join("bad.db");
        std::fs::write(&file, b"broken").unwrap();
        assert!(db.restore_backup(&file, dir.path()).is_err());
        assert_eq!(db.list_categories().unwrap().len(), 5);
    }

    #[test]
    fn old_v4_backup_is_migrated_without_changing_original() {
        let dir=tempfile::tempdir().unwrap();
        let db=Database::open_in_memory().unwrap();
        db.ensure_default_categories().unwrap();
        let file=db.create_backup(dir.path(),"manual").unwrap();
        let old=Connection::open(&file).unwrap();
        old.execute_batch("ALTER TABLE popup_reminders DROP COLUMN snoozed_until; DELETE FROM schema_migrations WHERE version>=5; DROP TABLE task_trash;").unwrap();
        drop(old);
        db.restore_backup(&file,dir.path()).unwrap();
        assert!(db.list_popup_reminders().unwrap().is_empty());
        let original=Connection::open(&file).unwrap();
        assert_eq!(original.query_row("SELECT MAX(version) FROM schema_migrations",[],|r|r.get::<_,i64>(0)).unwrap(),4);
    }

    #[test]
    fn safety_backup_failure_aborts_restore_before_live_changes() {
        let dir=tempfile::tempdir().unwrap();
        let db=Database::open_in_memory().unwrap();
        let backup=db.create_backup(dir.path(),"manual").unwrap();
        db.ensure_default_categories().unwrap();
        let blocked=dir.path().join("not-a-directory");
        std::fs::write(&blocked,b"occupied").unwrap();
        assert!(db.restore_backup(&backup,&blocked).is_err());
        assert_eq!(db.list_categories().unwrap().len(),5);
    }
}
