//! 最小持久化诊断日志。
//!
//! 此前项目只使用 `log` 宏但从未初始化 logger，安装版里的通知失败等错误
//! 只会进入内存后丢失，无法取得底层 HRESULT。这里把日志追加写入应用日志目录，
//! 保留错误码用于定位；不记录任务标题/描述等业务内容。
//!
//! 保留策略（运行期生效，不只在启动时检查）：
//! - 活动文件 `checkit.log` 上限 [`MAX_LOG_BYTES`]（1 MiB）；
//! - 归档 `checkit.log.1` ~ `checkit.log.3` 共 [`MAX_ARCHIVES`] 个，轮转时删除最旧；
//! - 因此磁盘占用上限约为 4 MiB（外加单条记录，记录本身也会被截断到单文件上限）；
//! - 每次写入前在同一把互斥锁内完成「检查大小 → 轮转 → 写入」，多线程并发安全；
//! - 打开/写入/轮转失败一律静默丢弃本轮日志，绝不递归记录（避免日志系统自我放大）。

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use log::{LevelFilter, Log, Metadata, Record};

/// 单个活动日志文件上限。
const MAX_LOG_BYTES: u64 = 1024 * 1024;
/// 归档文件数量上限（checkit.log.1 … checkit.log.N）。
const MAX_ARCHIVES: usize = 3;

struct WriterState {
    /// 保持打开的追加句柄；轮转前必须置空以释放 Windows 文件占用。
    file: Option<File>,
    /// 活动文件当前字节数，避免每次写入都 stat。
    written: u64,
}

struct FileLogger {
    path: PathBuf,
    level: LevelFilter,
    max_bytes: u64,
    max_archives: usize,
    state: Mutex<WriterState>,
}

impl FileLogger {
    fn new(path: PathBuf, level: LevelFilter, max_bytes: u64, max_archives: usize) -> Self {
        let written = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        Self {
            path,
            level,
            max_bytes: max_bytes.max(1),
            max_archives,
            state: Mutex::new(WriterState { file: None, written }),
        }
    }

    fn archive_path(&self, index: usize) -> PathBuf {
        PathBuf::from(format!("{}.{index}", self.path.display()))
    }

    /// 轮转：关闭句柄 → 删除最旧归档 → 依次后移 → 活动文件改名为 `.1`。
    ///
    /// 任何失败都不向上抛出、也不记录日志；改名失败时退化为删除活动文件，
    /// 保证文件不会无限增长。返回 false 表示无法让出活动文件，本轮应丢弃。
    fn rotate(&self, state: &mut WriterState) -> bool {
        state.file = None;
        let _ = fs::remove_file(self.archive_path(self.max_archives));
        for index in (1..self.max_archives).rev() {
            let _ = fs::rename(self.archive_path(index), self.archive_path(index + 1));
        }
        if fs::rename(&self.path, self.archive_path(1)).is_err()
            && fs::remove_file(&self.path).is_err()
        {
            // 既不能改名也不能删除（如被杀软/其它进程占用）：保留真实文件大小继续丢弃，
            // 绝不能把 written 当成 0，否则下一条记录会跳过容量检查继续追加旧文件。
            state.written = self.actual_size(state.written);
            return false;
        }
        state.written = 0;
        true
    }

    /// 读取活动文件实际大小；读不到时退回传入的已知大小，避免误判为空文件。
    fn actual_size(&self, fallback: u64) -> u64 {
        fs::metadata(&self.path)
            .map(|meta| meta.len())
            .unwrap_or(fallback)
    }

    /// 让单条记录适配单文件上限：先为截断标记与换行预留字节，再按 UTF-8 边界截断，
    /// 保证最终字节数（含标记）不超过 max_bytes。
    fn fit_line(&self, mut line: String) -> String {
        const MARKER: &str = "…(truncated)\n";
        let max = self.max_bytes as usize;
        if line.len() <= max {
            return line;
        }
        if max <= MARKER.len() {
            // 上限连标记都放不下（极小上限的测试/配置）：只保留能容纳的内容和换行。
            let suffix = if max >= 1 { "\n" } else { "" };
            let mut cutoff = max - suffix.len();
            while cutoff > 0 && !line.is_char_boundary(cutoff) {
                cutoff -= 1;
            }
            line.truncate(cutoff);
            line.push_str(suffix);
            return line;
        }
        let mut cutoff = max - MARKER.len();
        while cutoff > 0 && !line.is_char_boundary(cutoff) {
            cutoff -= 1;
        }
        line.truncate(cutoff);
        line.push_str(MARKER);
        line
    }

