use super::*;

#[derive(Serialize, Default)]
pub struct BackupStatus {
    pub last_success: Option<String>,
    pub last_attempt: Option<String>,
    pub error: Option<String>,
}

fn read(conn: &Connection, key: &str) -> DbResult<Option<String>> {
    use rusqlite::OptionalExtension;
    Ok(conn
        .query_row("SELECT value FROM app_settings WHERE key=?1", [key], |r| {
            r.get(0)
        })
        .optional()?)
}
fn write(conn: &Connection, key: &str, value: &str) -> DbResult<()> {
    conn.execute("INSERT INTO app_settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",rusqlite::params![key,value])?;
    Ok(())
}
fn recent(value: Option<&String>, now: DateTime<Utc>, seconds: i64) -> bool {
    value
        .and_then(|v| DateTime::parse_from_rfc3339(v).ok())
        .is_some_and(|last| now.signed_duration_since(last).num_seconds() < seconds)
}
impl Database {
    pub fn backup_status(&self) -> DbResult<BackupStatus> {
        let conn = self.conn.lock();
        Self::backup_status_inner(&conn)
    }
    fn backup_status_inner(conn: &Connection) -> DbResult<BackupStatus> {
        Ok(BackupStatus {
            last_success: read(conn, "auto_backup_last_success")?,
            last_attempt: read(conn, "auto_backup_last_attempt")?,
            error: read(conn, "auto_backup_error")?.filter(|v| !v.is_empty()),
        })
    }
    /// 按真实 UTC 时间补做一次备份，不累计补做休眠期间错过的次数。失败后十五分钟再试。
    pub fn run_daily_backup(
        &self,
        directory: &Path,
        now: DateTime<Utc>,
        force: bool,
    ) -> DbResult<BackupStatus> {
        let conn = self.conn.lock();
        let settings = Self::load_settings_inner(&conn)?;
        let status = Self::backup_status_inner(&conn)?;
        if !settings.auto_backup_enabled
            || (!force
                && (recent(status.last_success.as_ref(), now, 86400)
                    || recent(status.last_attempt.as_ref(), now, 900)))
        {
            return Ok(status);
        }
        write(&conn, "auto_backup_last_attempt", &format_iso(now))?;
        let result = (|| -> DbResult<()> {
            super::backups::backup_connection(&conn, directory, "auto")?;
            write(&conn, "auto_backup_last_success", &format_iso(now))?;
            // 仅识别本程序的自动备份文件，不触碰手动和保险备份，也不跟随文件符号链接。
            let mut files = vec![];
            for entry in std::fs::read_dir(directory).map_err(|e| DbError::Io(e.to_string()))? {
                let entry = entry.map_err(|e| DbError::Io(e.to_string()))?;
                let name = entry.file_name().to_string_lossy().into_owned();
                if entry
                    .file_type()
                    .map_err(|e| DbError::Io(e.to_string()))?
                    .is_file()
                    && name.len() > 16
                    && name.starts_with("checkit-")
                    && name.ends_with("-auto.db")
                    && chrono::NaiveDateTime::parse_from_str(
                        &name[8..name.len() - 8],
                        "%Y%m%d-%H%M%S-%f",
                    )
                    .is_ok()
                {
                    files.push(entry.path());
                }
            }
            files.sort();
            let remove_count = files
                .len()
                .saturating_sub(settings.auto_backup_keep as usize);
            for file in files.into_iter().take(remove_count) {
                std::fs::remove_file(file).map_err(|e| DbError::Io(e.to_string()))?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => write(&conn, "auto_backup_error", "")?,
            Err(e) => {
                write(&conn, "auto_backup_error", &e.to_string())?;
                log::error!("Automatic backup failed: {e}");
            }
        }
        Self::backup_status_inner(&conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn daily_backup_catches_up_once_and_retention_preserves_manual_and_safety_copies() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open_in_memory().unwrap();
        let now = Utc::now();
        db.apply_settings_patch(&SettingsPatch {
            auto_backup_keep: Some(3),
            ..Default::default()
        })
        .unwrap();
        let manual = db.create_backup(dir.path(), "manual").unwrap();
        let safety = db.create_backup(dir.path(), "before-restore").unwrap();
        let unrelated = dir.path().join("checkit-not-a-timestamp-auto.db");
        std::fs::write(&unrelated, b"keep").unwrap();
        assert!(db
            .run_daily_backup(dir.path(), now, false)
            .unwrap()
            .last_success
            .is_some());
        db.run_daily_backup(dir.path(), now + chrono::Duration::hours(23), false)
            .unwrap();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 4);
        for day in [1, 2, 10, 11] {
            assert!(db
                .run_daily_backup(dir.path(), now + chrono::Duration::days(day), false)
                .unwrap()
                .error
                .is_none());
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 6);
        assert!(manual.exists() && safety.exists() && unrelated.exists());
    }
    #[test]
    fn failure_is_visible_throttled_and_can_be_retried() {
        let dir = tempfile::tempdir().unwrap();
        let blocked = dir.path().join("blocked");
        std::fs::write(&blocked, b"occupied").unwrap();
        let db = Database::open_in_memory().unwrap();
        let now = Utc::now();
        let status = db.run_daily_backup(&blocked, now, false).unwrap();
        assert!(status.error.is_some());
        assert!(status.last_success.is_none());
        let skipped = db
            .run_daily_backup(dir.path(), now + chrono::Duration::minutes(1), false)
            .unwrap();
        assert!(skipped.last_success.is_none());
        let success = db
            .run_daily_backup(dir.path(), now + chrono::Duration::minutes(1), true)
            .unwrap();
        assert!(success.error.is_none());
        assert!(success.last_success.is_some());
    }
    #[test]
    fn disabled_settings_and_last_success_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("live.db");
        let folder = dir.path().join("backups");
        let now = Utc::now();
        let db = Database::open(&path).unwrap();
        db.apply_settings_patch(&SettingsPatch {
            auto_backup_enabled: Some(false),
            auto_backup_keep: Some(14),
            ..Default::default()
        })
        .unwrap();
        assert!(db
            .run_daily_backup(&folder, now, true)
            .unwrap()
            .last_success
            .is_none());
        assert!(!folder.exists());
        drop(db);
        let db = Database::open(&path).unwrap();
        assert!(!db.load_settings().unwrap().auto_backup_enabled);
        assert_eq!(db.load_settings().unwrap().auto_backup_keep, 14);
        db.apply_settings_patch(&SettingsPatch {
            auto_backup_enabled: Some(true),
            ..Default::default()
        })
        .unwrap();
        db.run_daily_backup(&folder, now, false).unwrap();
        drop(db);
        let db = Database::open(&path).unwrap();
        db.run_daily_backup(&folder, now + chrono::Duration::hours(1), false)
            .unwrap();
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1);
    }
}
