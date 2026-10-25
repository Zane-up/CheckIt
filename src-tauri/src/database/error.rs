//! 数据库错误类型。

use std::fmt;

#[derive(Debug)]
pub enum DbError {
    Sqlite(String),
    Io(String),
    Validation(String),
    NotFound,
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DbError::Sqlite(m) => write!(f, "数据库错误: {m}"),
            DbError::Io(m) => write!(f, "文件错误: {m}"),
            DbError::Validation(m) => write!(f, "{m}"),
            DbError::NotFound => write!(f, "记录不存在"),
        }
    }
}

impl std::error::Error for DbError {}

impl From<rusqlite::Error> for DbError {
    fn from(e: rusqlite::Error) -> Self {
        match &e {
            rusqlite::Error::QueryReturnedNoRows => DbError::NotFound,
            _ => DbError::Sqlite(e.to_string()),
        }
    }
}

impl From<std::ffi::NulError> for DbError {
    fn from(e: std::ffi::NulError) -> Self {
        DbError::Sqlite(e.to_string())
    }
}