    /// 串行化「检查 → 轮转 → 写入」；锁毒化后继续使用内部状态，避免日志整体失效。
    fn write_line(&self, line: &str) {
        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let bytes = line.len() as u64;
        if state.written > 0 && state.written + bytes > self.max_bytes {
            if !self.rotate(&mut state) {
                return;
            }
        }
        if state.file.is_none() {
            match OpenOptions::new().create(true).append(true).open(&self.path) {
                Ok(file) => state.file = Some(file),
                Err(_) => return,
            }
        }
        if let Some(file) = state.file.as_mut() {
            if file.write_all(line.as_bytes()).is_ok() {
                state.written += bytes;
            }
        }
    }
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let mut line = format!(
            "[{}] {:<5} {}: {}\n",
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
            record.level(),
            record.target(),
            record.args()
        );
        // 单条记录也受单文件上限约束：标记与换行计入预算，按 UTF-8 边界截断。
        line = self.fit_line(line);
        self.write_line(&line);
    }

    fn flush(&self) {}
}

static LOGGER: OnceLock<FileLogger> = OnceLock::new();

/// 初始化全局文件日志。重复调用直接忽略。
pub fn init(log_dir: &Path) {
    if LOGGER.get().is_some() {
        return;
    }
    let _ = fs::create_dir_all(log_dir);
    let path = log_dir.join("checkit.log");
    let level = if cfg!(debug_assertions) {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };
    let logger = FileLogger::new(path, level, MAX_LOG_BYTES, MAX_ARCHIVES);
    if LOGGER.set(logger).is_ok() {
        if let Some(logger) = LOGGER.get() {
            let _ = log::set_logger(logger);
            log::set_max_level(level);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use log::{Level, Record};
    use std::sync::Arc;
    use std::thread;

    /// 在调用点构造记录，避免 `format_args!` 借用局部变量后无法跨语句保存。
    macro_rules! log_line {
        ($logger:expr, $level:expr, $($arg:tt)*) => {{
            $logger.log(
                &Record::builder()
                    .args(format_args!($($arg)*))
                    .level($level)
                    .target("checkit_lib::tests")
                    .build(),
            );
        }};
    }

    fn archive_files(dir: &Path) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = fs::read_dir(dir)
            .unwrap()
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.starts_with("checkit.log"))
                    .unwrap_or(false)
            })
            .collect();
        files.sort();
        files
    }

    #[test]
    fn notification_error_with_hresult_is_persisted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("checkit.log");
        let logger = FileLogger::new(path.clone(), LevelFilter::Info, MAX_LOG_BYTES, MAX_ARCHIVES);
        logger.log(&Record::builder()
            .args(format_args!(
                "提醒提交失败，已进入重试状态机: Windows API error: 0x80070490"
            ))
            .level(Level::Error)
            .target("checkit_lib::lib")
            .build());
        logger.log(&Record::builder()
            .args(format_args!(
                "提醒提交失败，已进入重试状态机: Windows API error: E_ACCESSDENIED (0x80070005)"
            ))
            .level(Level::Error)
            .target("checkit_lib::lib")
            .build());
        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("0x80070490"), "日志应保留底层 HRESULT");
        assert!(
            content.contains("0x80070005"),
            "日志应保留第二个底层错误码"
        );
        assert!(content.contains("checkit_lib::lib"), "日志应保留来源");
        assert!(content.contains("ERROR"), "日志应保留级别");
    }

    #[test]
    fn rotates_while_running_without_reinitializing() {
        let dir = tempfile::tempdir().unwrap();
        // 不经过全局 LOGGER / init，直接写同一个实例，模拟长期运行期间持续报错。
        let logger = FileLogger::new(dir.path().join("checkit.log"), LevelFilter::Info, 256, 2);
        for index in 0..40 {
            log_line!(logger, Level::Error, "持续错误 #{}: error=0x80070490", index);
        }
        let files = archive_files(dir.path());
        assert!(
            dir.path().join("checkit.log.1").exists(),
            "运行期间应发生轮转，实际文件: {files:?}"
        );
        assert!(
            files.len() <= 3,
            "活动文件 + 归档不得超过 3 个，实际: {files:?}"
        );
        let total: u64 = files
            .iter()
            .filter_map(|path| fs::metadata(path).ok())
            .map(|meta| meta.len())
            .sum();
        assert!(
            total <= 256 * 3,
            "总占用应受 256×3 约束，实际 {total} 字节"
        );
    }

    #[test]
    fn archive_count_and_size_are_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let logger = FileLogger::new(dir.path().join("checkit.log"), LevelFilter::Info, 512, 3);
        // 写入远高于容量上限的数据。
        for index in 0..2000 {
            log_line!(logger, Level::Info, "填充日志 {}", index);
        }
        let files = archive_files(dir.path());
        assert!(
            files.len() <= 4,
            "活动文件 + 最多 3 个归档，实际: {files:?}"
        );
        let total: u64 = files
            .iter()
            .filter_map(|path| fs::metadata(path).ok())
            .map(|meta| meta.len())
            .sum();
        assert!(
            total <= 512 * 4,
            "总占用应受 512×4 约束，实际 {total} 字节"
        );
        // 归档编号不应无限增长
        assert!(dir.path().join("checkit.log.3").exists());
        assert!(!dir.path().join("checkit.log.4").exists());
    }

    #[test]
    fn oversized_utf8_records_stay_within_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("checkit.log");
        let logger = FileLogger::new(path.clone(), LevelFilter::Info, 128, 2);

        // 超长中文记录：截断后必须仍是合法 UTF-8，且总字节数不超过上限。
        log_line!(logger, Level::Error, "{}", "超长中文日志内容".repeat(50));
        let content = fs::read_to_string(&path).expect("截断后仍应是合法 UTF-8");
        assert!(
            content.len() <= 128,
            "中文长记录不得突破上限，实际 {} 字节",
            content.len()
        );
        assert!(content.contains("…(truncated)"), "应带截断标记: {content:?}");

        // 超长英文记录：同样受限，并触发轮转。
        for index in 0..20 {
            log_line!(
                logger,
                Level::Error,
                "{} {}",
                "A".repeat(300),
                index
            );
        }
        let files = archive_files(dir.path());
        assert!(files.len() <= 3, "文件数应受约束: {files:?}");
        for file in files {
            let text = fs::read_to_string(&file).expect("所有归档都应是合法 UTF-8");
            let size = fs::metadata(&file).unwrap().len();
            assert!(size <= 128, "{} 超过上限: {size} 字节", file.display());
            assert!(text.is_char_boundary(text.len()), "UTF-8 边界应完整");
        }
    }

    #[test]
    fn very_small_limit_is_still_respected() {
        for limit in [1_u64, 8, 15] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("checkit.log");
            let logger = FileLogger::new(path.clone(), LevelFilter::Info, limit, 1);
            log_line!(logger, Level::Error, "{}", "长内容".repeat(40));
            let size = fs::metadata(&path).unwrap().len();
            assert!(
                size <= limit,
                "上限 {limit} 字节时实际写入 {size} 字节"
            );
            fs::read_to_string(&path).expect("极小上限下仍需是合法 UTF-8");
        }
    }

    #[cfg(windows)]
    #[test]
    fn rotation_failure_keeps_real_size_and_recovers() {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_SHARE_READ | FILE_SHARE_WRITE：允许其它句柄读写，但拒绝删除/改名。
        const SHARE_READ_WRITE: u32 = 0x1 | 0x2;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("checkit.log");
        // 先占住文件（模拟杀软/其它进程），再让 logger 打开同一个文件追加。
        let guard = OpenOptions::new()
            .create(true)
            .append(true)
            .share_mode(SHARE_READ_WRITE)
            .open(&path)
            .unwrap();

        let logger = FileLogger::new(path.clone(), LevelFilter::Info, 128, 2);
        // 复现用例：轮转失败后曾把 written 清零，导致持续追加到 960 字节。
        for index in 0..30 {
            log_line!(logger, Level::Error, "被占用的日志 {}", index);
        }
        let locked_size = fs::metadata(&path).unwrap().len();
        assert!(
            locked_size <= 128,
            "禁止删除时应持续丢弃，实际 {locked_size} 字节"
        );
        assert!(
            !dir.path().join("checkit.log.1").exists(),
            "文件被占用时不应产生归档"
        );

        // 解除占用后应恢复正常轮转，且仍受容量约束。
        drop(guard);
        for index in 0..30 {
            log_line!(logger, Level::Error, "占用解除后的日志 {}", index);
        }
        assert!(
            dir.path().join("checkit.log.1").exists(),
            "占用解除后应恢复轮转"
        );
        let files = archive_files(dir.path());
        let total: u64 = files
            .iter()
            .filter_map(|path| fs::metadata(path).ok())
            .map(|meta| meta.len())
            .sum();
        assert!(
            total <= 128 * 3,
            "恢复轮转后总占用仍应受约束，实际 {total} 字节"
        );
    }

    #[test]
    fn concurrent_writes_do_not_corrupt_rotation() {
        let dir = tempfile::tempdir().unwrap();
        let logger = Arc::new(FileLogger::new(
            dir.path().join("checkit.log"),
            LevelFilter::Info,
            512,
            2,
        ));
        let mut handles = Vec::new();
        for thread_id in 0..8 {
            let logger = Arc::clone(&logger);
            handles.push(thread::spawn(move || {
                for index in 0..200 {
                    log_line!(
                        logger,
                        Level::Error,
                        "线程{} 记录{} error=0x80070490",
                        thread_id,
                        index
                    );
                }
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
        let files = archive_files(dir.path());
        assert!(files.len() <= 3, "并发轮转后文件数仍应受约束: {files:?}");
        let total: u64 = files
            .iter()
            .filter_map(|path| fs::metadata(path).ok())
            .map(|meta| meta.len())
            .sum();
        assert!(
            total <= 512 * 3,
            "并发写入后总占用仍应受约束，实际 {total} 字节"
        );
        // 每行必须是完整记录：并发写入不能交错成半行，也不能丢失行首时间戳。
        for path in files {
            let content = fs::read_to_string(&path).unwrap();
            for line in content.lines() {
                assert!(line.starts_with('['), "发现被破坏的日志行: {line:?}");
                assert!(
                    line.contains("checkit_lib::tests"),
                    "日志行应保留来源: {line:?}"
                );
            }
            if !content.is_empty() {
                assert!(content.ends_with('\n'), "日志文件应以换行结尾");
            }
        }
    }
}
