# ARCHITECTURE — 技术架构

## 1. 总体结构

```
┌──────────────────────────────────────────────────────────┐
│  WebView 进程（每个窗口一个 WebView2 实例）                 │
│  ┌────────────────────┐   ┌───────────────────────────┐  │
│  │ widget 窗口 (React) │   │ manager 窗口 (React)       │  │
│  │  同一前端包，按      │   │                            │  │
│  │  window.label 分发  │   │                            │  │
│  └─────────┬──────────┘   └──────────┬────────────────┘  │
│            │ invoke / event（IPC）    │                   │
└────────────┼─────────────────────────┼───────────────────┘
             ▼                         ▼
┌──────────────────────────────────────────────────────────┐
│  Rust 主进程                                              │
│  commands.rs ──► database (SQLite, WAL, parking_lot)     │
│       │                        ▲                         │
│       │ 写后广播 data-changed   │ list_schedulable_tasks  │
│       ▼                        │ record_reminder_fired   │
│  events → 所有窗口同步刷新      │                         │
│                                │                         │
│  scheduler 线程（独立于窗口）──► reminders::ReminderEngine │
│       │ run_once(now) 每次以真实时间重算                    │
│       ▼                                                  │
│  tauri-plugin-notification → Windows 原生通知              │
│                                                          │
│  tray.rs ──► 菜单/单击行为      windows.rs ──► 几何/效果    │
└──────────────────────────────────────────────────────────┘
```

## 2. 前端架构

- **入口分发**：`App.tsx` 读取 `getCurrentWindow().label`，`widget` 渲染悬浮组件，`manager` 渲染管理窗口（同一 bundle）。
- **数据层**：`useAppData` 上下文是唯一数据源。启动时调用一次 `get_app_state` 全量加载；监听后端 `data-changed` 事件，任何窗口的写操作后由后端广播驱动所有窗口重取 —— 保证多窗口一致性，前端不做本地缓存写。
- **通信**：`services/api.ts` 是唯一 invoke 入口，统一错误处理；组件不得直接 invoke。
- **时钟**：`useNow` 提供全应用唯一的 1Hz 时钟（Context 分发），并监听 `visibilitychange/focus` 处理休眠恢复。倒计时组件全部由父级传入 `now`，自身无定时器。
- **纯逻辑层**：`utils/`（countdown / time / tasks / reminders）为无副作用纯函数，全部有单元测试。
- **主题**：`useTheme` 将设置写入 `<html>` 类名与 CSS 变量；Tailwind v4 用 `@custom-variant dark` 基于类切换。
- **玻璃效果**：Rust 侧 `set_effects(Acrylic)` 启用原生模糊；CSS 变量 `--glass-opacity` 控制面板底色。原生效果不可用时自动降级为半透明/不透明。

## 3. Rust 架构

| 模块 | 职责 |
| --- | --- |
| `main.rs` / `lib.rs` | 入口与组装：插件注册、状态管理、setup |
| `commands.rs` | Tauri 命令层；所有写操作后广播 `data-changed` 并唤醒调度器 |
| `database/` | 连接（WAL/busy_timeout）、版本迁移、CRUD、设置与窗口几何持久化 |
| `models/` | serde 模型（snake_case 与前端一一对应）、输入校验与规范化 |
| `reminders/` | 提醒引擎 `run_once(now)`：纯逻辑 + 数据库幂等 |
| `windows.rs` | 窗口事件（几何节流保存、关闭到托盘）、管理窗口创建、效果应用 |
| `tray.rs` | 托盘菜单与行为 |
| `settings.rs` | 开机启动同步（绝不默认开启） |

**状态管理**：`CheckItState { db: Database, wake: Sender<()> }` 由 `app.manage()` 托管；`Database` 内部为 `Arc<Mutex<Connection>>`（parking_lot），天然支持多窗口/线程并发调用。

## 4. 提醒调度设计（可靠性核心）

1. **绝对时间**：`run_once` 每轮对每个待完成任务的每个提醒偏移计算 `due_at - offset`，与真实 `now` 比较。不维护任何增量状态，因此休眠恢复、系统时间调整、进程重启全部自然正确。
2. **幂等**：触发前执行 `INSERT OR IGNORE INTO reminder_events`，插入成功（受唯一约束保护）才通知。轮询/重绘/重启都不会重复弹出。
3. **补发宽限**：`now - scheduled ≤ 15 分钟` → 发通知；超过 → 仅记录（`MarkMissed`），避免唤醒后通知洪泛。
4. **状态同步**：完成任务 → 调度器过滤 `status != pending`，不再触发；修改截止时间 → `update_task` 清除该任务的触发记录，新时间线重新计算；编辑/删除后命令层立即唤醒调度器。
5. **线程模型**：调度线程 `recv_timeout(5s)` 等待，既可被 `check_reminders_now` 立即唤醒，也保证空闲时低频扫描。

## 5. 时间策略

- 存储：SQLite 中全部为 RFC3339 UTC 字符串
- 计算：Rust chrono / JS Date，均按时间戳运算，禁止字符串比较
- 展示：一律转为用户本地时区（`datetime-local` 输入、格式化文本）
- 紧急阈值：前端 `URGENCY_THRESHOLDS`、后端 `MISSED_GRACE_SECS` 集中定义

## 6. 安全

- 全部 SQL 使用参数绑定（含注入测试用例）
- Tauri capabilities 白名单（core:window 最小集 + event/app）
- CSP 当前为 null（本地静态资源，无远程内容；后续可收紧）
- 自定义命令仅暴露业务操作，无任意路径/命令执行

## 7. 构建

- 前端：Vite（`tsc --noEmit` 严格检查 → `vite build`）
- 桌面：`tauri build`（NSIS 安装包，currentUser 模式）
- CI：GitHub Actions windows-latest 构建并上传产物
