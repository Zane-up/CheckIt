# DECISIONS — 重要技术决策记录

格式：决策 / 背景 / 理由 / 后果

## D1. Tauri 2 而非 Electron

- **决策**：按总指令指定使用 Tauri 2。
- **理由**：安装包体积（~5MB vs 100MB+）、内存占用（系统 WebView2 复用）、Rust 生态承载后台调度与 SQLite。
- **后果**：WebView2 依赖系统组件（Win10/11 预装）；CSS 只能面向 Chromium。

## D2. 数据层完全由 Rust 持有

- **决策**：前端不接触 SQLite，全部通过 Tauri 命令 + `data-changed` 事件同步。
- **理由**：单点写路径保证一致性（多窗口）、提醒调度必须共享同一数据视图、便于 Rust 侧测试。
- **后果**：全量 `get_app_state` 在任务量大（数千条）时需增量化；当前规模（个人任务）全量重取 < 1ms 级别，可接受。

## D3. 提醒幂等基于 SQLite 唯一约束而非内存集合

- **决策**：`reminder_events(task_id, offset_seconds)` UNIQUE + `INSERT OR IGNORE`，插入成功才通知。
- **理由**：内存集合在重启后丢失，会导致重复通知；数据库记录天然跨重启持久，且并发扫描也安全。
- **后果**：修改截止时间需显式清除记录（已实现）；该表会缓慢增长（每人每年数百条，可忽略）。

## D4. 15 分钟补发宽限窗口

- **决策**：错过的提醒超 15 分钟只静默记录，不弹通知。
- **理由**：休眠唤醒/重启后如果不加限制会瞬间弹出大量历史通知（洪泛），违背 Non-intrusive 原则；完全不补发又违背 Reliable。
- **后果**：超过 15 分钟的提醒用户可能错过——但任务列表的逾期标记持续可见，且有截止时间兜底。

## D5. Overdue 为派生状态

- **决策**：不把 overdue 写入 status 列，运行时按 `due_at < now` 计算。
- **理由**：落库状态与真实时间必然漂移（休眠、时区、手动改系统时间），派生永远正确。
- **后果**：查询"逾期任务"需在应用层过滤；数据量级下无性能问题。

## D6. 共享 1Hz 时钟而非每任务定时器

- **决策**：`useNow` Context 提供唯一 `now`，所有倒计时组件消费同一值。
- **理由**：几十个 `setInterval` 会造成渲染风暴与定时器泄漏；单一时钟把重渲染收敛到每秒一次。
- **后果**：任一任务变化都会重渲染列表（React reconciliation 成本可控）；列表大时可按行 memo。

## D7. 原生 Acrylic 优先、CSS 降级

- **决策**：`set_effects(Effect::Acrylic)` 失败或用户关闭时降级为半透明/不透明主题。
- **理由**：Acrylic 在部分驱动/远程桌面/旧系统不可用；可读性与稳定性优先于视觉效果。
- **后果**：拖动窗口时 Acrylic 可能出现轻微延迟（系统已知行为）；提供关闭开关。

## D8. 关闭窗口 = 隐藏到托盘（默认）

- **决策**：`close_to_tray` 默认 true，设置中明确说明并可改为"关闭即退出"。
- **理由**：提醒依赖常驻进程；直接退出会静默失效提醒，违背核心价值。
- **后果**：真正退出必须走托盘"退出 CheckIt"或设置切换；退出时 WAL 由连接 Drop 正常收尾。

## D9. 开机启动默认关闭

- **决策**：`launch_at_login` 默认 false，且只有在用户显式开启时才调用 autostart 插件。
- **理由**：总指令明确要求不得未经同意自启。

## D10. 不引入状态管理库

- **决策**：React Context（useAppData/useNow/useTheme）+ 内置 hooks。
- **理由**：数据源单一（后端广播驱动重取），Redux/Zustand 的收益不抵复杂度。
- **后果**：未来若需要细粒度选择器可平滑迁移到 Zustand。

## D11. 时间存储 RFC3339 UTC 字符串

