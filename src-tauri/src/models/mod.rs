//! CheckIt 数据模型：与前端 src/types 一一对应（serde snake_case）。

pub mod category;
pub mod settings;
pub mod task;

pub use category::Category;
pub use settings::{AppSettings, SettingsPatch};
pub use task::{Priority, Task, TaskInput, TaskStatus};
