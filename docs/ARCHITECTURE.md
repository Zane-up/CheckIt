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
│       │ run_once(scan_now) 领取/恢复共用扫描快照            │
│       │ 每条动作提交前 deliver_with_clock 实时取时并重校验  │
│       ▼                                                  │
│  notification::NotificationTransport → WinRT 原生通知      │
│       （同步提交结果：submitted / failed 重试）              │
│                                                          │
│  单实例插件（最先注册）──► 第二实例唤醒已有窗口后退出        │
│  tray.rs ──► 菜单/单击行为      windows.rs ──► 几何/效果    │
└──────────────────────────────────────────────────────────┘
```

## 2. 前端架构

- **入口分发**：`App.tsx` 读取 `getCurrentWindow().label`，`widget` 渲染悬浮组件，`manager` 渲染管理窗口（同一 bundle）。
- **数据层**：`useAppData` 上下文是唯一数据源。启动时**先注册 `data-changed` 订阅、再加载** `get_app_state` 全量快照，并用请求序号丢弃过期响应——避免初始窗口内丢事件；任何窗口的写操作后由后端广播驱动所有窗口重取，前端不做本地缓存写。刷新失败时保留上次数据与组件树，仅显示可重试提示。
- **通信**：`services/api.ts` 是唯一 invoke 入口，统一错误处理；组件不得直接 invoke。
- **时钟**：`useNow` 提供全应用唯一的 1Hz 时钟（Context 分发），并监听 `visibilitychange/focus` 处理休眠恢复。倒计时组件全部由父级传入 `now`，自身无定时器。
- **纯逻辑层**：`utils/`（countdown / time / tasks / reminders）为无副作用纯函数，全部有单元测试。
- **主题**：`useTheme` 将设置写入 `<html>` 类名与 CSS 变量；Tailwind v4 用 `@custom-variant dark` 基于类切换。
- **玻璃效果**：Rust 侧 `set_effects(Acrylic)` 启用原生模糊；CSS 变量 `--glass-opacity` 控制面板底色。原生效果不可用时自动降级为半透明/不透明。

## 3. Rust 架构

| 模块 | 职责 |
| --- | --- |
| `main.rs` / `lib.rs` | 入口与组装：单实例插件、其他插件、状态管理、setup、调度线程 |
| `commands.rs` | Tauri 命令层；所有写操作后广播 `data-changed` 并唤醒调度器 |
| `database/` | 连接（WAL/busy_timeout）、版本迁移、CRUD、设置与窗口几何持久化；设置补丁原子化 |
| `models/` | serde 模型（snake_case 与前端一一对应）、输入校验与规范化 |
| `reminders/` | 提醒引擎：领取→统一校验→同步提交→状态回写；快照键去重与崩溃恢复；可注入 `Clock` |
| `notification.rs` | 通知传输适配器；同步取得 OS 提交结果，可注入 fake 用于测试 |
| `windows.rs` | 窗口事件（几何尾部防抖保存、关闭到托盘）、管理窗口创建、效果应用；托盘勾选同步 |
| `tray.rs` | 托盘菜单与行为 |
| `settings.rs` | 开机启动同步（绝不默认开启） |

**状态管理**：`CheckItState { db: Database, wake: Sender<()> }` 由 `app.manage()` 托管；`Database` 内部为 `Arc<Mutex<Connection>>`（parking_lot），天然支持多窗口/线程并发调用。

**托盘锁纪律**：`TrayPinItem` 持有的是 `CheckMenuItem`。Tauri 的 `set_checked()` 会通过
`run_on_main_thread` 投递并同步等待主线程，因此**不得在持有该锁时调用任何菜单 API**。
`sync_tray_pin` 只在锁内克隆菜单项句柄，释放锁后再 `set_checked`；面板设置命令与后台托盘事件
因此不会形成“设置等锁、托盘等主线程”的 ABBA 死锁。其他 Tauri 系统调用也不得在数据库锁内执行。

## 4. 提醒调度设计（可靠性核心）

1. **绝对时间**：`run_once(scan_now)` 每轮对每个待完成任务的每个提醒偏移计算 `due_at - offset`，与扫描快照时间比较。不维护任何增量状态，因此休眠恢复、系统时间调整、进程重启全部自然正确；真正提交前再按最新实时钟复核（见第 8 条）。
2. **领取与去重**：以 `(task_id, offset_seconds, due_at_snapshot)` 唯一键 `INSERT OR IGNORE` 领取事件（state=pending）；改期会自然产生新键，旧动作在提交前失效。
3. **统一有效性校验**：新领取、崩溃恢复、提交前都检查——任务仍 pending、due_at 等于快照、offset 仍在提醒列表中、通知开关仍开启、距计划时间不超过 15 分钟；通知关闭或超宽限置 skipped，任务/offset/快照变化置 cancelled。
4. **同步提交**：`NotificationTransport::submit` 直接返回 OS 受理结果；成功置 submitted，失败置 failed（attempts+1，30s 退避，最多 3 次）。状态中的 `updated_at` 使用**提交完成时刻**，而非扫描开始时刻。
5. **崩溃恢复**：pending 超过 90s 视为崩溃遗留，failed 到达退避时间后重试；恢复同样走有效性校验。失败重试间隔从**实际失败完成时间**起算。语义为 at-least-once（OS 已受理但状态未落库的窗口内可能重复一次），不是 exactly-once。
6. **状态同步**：完成任务、删除任务、修改截止时间或提醒配置都会使旧动作失效；编辑/删除后命令层立即唤醒调度器。
7. **线程模型**：调度线程 `recv_timeout(5s)` 等待，既可被 `check_reminders_now` 立即唤醒，也保证空闲时低频扫描；每轮扫描与数据库恢复共用同一个扫描快照 `now`。
8. **实时钟与二次校验**：`run_once(scan_now)` 仅负责领取/恢复；每个动作在调用系统通知 API 前由
   `deliver_with_clock` 通过可注入 `Clock` 重新读取当前时间，并重做任务状态、due 快照、offset、
   通知开关、15 分钟宽限的完整校验。扫描批次后若发生睡眠、阻塞或前一条通知提交耗时过长，
   后续动作会按最新时间静默跳过或取消，不会使用过期文案或写入过期时间。生产路径使用
   `SystemClock`，测试使用固定/可推进时钟；禁止在引擎内散落不受控的 `Utc::now()`。
9. **单实例（产品策略）**：`tauri-plugin-single-instance` 必须是**第一个注册的插件**。第二实例在
   插件 setup 阶段检测命名互斥量，向已有实例转发参数并退出，早于数据库迁移、托盘和调度器启动；
   已有实例的回调投递到 async 线程唤醒悬浮窗。提醒引擎不具备跨进程原子领取/租约能力，因此
   **不支持多调度器并行**；崩溃恢复的“OS 已受理、DB 未落库”重复边界仍然存在。

## 5. 时间策略

- 存储：SQLite 中全部为 RFC3339 UTC 字符串
- 计算：Rust chrono / JS Date，均按时间戳运算，禁止字符串比较
- 展示：一律转为用户本地时区（`datetime-local` 输入、格式化文本）
- 紧急阈值：前端 `URGENCY_THRESHOLDS`、后端 `MISSED_GRACE_SECS` 集中定义

## 6. 安全

- 全部 SQL 使用参数绑定（含注入测试用例）
- Tauri capabilities 白名单（core:window 最小集 + event/app）
- CSP：生产 `script-src 'self'` 等收紧策略（保留 IPC 与内联样式所需），开发期使用独立 `devCsp` 支持 Vite/React Refresh 与 HMR WebSocket
- 自定义命令仅暴露业务操作，无任意路径/命令执行

## 7. 构建

- 前端：Vite（`tsc --noEmit` 严格检查 → `vite build`）
- 桌面：`tauri build`（NSIS 安装包，currentUser 模式）
- CI：GitHub Actions windows-latest 构建并上传产物

## 8. 误删保护与定期备份

- 最近删除采用独立 task_trash 标记表，正常任务查询统一过滤；删除和恢复与整轮调度共用 maintenance 锁，并通过后台阻塞任务执行，UI 线程不等待该锁。
- 七天保留与备份检查均在 Rust 调度线程运行，窗口隐藏仍工作；每分钟检查维护，自动备份按持久化的 UTC 成功时间控制二十四小时间隔。
- 自动备份沿用 SQLite 在线备份，成功发布完整文件后才轮换自动文件。手动、升级前和恢复前备份保留。设置页轮询只读状态并在卸载时清理计时器。