- **决策**：SQLite 存 `2026-10-25T12:00:00.000Z` 形式。
- **理由**：可读、跨语言（chrono/Date 直接解析）、可比较（等长字符串的字典序 = 时间序，仅用于索引，业务逻辑仍用时间戳）。
- **后果**：展示层必须转本地时区（已封装 `utcIsoToLocalInput` / `formatDueLocal` 并测试）。

## D12. TypeScript strict + 无 any

- **决策**：`strict: true`、`noUnusedLocals/Parameters`，Rust 模型与 TS 类型一一对应。
- **理由**：前端与 Rust 边界是主要出错面；类型即文档。

## D14. 提醒状态机与快照去重键（v0.1.1 修复，v3 语义精确化）

- **决策**：reminder_events 升级为 (task_id, offset, due_at_snapshot) 唯一键 + state 状态机
  （pending/submitted/failed/skipped/cancelled），先领取（pending）后提交，成功置 submitted、失败置 failed 可重试。
- **理由**：
  - 旧设计"先记 fired_at 再发送"在崩溃/发送失败时**永久漏发**；状态机让崩溃遗留的 pending 可恢复（at-least-once）
  - 旧键 (task_id, offset) 在改期后互相抑制且产生"旧扫描快照误发"竞态；快照键使改期自然产生新键，
    提交前二次校验（任务仍 pending、due_at == snapshot、offset 仍在、通知开启、未超宽限）令旧动作失效
- **v3 迁移**：旧 state='sent' 统一改名为 'submitted'（重建表以更新 CHECK），逐行保留 id/attempts/时间戳，
  不删除任何事件记录。
- **诚实的语义边界**：submitted = 系统通知 API 同步返回成功（OS 受理），**不等于用户看到横幅**；
  at-least-once 可能极端情况下重复通知，不承诺 exactly-once。

## D15. 托盘/菜单回调不做同步窗口创建

- **决策**：托盘菜单与单击回调中的窗口创建/显示统一投递到 async 线程执行。
- **理由**：菜单事件在主线程派发；主线程同步 `.build()` 会阻塞 WebView2 controller 初始化（死锁），
  与已修复的 sync command 问题同构。

## D16. 编辑表单未改截止时间时传回原始 UTC

- **决策**：表单记录进入时的 datetime-local 快照；保存时输入未变则原样传回任务原始 UTC 值。
- **理由**：datetime-local 只有分钟精度，往返转换丢秒；夏令时重复时段（fall-back）本地时间有歧义，
  未修改就不应引入偏移。用户真正修改时按本地时区重新解析（歧义时段取第一个，平台已知限制）。

## D17. CSP 收紧

- **决策**：生产策略 `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self' ipc: http://ipc.localhost`；
  另设 `devCsp`，仅在开发服务器下放开 `script-src 'unsafe-inline'` 与 `ws://localhost:1420` 等 HMR 所需来源。
- **理由**：生产包默认同源，脚本不引入外部代码；React/Tailwind 的内联 style 属性需要 style 的 unsafe-inline；
  Tauri IPC 在 Windows 上走 http://ipc.localhost。此前为 null（继承默认放开），属加固而非修复已知漏洞。
  Vite/React Refresh 依赖内联脚本与 WebSocket，若开发期沿用生产 CSP 会白屏或无法热更新；devCsp 不进入生产产物。

## D13. 图标由脚本生成

- **决策**：`scripts/generate-icon.mjs` 纯 Node（zlib）生成 1024×1024 源图，`npx tauri icon` 产出全平台图标。
- **理由**：无设计工具依赖，可复现、可版本化。

## D18. 通知发送走同步适配器，不使用插件 show() 的返回值

- **决策**：新增 `notification::SystemNotificationTransport`，直接调用 notify-rust（与插件共用底层 WinRT 实现）
  取得同步提交结果；`Ok` 置 submitted，`Err` 置 failed 并按 30s 退避重试，最多 3 次。
- **理由**：tauri-plugin-notification 2.7.0 的 Windows 实现把底层发送丢进 `spawn` 并丢弃错误，
  `show() == Ok` 只代表已准备/入队。若不改，发送失败会被记为成功而永久漏发。
- **语义边界**：仅区分「插件入队 / OS 受理（submitted）/ 用户看到横幅」中的第二层；
  第三层无法由应用 API 观测。AUMID 注册、专注助手、通知权限仍需实机验证。

