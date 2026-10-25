# DATABASE — 数据库设计

文件位置：`%APPDATA%\com.checkit.desktop\checkit.db`（SQLite，WAL 模式，busy_timeout 2s，外键开启）

## ER 概览

```
categories 1 ──── N tasks 1 ──── N reminders
                       1 ──── N reminder_events（触发记录，幂等）
app_settings（key-value）
schema_migrations（版本）
```

## 表结构（v1）

### tasks

| 列 | 类型 | 说明 |
| --- | --- | --- |
| id | INTEGER PK AUTOINCREMENT | |
| title | TEXT NOT NULL | 任务名称（trim 后非空，应用层校验） |
| description | TEXT NOT NULL DEFAULT '' | |
| category_id | INTEGER NULL → categories(id) ON DELETE SET NULL | 删除分类不清除任务 |
| priority | TEXT CHECK IN ('high','medium','low') DEFAULT 'medium' | |
| status | TEXT CHECK IN ('pending','completed') DEFAULT 'pending' | Overdue 为派生，不落库 |
| due_at | TEXT NULL | RFC3339 UTC，NULL = 无截止 |
| created_at / updated_at | TEXT NOT NULL | RFC3339 UTC |
| completed_at | TEXT NULL | |

索引：`idx_tasks_status_due(status, due_at)`、`idx_tasks_category(category_id)`、`idx_tasks_due_at(due_at)`

### categories

| 列 | 类型 | 说明 |
| --- | --- | --- |
| id | INTEGER PK AUTOINCREMENT | |
| name | TEXT NOT NULL UNIQUE | |
| color | TEXT NOT NULL DEFAULT '#6366f1' | #RRGGBB |
| sort_order | INTEGER NOT NULL DEFAULT 0 | |
| created_at / updated_at | TEXT NOT NULL | |

默认数据：学习 #6366f1 / 工作 #0ea5e9 / 会议 #f59e0b / 生活 #10b981 / 其他 #64748b

### reminders（任务配置的提醒规则）

| 列 | 类型 | 说明 |
| --- | --- | --- |
| id | INTEGER PK AUTOINCREMENT | |
| task_id | INTEGER NOT NULL → tasks(id) ON DELETE CASCADE | |
| offset_seconds | INTEGER NOT NULL CHECK >= 0 | 提前秒数，0 = 截止时 |

约束：UNIQUE(task_id, offset_seconds)

### reminder_events（提醒触发记录 = 幂等依据）

| 列 | 类型 | 说明 |
| --- | --- | --- |
| id | INTEGER PK AUTOINCREMENT | |
| task_id | INTEGER NOT NULL | 冗余存储，不设外键（允许保留历史） |
| offset_seconds | INTEGER NOT NULL | |
| fired_at | TEXT NOT NULL | 实际记录时间 |

约束：UNIQUE(task_id, offset_seconds) —— 同一提醒事件全局最多一条记录。
INSERT OR IGNORE 的affected 行数即"是否首次触发"。

### app_settings（key-value）

| key | 值 | 默认 |
| --- | --- | --- |
| theme | dark / light / system | dark |
| glass_enabled | true/false | true |
| glass_opacity | 0.05–0.6 | 0.35 |
| always_on_top | true/false | true |
| close_to_tray | true/false | true |
| notifications_enabled | true/false | true |
| widget_display_count | 3–20 | 8 |
| launch_at_login | true/false | false |
| default_reminder_offsets | JSON 数组 | [3600,600] |
| widget_x/y/w/h | 数字（逻辑像素） | 启动后首次拖动/缩放时写入 |

### schema_migrations

| version | applied_at |
| --- | --- |

迁移在 `Database::migrate()` 中顺序执行；重复打开时跳过已应用版本（幂等，见测试 `migration_is_idempotent`）。

## 一致性策略

- 多窗口并发：WAL + busy_timeout + 每操作短事务（单语句原子）
- 写路径同步：命令层写库成功后广播 `data-changed`，所有窗口重取
- 截止时间修改：清除该任务全部 reminder_events，重新调度
- 完成任务：调度器仅扫描 pending 任务；已触发记录保留
- 日期一律 RFC3339 UTC 存储，展示层本地化
- 全部查询参数绑定；层内校验（空标题、非法时间、非法颜色）
