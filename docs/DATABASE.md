# DATABASE — 数据库设计

文件位置：`%APPDATA%\com.checkit.desktop\checkit.db`（SQLite，WAL 模式，busy_timeout 2s，外键开启）

## ER 概览

```
categories 1 ──── N tasks 1 ──── N reminders
                       1 ──── N reminder_events（状态机与快照去重）
app_settings（key-value）
schema_migrations（版本）
```

## 表结构（v4）

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

### reminder_events（提醒去重 + 状态机，v3）

| 列 | 类型 | 说明 |
| --- | --- | --- |
| id | INTEGER PK AUTOINCREMENT | |
| task_id | INTEGER NOT NULL | 不设外键（保留历史） |
| offset_seconds | INTEGER NOT NULL | |
| due_at_snapshot | TEXT NOT NULL | **领取时任务的截止时间**（快照键组成部分） |
| state | TEXT CHECK IN ('pending','submitted','failed','skipped','cancelled') | pending=已领取待同步提交；submitted=OS/系统通知 API 已受理（**≠ 用户已看到横幅**）；failed=同步提交返回失败，可重试；skipped=超 15 分钟宽限或通知关闭时静默；cancelled=任务改期/完成/删除或提醒被移除后失效 |
| attempts | INTEGER NOT NULL DEFAULT 0 | failed 重试计数（上限 3） |
| created_at / updated_at | TEXT NOT NULL | updated_at 兼作崩溃检测与重试退避依据；submitted/failed 写入**实际提交完成时刻**（pending 超 90s 视为崩溃遗留；failed 从失败完成时刻起 30s 退避） |

约束：UNIQUE(task_id, offset_seconds, due_at_snapshot)。
- 改期产生新快照键 → 新计划不受旧记录抑制；旧键 pending 动作在提交前二次校验中置 cancelled
- 该表为 at-least-once：崩溃窗口（OS 已受理但 submitted 尚未落库）恢复后可能重复提交，不承诺 exactly-once
- v1 旧表迁移（v2）：历史行 state='sent'，snapshot 回填任务当前 due_at（未改期任务去重语义不变）
- v2→v3：仅将 state='sent' 重命名为 'submitted'，逐行保留 id/attempts/created_at/updated_at 与唯一键；
  重建表以更新 CHECK 约束，不删除任何事件记录

索引：task_id、state。

### popup_reminders（v4 居中弹窗收件箱）

| 列 | 类型 | 说明 |
| --- | --- | --- |
| event_id | INTEGER PK → reminder_events(id) ON DELETE CASCADE | 每条提醒事件最多入箱一次 |
| body | TEXT NOT NULL | 提交时提醒正文快照 |
| dismissed | INTEGER CHECK IN (0,1) DEFAULT 0 | 确认过的提醒保留标记，重试不会重新弹出 |

仅展示任务仍待完成、截止时间快照和提前量仍有效的未确认记录。确认按前端实际展示的 ID 批量执行事务，不清空同时到达的新提醒。重启后恢复未确认记录（收件箱恢复不受新提醒 15 分钟补发宽限限制）。预览不写入此表。

两种方式同时开启时先幂等入箱并展示，再提交系统通知；系统通知失败由既有状态机重试，弹窗不会重复入箱。`submitted` 在仅弹窗模式下表示持久化并成功请求展示，仍不代表用户已确认；是否确认以 `dismissed` 为准。

### app_settings（key-value）

| key | 值 | 默认 |
| --- | --- | --- |
| theme | dark / light / system | dark |
| glass_enabled | true/false | true |
| glass_opacity | 0.05–0.6 | 0.35 |
| always_on_top | true/false | true |
| close_to_tray | true/false | true |
| notifications_enabled | true/false | true |
| popup_reminders_enabled | true/false，独立于系统通知开关 | false |
| widget_display_count | 3–20 | 8 |
| launch_at_login | true/false | false |
| default_reminder_offsets | JSON 数组 | [3600,600] |
| widget_px_x/y/w/h | 数字（物理像素） | 启动后拖动/缩放结束（或关闭窗口）时写入；仅保存展开态尺寸，收起时保留上次展开高度 |

### schema_migrations

| version | applied_at |
| --- | --- |

迁移在 `Database::migrate()` 中顺序执行 v1→v2→v3→v4；重复打开时跳过已应用版本（幂等，见测试 `migration_is_idempotent`）。v4 仅增加弹窗收件箱，不改写已有任务或提醒记录。
新建库在一次打开中依次应用全部版本；v1/v2 旧库自动升级且保留数据。

## 一致性策略

- 多窗口并发：WAL + busy_timeout + 事务
- **事务边界**：create_task / update_task / save_window_geometry 与设置写入均为显式事务，
  任一步失败整体回滚（任务不会带半套提醒、设置不会写一半）
- **设置原子补丁**：`update_settings_atomic` 在同一把连接锁 + 同一事务内完成
  「读取 → 应用 patch → 校验 → 写回」，`apply_settings_patch` / `toggle_always_on_top` 都走该路径，
  并发修改不同字段不会互相覆盖，也不存在读锁重入
- 写路径同步：命令层写库成功后广播 `data-changed`，所有窗口重取
- 截止时间修改：**不再删除事件记录**——快照键天然区分新旧计划；旧动作在提交前失效
- 完成任务：调度器仅扫描 pending 任务；旧动作在提交前被取消；已提交记录保留
- 日期一律 RFC3339 UTC 存储，展示层本地化
- 全部查询参数绑定；层内校验（空标题、非法时间、非法颜色）
- 窗口几何以**物理像素**存储（键名带 `_px_` 前缀），跨 DPI 恢复无歧义
- **单调度器前提**：`recoverable_reminders` 是 SELECT 查询，恢复领取并非跨进程原子操作；
  产品通过 `tauri-plugin-single-instance` 保证同一时刻只有一个应用进程/调度器。引擎明确不支持
  多调度器并行，绕过单实例保护可能重复领取并重复提交。本版无新增数据库迁移，schema 仍为 v3；
  时间戳语义变化（使用提交完成时刻）不改变表结构，不需要迁移