## D19. 设置补丁在单个 DB 事务内完成

- **决策**：`update_settings_atomic` 在一次连接锁 + 一次事务中读取、应用 patch、sanitize、写回；
  设置面板与托盘置顶都调用同一路径。
- **理由**：此前的「读取→修改→整体保存」在并发下会丢失其他字段的修改；读锁重入还可能死锁。
  窗口/系统调用放在事务提交之后，失败只记录日志；置顶切换在窗口调用失败时回滚数据库并通知前端。

## D20. 只持久化展开态尺寸，收起状态不落库

- **决策**：窗口几何按物理像素保存；收起（高度 ≤ 72px）时不覆盖已保存的展开高度，恢复时一律以展开态启动；
  旧版本保存的 64px 收起高度映射为默认 440px。
- **理由**：若把收起尺寸当作持久化尺寸，重启后会得到被裁剪的 64px 窗口；独立保存展开/收起两份尺寸增加状态复杂度。
  几何保存采用 500ms 尾部防抖，并在移动/缩放结束与关闭窗口时强制落盘，避免丢失最后一次调整。

## D21. 托盘菜单项在锁外调用 Tauri API

- **决策**：`sync_tray_pin` 在 `TrayPinItem` 锁内只克隆 `CheckMenuItem`；释放锁后才调用 `set_checked`。
- **理由**：Tauri 2 的 `CheckMenuItem::set_checked` 经 `run_on_main_thread` 投递并**同步等待主线程**。
  若后台托盘事件持有锁等待主线程，而主线程的设置命令又等待同一把锁，会形成 ABBA 死锁。
  锁内克隆菜单项句柄是廉价操作，系统调用与锁解耦后不存在该等待环。
- **后果**：必须保持“任何 Tauri 菜单/窗口 API 都不在 `TrayPinItem` 或数据库锁内调用”的纪律；
  回归测试 `tray_pin_item_is_cloned_before_calling_menu_api` 用模拟主线程 `try_lock` 验证锁已释放。
  真实托盘菜单线程与设置命令的并发仍需实机复测（自动化不覆盖 Win32 菜单线程时序）。

## D22. 扫描快照领取，提交前实时取时，完成时间落库

- **决策**：`run_once(scan_now)` 用一个扫描快照完成领取/恢复；每个动作在系统通知调用前通过
  可注入 `Clock` 重新取时并重做全部有效性/宽限校验；submitted/failed 的 `updated_at`
  使用提交**完成**时间，失败重试的 30s 退避从该完成时间起算。
- **理由**：批次扫描中若发生睡眠、调度阻塞或单条通知提交耗时，沿用扫描开始时间会把已超宽限的
  提醒误发、使用过期文案，或让失败重试提前。实时二次取时可关闭该窗口，同时保留可注入时钟便于确定性测试。
- **后果**：状态时间戳反映真实完成时刻；不承诺 exactly-once——**已开始的系统通知调用无法撤回**，
  OS 已受理而 DB 未落库时崩溃仍可能重复提交一次。已通过固定/可推进时钟覆盖扫描过期、
  首条提交推进时钟、失败时间戳与重试基准。

## D23. 选择单实例策略（A），不实现跨进程恢复租约

- **决策**：使用成熟的 `tauri-plugin-single-instance`，并将它注册为第一个插件；第二实例在插件
  setup 阶段转发唤醒请求后退出，不进入数据库迁移、托盘与调度器启动。回调投递到 async 线程
  唤醒已有悬浮窗。
- **理由**：CheckIt 是个人桌面单用户应用，单实例足以满足使用场景；实现原子恢复领取还需要
  owner/lease/条件更新与锁外系统调用，复杂度和故障面显著增加。插件在应用 setup 之前拦截，
  避免两个进程并发迁移与并发调度。
- **后果**：**提醒引擎明确不支持多调度器并行**；若绕过单实例保护启动第二个引擎，崩溃恢复的
  `recoverable_reminders` 可能被两处同时领取并重复提交。单实例不消除崩溃窗口内的重复提交边界。
  dev 隔离 identifier 已验证第二实例退出与隐藏窗口唤醒；安装包形态仍需实机复验。
