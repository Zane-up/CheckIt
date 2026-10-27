# TESTING — 测试指南

当前基线：Rust 111 个测试、前端 97 个测试（10 个文件），均已针对最终源码复跑通过。
全部自动化测试使用内存库或临时目录中的文件库，不读取、不写入用户真实数据库，
不触碰真实系统自启动设置与真实任务。

> 本轮（2026-10-27 晚，贴边收纳球）状态速查：
> **已实现**（代码完成）→ **自动测试通过**（Rust 111 / 前端 97 / typecheck / 生产构建）→
> **原生实机拖动未验证**（本机自动化会话无法向窗口注入鼠标拖动，见文末「贴边收纳球：验证状态」）。
> 未取得原生窗口截图证据时，不得写成「实机通过」。

验收状态分为四类，文档中严格区分：
- **自动测试通过**：`npm run typecheck` / `npm run test` / `cargo test` / `cargo check` / `npm run build`
- **安装版实机验证通过**：用本轮重建的 NSIS 安装包在正常交互登录账户下实际操作并留下 OS 级证据
- **失败**：有证据表明未达到预期
- **待验证**：尚未取得可复现证据，不得勾选

## 测试矩阵与验收映射

| # | 验收场景 | 测试 | 位置 |
| --- | --- | --- | --- |
| 1 | 新建任务 | `create_and_read_back_task` | src-tauri `database/tests.rs` |
| 2 | 编辑任务 | `update_task_reflects_changes` | 同上 |
| 3 | 删除任务 | `delete_task_removes_reminders` | 同上 |
| 4 | 完成任务 | `complete_and_restore` | 同上 |
| 5 | 恢复任务 | `complete_and_restore` / `restore_pending_rearms_future_reminders` | 同上 / reminders |
| 6 | 分类筛选 | `applyTaskFilter` 分类/优先级/关键词 | src `tasks.test.ts` |
| 7 | 截止时间排序 | `sortTasks` 升序/沉底/优先级 | 同上 |
| 8 | 跨天倒计时 | 跨月/跨年/闰年用例 | src `countdown.test.ts` |
| 9 | 已过期任务 | overdue 文本与紧急度 | 同上 |
| 10 | 多级提醒计算 | `multiple_offsets_fire_independently` | src-tauri reminders |
| 11 | 提醒去重（含跨重启持久） | `idempotent_no_double_fire` / `dedup_survives_process_restart_on_file_db` | 同上 |
| 12 | 修改截止时间重算（快照键） | `due_change_recomputes_with_snapshot_keys` | 同上 |
| 12b | 改期/完成/删除使旧动作失效 | `task_change_reschedule_complete_delete_invalidates_action` / `completed_task_stops_reminders` | 同上 |
| 12c | 已领取后提醒被移除 → 不发送 | `removed_offset_invalidates_claimed_action` / `removed_offset_is_cancelled_during_recovery` | 同上 |
| 13 | 完成后停止提醒 | `completed_task_stops_reminders` | 同上 |
| 13b | 底层提交失败 → failed 并进入退避重试 | `notification_submission_failure_enters_retry_with_backoff` | 同上 |
| 13c | 重试至上限后停止 | `failed_retry_stops_at_max_attempts` | 同上 |
| 13d | failed 状态跨重启保留 | `failed_state_survives_restart` | 同上 |
| 13e | 崩溃遗留 pending 恢复（90s 边界） | `crashed_pending_recovers_at_stale_boundary` | 同上 |
| 13f | 提交前通知已关闭 → 不发送 | `deliver_skips_when_notifications_turned_off_after_claim` / `notifications_disabled_after_failure_is_silent` | 同上 |
| 13g | 通知关闭时领取 → 静默 skipped | `notifications_disabled_claims_as_skipped` | 同上 |
| 14 | 重启数据恢复（连接真正关闭） | `restart_recovers_data_from_file_db`（drop 后重开） | database/tests.rs |
| 15 | 补发宽限边界（恰好 15min / 超 1s） | `grace_boundary_exact` / `missed_within_grace_still_notifies` / `missed_reminder_beyond_grace_is_silent` | reminders |
| 15b | 失败/崩溃恢复超宽限 → 静默 | `failed_beyond_grace_is_silent` / `crashed_pending_beyond_grace_is_skipped` / `deliver_beyond_grace_is_skipped` | 同上 |
| 16 | 窗口设置持久化 | `window_geometry_roundtrip` / `settings_roundtrip` | database/tests.rs |
| 17 | 事务回滚（创建/更新中途失败） | `create_task_rolls_back_on_midway_failure` / `update_task_rolls_back_preserving_old_state` | database/tests.rs |
| 18 | v1→v3 数据库迁移 | `migrates_v1_database_to_current_preserving_data` | 同上 |
| 18b | v2→v3（sent→submitted，数据不丢） | `migrates_v2_events_to_v3_submitted_without_data_loss` | 同上 |
| 19 | 日历日界（显式 TZ，不按宿主跳过） | 上海/UTC/纽约春 23h/秋 25h 日界 | src `time.test.ts` |
| 20 | 深夜预设不跨错日 | `calendarTodayAt/TomorrowAt/NextSaturdayAt` | 同上 |
| 21 | 通知文案三阶段 | `notification_body_three_phases` | src-tauri reminders |
| 22 | 通知正文确实包含任务内容 | `submitted_notification_content_contains_body` | 同上 |
| 23 | 开发/安装态 app_id 判定 | `dev_binary_detection` | src-tauri `notification.rs` |
| 24 | 设置补丁不丢未触及字段 | `settings_patch_preserves_untouched_fields` | database/tests.rs |
| 25 | 设置补丁与置顶并发交错 | `concurrent_settings_patch_and_pin_keep_all_fields`（2 线程 × 100 次） | 同上 |
| 26 | 收起尺寸不覆盖展开尺寸 | `collapsed_height_keeps_last_expanded_height` / `collapsed_without_expanded_record_falls_back_to_default` / `expanded_height_is_saved_verbatim` | src-tauri `windows.rs` |
| 27 | 旧版 64px 收起高度恢复为展开 | `legacy_collapsed_geometry_restores_expanded` / `restored_height_clamps_into_expanded_range` | 同上 |
| 28 | 订阅就绪前不读快照；事件不丢 | `订阅就绪前不读取快照；订阅成功后立即读取` / `快照请求期间收到事件 → 重取最新；旧慢响应被丢弃` | src `useAppData.test.tsx` |
| 29 | 卸载注销订阅；订阅失败降级并可重试 | `卸载时注销 data-changed 订阅` / `订阅注册失败仍可加载数据，并显示降级提示；重试后恢复订阅` | 同上 |
| 30 | 已加载后刷新失败保留旧数据 | `已加载后刷新失败：保留旧数据并记录错误` / `已有数据后刷新失败：保留组件树与未保存草稿，仅显示可重试提示` | 同上 / `DataGate.test.tsx` |
| 31 | 首屏失败才用错误页；重试成功恢复 | `首屏加载失败：显示错误页与重试；重试成功后渲染内容` | `DataGate.test.tsx` |
| 32 | 时钟 tick 不重注册快速添加监听 | `时钟 tick 重渲染不重新获取 Window、也不重注册 widget-quick-add` | `WidgetApp.test.tsx` |
| 33 | 收起后托盘快速添加恢复展开尺寸 | `收起后托盘快速添加：恢复展开高度并显示面板` | 同上 |
| 34 | 设置响应乱序只应用最新 | `乱序响应只应用最后一次请求的结果` | `useAppData.test.tsx` |
| 35 | 托盘勾选同步不在锁内等待主线程 | `tray_pin_item_is_cloned_before_calling_menu_api`（模拟主线程 + `try_lock`） | src-tauri `windows.rs` |
| 36 | 扫描快照过期/阻塞后提交前重校验，超宽限不发送 | `stale_scan_time_beyond_grace_is_skipped_before_submit` / `later_actions_use_updated_clock_after_first_submit_blocks` | src-tauri reminders |
| 37 | failed 时间戳使用实际提交失败完成时间 | `failed_status_timestamp_uses_completion_time` | 同上 |
| 38 | 重试退避从实际失败完成时间起算 | `retry_backoff_starts_from_actual_failure_time` | 同上 |
| 39 | 单实例：第二实例退出并唤醒已有隐藏窗口 | `安装版实机验证通过`（`com.checkit.desktop`，见本轮记录）；dev 隔离 identifier 亦通过 | `lib.rs` / 手动 |
| 40 | 诊断日志保留底层通知 HRESULT | `notification_error_with_hresult_is_persisted` | src-tauri `diagnostics.rs` |
| 41 | 运行期日志轮转（不重新 init 也轮转） | `rotates_while_running_without_reinitializing` | 同上 |
| 42 | 活动文件 + 归档总量有界 | `archive_count_and_size_are_bounded` | 同上 |
| 43 | 并发写入不破坏轮转与行完整性 | `concurrent_writes_do_not_corrupt_rotation` | 同上 |
| 44 | 轮转失败（文件禁止改名/删除）时持续写入仍不超限 | `rotation_failure_keeps_real_size_and_recovers`（Windows 共享模式占位） | src-tauri `diagnostics.rs` |
| 45 | 解除占用后轮转恢复，总量仍有界 | 同上（drop 句柄后再写入 30 条，检查 `.1` 出现且总量 ≤ 128×3） | 同上 |
| 46 | 超长中文/英文记录截断后不超限且 UTF-8 有效；极小上限仍守约 | `oversized_utf8_records_stay_within_limit` / `very_small_limit_is_still_respected`（上限 1/8/15 字节） | 同上 |
| 47 | 自定义提醒解析校验：空值/非数字/小数/负数/超大数值/0 就地提示且保留输入；60 天边界（60 天通过、61 天拒绝） | ReminderPicker 输入校验用例 | src `ReminderPicker.test.tsx` |
| 48 | 添加 25 分钟保存为 1500 秒；添加成功后输入清空 | 添加用例 | 同上 |
| 49 | 去重：60 分钟与 1 小时视为相同；自定义与预设重复不生成重复标签（显示预设选中） | 去重用例（onChange 不触发） | 同上 |
| 50 | Enter 仅添加提醒并过滤中文输入法组合态（keyCode 229） | Enter/IME 用例 | 同上 |
| 51 | 非预设提醒回显为标签（含移除按钮）；删除一个其余保留 | 回显 / 移除用例 | 同上 |
| 52 | 草稿状态上报（onDraftChange）；禁用时清空草稿 | 草稿 / 禁用用例 | 同上 |
| 53 | 编辑已有非预设提醒正常回显；未修改保存不丢值 | 编辑回显用例 | `TaskFormModal.test.tsx` |
| 54 | 草稿未添加时阻止保存并提示；Enter 不误触发任务保存 | 草稿阻止 / Enter 用例 | 同上 |
| 55 | 未设置截止时间时禁用提醒添加并说明原因 | 禁用用例 | 同上 |
| 56 | 新建任务继承默认提醒；移除一个保存后其余保留 | 继承 / 移除用例 | 同上 |
| 57 | 默认提醒设置：回显自定义值、添加、移除、非法输入不保存 | 设置页用例 | `SettingsPanel.test.tsx` |
| 58 | 自定义提醒秒值文件库往返（保存 → 重开一致，含非预设 1500） | `custom_reminder_offsets_roundtrip_file_db` | src-tauri `database/tests.rs` |
| 59 | 自定义默认提醒补丁规范化（去重 / 越界滤除）并跨重开保留 | `custom_default_reminder_offsets_persist_across_reopen` | 同上 |
| 60 | 60 天闭边界：恰好 60 天保留，超出 1 秒滤除 | `normalize_boundary_sixty_days` | src-tauri `models/task.rs` |
| 61 | 快速添加三行布局：日期输入与添加按钮不重叠、不被裁切（340/300 宽，深/浅主题） | 真实浏览器渲染 + 计算样式/几何断言 + 截图（`docs/screenshots/quickadd-layout-2026-10-27/`） | 手动（CDP） |
| 62 | 紧凑输入左内边距真实生效（组件类优先于分层工具类） | 同上：`paddingLeft` 实测 8px；`@layer utilities` 对照组 10px | 手动（CDP） |
| 63 | 正常高度或收起态打开快速添加：临时抬高，关闭后恢复原展开高度（不写入 320） | `展开高度低于快速添加所需时：临时抬高到 320，关闭面板后恢复原高度` / `收起态打开快速添加：关闭后恢复到正常展开高度而不是收起高度` | `WidgetApp.test.tsx` |
| 64 | 快速添加 Enter 提交、快捷日期填入、Tab 焦点顺序 | 浏览器真实键盘/点击（见「快速添加布局修复验收」） | 手动（CDP） |
| 65 | 贴边阈值与边选择：四边各自吸附，角落取最近边，阈值用 `<=` 稳定 | `select_edge_detects_each_edge` / `nearest_edge_picks_closest` / `corner_picks_nearest_edge` | src-tauri `dock.rs` |
| 66 | 圆圈锚点四边均完整落在工作区内 | `dock_anchor_places_bubble_on_each_edge` / `dock_anchor_clamps_into_work_area` | 同上 |
| 67 | 从圆圈恢复到面板：按原边向内展开并完整可见 | `expand_position_clamps_near_taskbar` | 同上 |
| 68 | 拖动结束识别：暂停不误判、跨屏 DPI 缩放不误判为拖动 | `drag_move_distance` 相关用例 `cross_screen_dpi_scale_does_not_look_like_drag` | src-tauri `windows.rs` tests |
| 69 | 单击与拖动区分：圆圈上 ≤4 逻辑像素抖动算单击 | `click_on_bubble_is_not_a_drag` / `drag_away_from_edge_exceeds_click_threshold` | 同上 |
| 70 | 缩放会话不触发吸附 | `resize_session_never_docks` | src-tauri `dock.rs` |
| 71 | 圆圈计数格式：0 / 1–99 / 99+ | `bubble_count_formatting` | 同上 |
| 72 | 圆圈显示全部 pending（不受显示条数与筛选限制） | `dock-bubble-count` 计数用例 | src `WidgetApp.test.tsx` |
| 73 | 模式转换：expanded ↔ collapsed ↔ docked 的状态与几何互不覆盖 | `dock_mode_does_not_touch_expanded_geometry` / `docked_restart_keeps_expanded_geometry` | src-tauri `database/tests.rs` |
| 74 | 临时高度（320 / 64 / 40）不写入正常展开几何 | `temp_quick_add_height_never_overwrites_expanded_height` / `should_persist_expanded` 单测 | 同上 / `dock.rs` |
| 75 | 旧数据缺少 `widget_mode` 时按 expanded 处理；新增设置缺键时默认开启 | `missing_mode_key_falls_back_to_expanded` / `edge_dock_setting_defaults_on_and_patch_persists` | src-tauri `database/tests.rs` |
| 76 | 圆圈模式下托盘快速添加：先恢复面板再请求 320 | `圆圈模式托盘快速添加：先恢复完整面板，再请求临时 320` | src `WidgetApp.test.tsx` |
| 77 | 从圆圈展开清空快速添加标记（不残留窄高度面板） | `从圆圈展开会清空快速添加标记（不残留窄高度面板）` | 同上 |
| 78 | 保存坐标落在当前显示器之外：钳制回工作区且尺寸不变（不退化到最小尺寸） | `offscreen_positive_x_falls_back_inside_work_area` / `clamp_keeps_rect_intact_when_only_position_is_offscreen` | src-tauri `dock.rs` |
| 79 | 圆圈的 0 / 9 / 99 / 99+ 在深浅主题下不裁切 | `data-len` 字号自适应 + `.dock-bubble-count` 样式 | src `globals.css` / `DockBubble` |
| 80 | **原生窗口实机拖动全链路**（收纳 / 计数 / 单击恢复 / 拖离恢复 / 重启保持） | **未验证**——本机自动化会话无法注入窗口拖动且无法截取 WebView2 画面，见「贴边收纳球：验证状态」 | 人工 |

## 命令与执行顺序

```bash
# 1. 前端类型检查
npm run typecheck          # tsc --noEmit（strict）

# 2. 前端测试（jsdom + Testing Library）
npm run test               # vitest run

# 3. Rust 测试与编译检查（在 src-tauri 下）
cargo test
cargo check

# 4. 前端生产构建
npm run build              # 类型检查 + vite build

# 5. Windows NSIS 安装包
npm run tauri build
```

## 本轮执行记录（2026-10-27，自定义提醒提前量功能）

### 功能摘要

任务新建/编辑与设置页「默认提醒」统一改用共享 `ReminderPicker` 组件
（`src/components/ui/ReminderPicker.tsx`），`src/utils/reminders.ts` 新增单位换算与
`parseReminderAmount` 校验；存储继续使用「提前秒数」，无数据库 schema 变更、
无提醒调度器行为变更（`normalize_reminder_offsets` 原有 0~60 天语义即自定义值的持久化边界）。

- 保留原预设（提前 1 天 / 3 小时 / 1 小时 / 30 分钟 / 10 分钟 / 截止时），新增
  `提前 [数值输入] [分钟/小时/天] [添加]` 行（默认单位分钟，可添加多个自定义提前量）；
- 校验：仅接受正整数；0 引导改用「截止时」；按所选单位换算为秒后检查安全整数与 60 天上限；
  失败就地提示并保留输入，不静默过滤；
- 去重按换算后的秒数：60 分钟与 1 小时视为相同；与预设重复时只显示预设选中，不生成重复标签；
- 标签按提前量降序稳定排列，选中项带「移除」按钮；编辑任务时非预设提醒原样回显，
  秒数不误显示为分钟；
- 未设置截止时间时输入区禁用并说明原因；Enter 只添加提醒（过滤中文输入法组合态），
  不触发任务保存；存在未添加草稿时保存被阻止并提示先添加或清空；
- 设置页修改默认提醒只影响后续新建任务，不回写已有任务。

### 自动测试（全部通过）

- TypeScript 严格检查：`npm run typecheck` 通过
- 前端测试：`npm run test` **9 文件 / 80 用例通过**（新增 21 项：
  ReminderPicker 11、TaskFormModal 6、SettingsPanel 4，见矩阵第 47–57 行）
- Rust 测试：`cargo test` **73 用例通过**（新增 3 项，见矩阵第 58–60 行）；
  `cargo check --all-targets` 通过
- 前端生产构建：`npm run build` 通过
- `npm run tauri build`：成功产出 NSIS 安装包（00:59:24 一次、09:00:44 重建一次，后者为当前验收对象）

### 安装包元数据（2026-10-27 09:00:44 重建；已被 12:28:04 布局修复包超越）

> 本节的包是当时（09:00 验收）的对象；最新包元数据见下一节。

| 项 | 值 |
| --- | --- |
| 路径 | `src-tauri/target/release/bundle/nsis/CheckIt_0.1.0_x64-setup.exe` |
| 构建时间 | 2026-10-27 09:00:44（晚于本轮全部源码修改） |
| 大小 | 2,015,054 字节（约 1.92 MiB） |
| SHA256 | `AF5AE97701E0BE44E5548434C626A643A946D841CA913C04CF8415E9F263C697` |
| 可执行文件（构建产物） | `src-tauri/target/release/checkit.exe` 5,023,232 字节，SHA256 `DF8D2A0CFF5C77AAE6D7B3CF8EC4B3B9877881D5D796FAC2DF79C5276D7BBE96` |
| 可执行文件（安装目录） | `%LOCALAPPDATA%\CheckIt\checkit.exe` 5,023,232 字节，SHA256 `212B1F79D024630A3B8879E7FEF83EC195992CC321205CB63C362886DF81B893` |

安装目录 exe 与构建产物逐字节比较仅 3 字节差异（bundle 类型标记 `UNK`→`NSS`，
NSIS 安装版重写所致），定性为同一构建的预期现象，功能代码一致。
00:59:24 旧包（2,015,910 字节 / `7A8B498B…038DA3`）被本包超越，未作为验收对象。
相对已实机验收 15:08 包的差异：`diagnostics.rs` 两处日志边界修复 + 全应用文字可读性调整 +
主按钮对比度令牌修复 + 自定义提醒提前量功能（无数据库 schema 变更、无提醒逻辑变更、无依赖升级）。
本包已于 2026-10-27 09:00–09:04 完成安装并启动，实机验收记录见下节。

### 安装包元数据（快速添加布局修复最终包，2026-10-27 12:45:29，已安装）

| 项 | 值 |
| --- | --- |
| 路径 | `src-tauri/target/release/bundle/nsis/CheckIt_0.1.0_x64-setup.exe` |
| 构建时间 | 2026-10-27 12:45:29（晚于本轮全部源码修改，含收起态高度回归） |
| 大小 | 2,015,665 字节 |
| SHA256 | `AE5F8468EB408A7D96EED7C9A1916EFEB4FF22E1F7F13B0A978C91EBA1D6F8C9` |
| 可执行文件（构建产物） | `src-tauri/target/release/checkit.exe` 5,023,232 字节，SHA256 `7CB22E0D170E7736AE3B690D2B9E8EBFCF03BE51AF1D5FA949731EF90D1D182D` |
| 可执行文件（安装目录） | `%LOCALAPPDATA%\CheckIt\checkit.exe` 5,023,232 字节，SHA256 `B3B245359F1B942240909FD9B3768EE605552D275C097B00C20E0A0228369412` |

安装目录 exe 与构建产物逐字节比较仍为 **3 字节差异**（偏移 4594591–4594593，
bundle 类型标记 `UNK`→`NSS`），与上一包一致，属同一构建的预期现象。
本包已于 2026-10-27 12:48 静默安装（安装器 ExitCode 0）并按原生 WebView2 验收通过
（见「原生 WebView2 实机验收」）。12:28:04 的中间包（2,017,149 字节 / `E13B5F95…EB784`）
与 09:00:44 包均被本包超越；**旧包结论不得直接套到本包**，本轮已对本包复测布局、
最小宽度、展开态与收起态高度闭环。

### 安装版实机验收（2026-10-27 09:00 包，正常交互登录账户）

安装与启动：NSIS 安装成功，进程于 09:04:36 从 `%LOCALAPPDATA%\CheckIt\checkit.exe` 启动；
启动日志确认数据库迁移 v1→v2→v3 全部应用，schema 为 v3；数据目录
`%APPDATA%\com.checkit.desktop\`（checkit.db + -wal + -shm）。

| # | 验收项 | 结果 | 证据 |
| --- | --- | --- | --- |
| A1 | 任务创建（完整表单） | **通过** | UIA 驱动新建弹窗：标题「验收-自定义提醒-25分钟」+ 截止 2026-10-28 10:00；列表出现且倒计时「23小时 50分钟」正确 |
| A2 | 任务数据落库 | **通过** | 数据库副本（三件套复制，integrity ok）：due_at `2026-10-28T02:00:00.000Z`（UTC，即本地 10:00） |
| A3 | 自定义提醒保存 | **通过** | 自定义「25 分钟」经输入校验、去重后保存；数据库 reminders 表两行：`86400`（提前 1 天）+ `1500`（提前 25 分钟），秒值正确 |
| A4 | 自定义提醒回显 | **通过** | 保存后重开编辑弹窗：「移除 提前 1 天」「移除 提前 25 分钟」选中态（带移除按钮）存在；未选项「提前 3 小时」「提前 30 分钟」「截止时」均为未选中按钮 |
| A5 | 默认提醒继承语义 | **通过** | 无截止时间保存任务时提醒集合清空（默认继承仅新建弹窗初始化时生效）；符合「设置页默认提醒只影响后续新建任务」设计，非缺陷 |
| A6 | 提醒实际触发与通知提交 | **通过** | 「提前 1 天」事件到期点（本地 10:00）在 15 分钟补发宽限内于 10:08:49 被补发；reminder_events 行 `state='submitted'`（创建到提交完成 37ms）；`wpndatabase.db` 同秒（10:08:50）登记；诊断日志 checkit.log 无任何错误记录 |
| A7 | 单实例与唤醒 | **通过** | 运行中再次启动 checkit.exe：第二实例在完成初始化前退出（日志「检测到第二实例启动」），进程数不变，已有实例管理窗口被唤醒显示 |
| A8 | 托盘菜单正常退出 | **通过** | 托盘菜单「退出 CheckIt」→ 进程正常结束（日志无异常、无崩溃残留） |
| A9 | 正常退出重启去重 | **通过** | 托盘退出后重启：reminder_events 无新增 submitted 行（仍为 1 行），submitted 幂等去重生效 |
| A10 | 验收记录清理 | **通过** | 应用退出后于数据库副本删除自建验收任务（1 任务 + 2 提醒 + 1 事件），三件套回写，integrity ok，用户数据无损失 |

数据库备份方法说明：运行中的应用持有 WAL 文件锁，Python sqlite3 `backup()` API 与
`mode=ro` 只读连接均报 disk I/O error；采用 **db + wal + shm 三件套文件复制**后再打开副本，
integrity ok，可安全用于验收期间的只读取证与本次清理回写（回写时应用已退出，
wal 经 `wal_checkpoint(TRUNCATE)` 合并后整体替换三件套，替换后再次校验 integrity ok）。

本轮**未完成独立实机验证**的项（如实保留，不得勾选）：

- 通知横幅**目视可见性**（第三层证据；本轮已取得前两层：submitted + wpndatabase 登记）
- 关闭到托盘后的后台提醒独立实机复测（管理窗口关闭后 UIA 无法重新枚举、托盘图标对本轮
  UIA 枚举不可见，无法继续自动化；支撑证据：调度器线程独立于窗口显示状态的架构与
  cargo test 全覆盖 + 上轮 15:08 包同代码路径实机记录 + 本轮 A6 证明调度器在实机上正常触发）
- 任务完成/恢复的 UI 层操作（自动测试矩阵 #4/#5 全绿，逻辑层已覆盖）
- 设置页「默认提醒」UI 独立操作（SettingsPanel 4 项自动测试全绿；本轮通过 A5 间接验证继承语义）
- 真实睡眠/注销/系统重启唤醒、开机自启、旧包覆盖升级、原生 Acrylic、混合 DPI、
  长时间驻留资源占用（同 00:59 包待验证清单，未执行）

### 自定义提醒界面截图（浏览器预览环境）

> **环境声明**：与上文可读性验收相同，为 Vite 预览 + Chrome headless CDP 真实渲染，
> 使用替身 api/Tauri 模块与固定样例数据，**不是 Windows 原生安装版截图**。

| 截图 | 内容 |
| --- | --- |
| `docs/screenshots/custom-reminder-task-form-dark.png`（960×700） | 任务编辑弹窗深色：已选「提前 2 天 / 1 小时 / 25 分钟」标签（带移除按钮）+ 自定义输入行 |
| `docs/screenshots/custom-reminder-task-form-light.png`（960×700） | 任务编辑弹窗浅色：同上 |
| `docs/screenshots/custom-reminder-settings-dark.png`（960×860） | 设置页深色：「新任务默认提醒」hint 文案 + 预设标签 + 自定义输入行 |
| `docs/screenshots/custom-reminder-settings-light.png`（960×860） | 设置页浅色：同上 |

四张截图已人工核对：标签文案与单位正确、移除按钮可见、输入行完整（设置页用 860px 高度
避免输入行被截断）、深浅主题样式正常。

## 本轮执行记录（2026-10-27 下午，快速添加布局修复）

### 问题与根因

安装版悬浮窗「快速添加任务」在 340px 默认宽度与 300px 最小宽度下，日期行控件互相重叠。
按最终计算样式核对（不是只看 className）：

| 检查 | 结果 |
| --- | --- |
| `.field-input` 所在级联层 | 未分层（`globals.css` 直接定义） |
| `@layer utilities` 中的 `pl-*` 是否生效 | **否**。构造 `@layer utilities { .probe-pl7 { padding-left: 1.75rem } }` + `class="field-input probe-pl7"`，`getComputedStyle().paddingLeft` 实测 **10px**，即被未分层的 `padding: 7px 10px` 覆盖 |
| 旧版日期输入左内边距 | 10px；13px 图标位于 `left: 8px`（占 8–21px），文字从 10–11px 起排 → 必然重叠 |
| 旧版弹性宽度 | 「今天/明天/日期/添加」同一行；340px 下日期输入约 150px，300px 下更窄，原生日期文本与选择器按钮互相挤压 |

### 修复

- 三行布局：任务名称 + 关闭 / 「今天」「明天」/ 日期时间 + 添加
- 移除 `CalendarClock` 装饰图标（与原生日期选择器重复，保留原生入口）
- 新增 `.field-input-compact` 组件类（同层后声明，`padding: 3px 8px; font-size: 12px`），不使用 `!important`
- 日期输入外层 `min-w-0 flex-1`，添加按钮 `shrink-0`
- 打开快速添加时窗口高度下限 320 逻辑像素；正常展开高度不足时临时抬高，
  关闭面板后恢复原高度，不把临时尺寸写入正常展开高度

### 自动测试（全部通过）

- `npm run typecheck` 通过
- `npm run test`：9 文件 / **82 用例**通过（新增 2 项窗口高度回归：展开态与收起态入口）
- `cargo test`：**73 用例**通过
- `npm run build` 通过

### 浏览器真实渲染验收（Edge headless + CDP，真实 Chromium 布局）

临时预览壳 stub Tauri internals 后渲染真实 `WidgetApp`（预览文件验收后删除，不入库）。
下表的宽度、内边距与边界均为 `getComputedStyle` / `getBoundingClientRect` 实测值：

| 场景 | 日期输入宽 | 左内边距 | 日期右边界 / 添加按钮左边界 | 面板底 / 底栏顶 | 文本裁切 |
| --- | --- | --- | --- | --- | --- |
| 340×440 深色 | 240px | 8px | 264 / 272（不相交） | 240.5 / 399 | 否 |
| 300×440 深色 | 200px | 8px | 224 / 232 | 240.5 / 399 | 否 |
| 340×440 浅色 | 240px | 8px | 264 / 272 | 240.5 / 399 | 否 |
| 300×440 浅色 | 200px | 8px | 224 / 232 | 240.5 / 399 | 否 |
| 340 深色 @1.25 / @1.5 设备缩放 | 240px | 8px | 同 340 | 同 340 | 否 |
| 340 深色，点「今天」并聚焦 | 240px | 8px | 同 340 | 同 340 | 否（`2026/10/27 21:00` 完整显示） |
| 300 深色，点「明天」并聚焦 | 200px | 8px | 224 / 232 | 同 300 | 否（`2026/10/28 09:00` 完整显示） |

其他交互（同一环境）：
- 日期行内已无装饰图标（实测 `iconInDateRow = false`）
- Tab 顺序：任务名称 → 关闭 → 今天 → 明天 → 日期时间（原生日期控件内部再分段属正常）
- 填标题 + Enter：面板关闭、任务名称输入被清空
- 「今天」填入本地 21:00（已过则 23:59），「明天」填入本地次日 09:00，与既有规则一致

截图（浏览器预览，非原生窗口）：`docs/screenshots/quickadd-layout-2026-10-27/`
`A-340-dark-empty` / `B-300-dark-empty` / `C-340-light-empty` / `D-300-light` /
`E-340-dark-today-focused` / `F-300-dark-tomorrow` / `G-340-dark-dsf125` / `G-340-dark-dsf150`

### 原生 WebView2 实机验收（安装版，真实 150% 系统缩放）

安装包（2026-10-27 12:45:29 最终构建）静默安装后启动，WebView2 页面内读取
`devicePixelRatio = 1.5`、`scaleFactor = 1.5`，窗口物理 510×424（逻辑 340×282.67），
即本机真实系统缩放为 **150%**（不是浏览器模拟）。快速添加打开与关闭均由 CDP 真实点击驱动，
未创建任务、未改动数据库内容。

| 检查 | 实测 |
| --- | --- |
| 打开快速添加前 | 物理 510×424（逻辑 340×282.67） |
| 打开快速添加后 | 物理 **510×480**（逻辑 340×**320**）：因正常高度 282.67 < 320 被临时抬高，面板完整可见 |
| 关闭快速添加后 | 恢复物理 **510×424**（逻辑 340×282.67），未把 320 留作正常高度 |
| 300 宽测试后关闭面板 | 物理 **450×424**（逻辑 300×282.67）：高度恢复、宽度保持 300（测试值） |
| 收起态 | 物理 **510×96**（逻辑 64） |
| 收起态打开快速添加 | 物理 **510×480**（逻辑 **320**），从收起高度直接恢复到可用面板高度 |
| 收起态关闭快速添加 | 物理 **510×424**（逻辑 340×282.67），未停在 320 或收起的 64 |
| 340 逻辑宽：日期输入 | 241.33px 宽；右边界 264.67，添加按钮左边界 272.67 → 不相交 |
| 300 逻辑宽：日期输入 | 201.33px 宽；右边界 224.67，添加按钮左边界 232.67 → 不相交 |
| 日期输入左内边距（原生） | **8px**（`.field-input-compact` 生效） |
| 面板 / 底部进度栏 | 面板底 236.83 < 底栏顶 279.33，不重叠 |
| 日期文本裁切 | 否（`2026/10/27 21:00` 完整显示） |
| 日期行装饰图标 | 无 |
| 退出 | `quit_app` 正常调用，进程结束；窗口几何最终仍为 510×424 |

截图（安装版 WebView2 内容区，非浏览器预览）：
`native-webview2-150pct-340-dark-empty.png`、
`native-webview2-150pct-340-dark-today.png`、
`native-webview2-150pct-300-dark-today.png`（均在
`docs/screenshots/quickadd-layout-2026-10-27/`）。

### 未验证（如实保留）

- 真实 100% 与 125% 系统缩放（本机当前为 150%，已实测；另外两个比例未切换系统设置）
- 原生 `datetime-local` 日期选择器弹窗的展开位置与遮挡（未截取原生浮层）
- 日期为空时原生提示文案的逐字判定（截图可见 WebView2 格式提示，但未做文字级断言）
- 浅色主题在安装版原生窗口下的复查（浅色已在浏览器预览验收，安装版本机设置为深色）
- 「贴边收纳球」入口：仓库当前版本无该功能，本轮只验证标题栏「+」与托盘入口的高度保障

## 本轮执行记录（2026-10-26 21:41–21:47，最终源码，含主按钮令牌修复后复跑）

### 自动测试（全部通过）

- TypeScript 严格检查：`npm run typecheck` 通过
- 前端测试：`npm run test` 6 文件 / 59 用例通过
- Rust 测试：`cargo test` **70 用例通过**（含诊断日志 7 项：轮转失败保持真实大小、解除占用后恢复、
  超长 UTF-8 记录不超限、极小上限守约、运行期轮转、归档数量有界、并发不破坏行完整性）；
  `cargo check --all-targets` 通过
- 前端生产构建：`npm run build` 通过（独立执行使用 safari13 目标：CSS 32.39 kB、JS 295.70 kB，
  gzip 后 6.72 kB / 89.13 kB；`tauri build` 内使用 `TAURI_ENV_PLATFORM=windows` → chrome105 目标，
  产物略小：CSS 29.34 kB、JS 294.52 kB，gzip 6.58 kB / 88.73 kB。同一源码，差异来自构建目标）
- `npm run tauri build`：成功产出 NSIS 安装包（release 编译 1m32s + makensis，21:47:53 完成）

### 安装包元数据（2026-10-26 21:47 重建，已被 2026-10-27 00:59 包超越）

| 项 | 值 |
| --- | --- |
| 路径 | `src-tauri/target/release/bundle/nsis/CheckIt_0.1.0_x64-setup.exe` |
| 构建时间 | 2026-10-26 21:47:53（晚于本轮全部源码修改，含主按钮令牌修复） |
| 大小 | 2,015,097 字节（约 1.92 MiB） |
| SHA256 | `B9F45C05FABE1940B7711A96A868C8DCB28025827515331F8CFF41A67BE49A09` |
| 可执行文件 | `src-tauri/target/release/checkit.exe` 5,019,648 字节，SHA256 `FD48A91A909B1E21801706D52713DEA587D16EC17101CFC8999BB4E9015F38C9` |

历史包均已报废，不作为验收对象：18:15 构建（2,014,445 字节 / `2FBE7031…4561271`，
未安装验收即被本轮按钮改动超越）、16:31（2,013,288 / `68A75874…46D580`）、
15:08（2,014,380 / `267C8979…7B1A23F`）。
**当前系统安装仍为 15:08 旧版**（注册表 HKCU 卸载项 `CheckIt` 0.1.0，安装目录
`%LOCALAPPDATA%\CheckIt\checkit.exe` 时间戳 2026-10-26 15:08:10，SHA256 `42B52D19…49D0EA`，
与用户上一轮观察一致，未发生变化；该旧版进程自 17:15 起一直在运行）。
本轮 21:47 包**未执行安装与启动冒烟**（安装会覆盖当前安装，需用户授权，见文末步骤）。
21:47 包相对已实机验收过的 15:08 包的全部差异是：`diagnostics.rs` 两处日志边界修复 +
全应用文字可读性调整 + 主按钮对比度令牌修复（无数据库 schema 变更、无提醒逻辑变更）。

### 全应用文字可读性验收（本轮，2026-10-26）

> **截图环境声明**：本节所有截图都是 **浏览器预览渲染**（Vite + 本机 Chrome headless/CDP），
> 使用替身 `api`/Tauri 模块与固定样例数据，**不是 Windows 原生安装版截图**。
> 原生窗口的 Acrylic 合成、真实 DPI 缩放与系统主题仍列入文末待验证项。

用户提供的红框截图（悬浮窗统计栏「待办 / 今日到期 / 已逾期」几乎不可辨认）在本轮无法取得原文件
（附件目录 `c:\Users\Zane\.trae-cn\attachments\6adef7344a0bc39723520bcb` 不存在），
因此「改前」对比图是从 `HEAD`（1eeef41）基线工作树用同一预览脚手架真实渲染得到的，
覆盖用户描述的全部低对比度位置：统计标签、日期、倒计时、无截止、底部「今日进度」与「全部任务」。

#### 根因

1. 文字颜色本身带透明度（`--text-muted` 为低不透明度浅色），又叠加在半透明玻璃面板和任意桌面壁纸上，
   实际合成对比度远低于原始 RGB 的观感；
2. `--text-secondary` 同样有衰减，导致日期、底部说明等「次要但必要」的文字一起变淡；
3. 统计栏标签长期使用 `text-[10px]`，管理/设置界面大量 `text-xs` 与 10–11px 混排；
4. 已完成任务用整行 `opacity` 弱化，把标题、倒计时、截止时间一起压暗；
5. 菜单、弹窗、侧栏沿用半透明 `--glass-bg-strong`，面板后面是什么颜色，文字对比度就跟着变。

#### 改动策略

- **文字令牌改为不透明实色**，深浅主题各一套主/次/辅助/禁用四级；
  透明度只用于背景层，不再让文字和图标一起变淡；
- 新增 `--surface-canvas` 内容承托层，并入 `.glass-panel` 的双层背景：
  `linear-gradient(var(--surface-canvas), var(--surface-canvas)), var(--glass-bg)`，
  保留下限不透明度，因此无论壁纸明亮或复杂都有稳定底；`--glass-opacity` 仍由用户设置驱动，
  **没有静默覆盖用户保存的透明度数值**；
- 菜单、弹窗、设置侧栏等「内容区」改用近不透明的 `--surface-overlay`；
- 语义色拆分为「色条/图标」与「文字」两套（`--urgent-text` / `--warning-text` / `--ok-text`），
  文字色专门按对比度选取，不再直接拿强调色当文字色；
- 去掉已完成任务整行 opacity，改为 `--text-muted` + 删除线（状态仍可辨认，且不依赖单一颜色表达）；
- 小幅提升组件内写死的小字号：统计标签 10→12px/500，数字 16→20px/600，任务标题 13px，
  描述 12px，截止与倒计时 11–11.5px，底部进度与「全部任务」11.5–12px，菜单 12.5px，
  弹窗标题 15px，侧栏 12.5px。

最终令牌（节选）：

| 令牌 | 深色 | 浅色 |
| --- | --- | --- |
| `--text-primary` | `#f2f4fa` | `#151824` |
| `--text-secondary` | `#d7dce9` | `#343a4d` |
| `--text-muted` | `#c9d0df` | `#4b5164` |
| `--text-disabled` | `#a3aabf` | `#6b7284` |
| `--accent-text` | `#ccd2ff` | `#4338ca` |
| `--urgent-text` | `#fecaca` | `#991b1b` |
| `--warning-text` | `#fde68a` | `#92400e` |
| `--ok-text` | `#6ee7b7` | `#065f46` |
| `--surface-canvas` | `rgba(18,20,27, 0.74 + 0.16×opacity)` | `rgba(249,250,253, 0.88 + 0.06×opacity)` |
| `--surface-overlay` | `rgba(22,24,32,0.94)` | `rgba(252,253,255,0.97)` |

#### 对比度核算方法与结果

不使用「原始 RGB 直接比」的做法：先按 alpha 依次合成
`壁纸 → 玻璃(--glass-opacity) → 内容承托 → 卡片/输入框`，再对合成后的实际底色计算 WCAG 相对亮度对比度。
遍历 2 个主题 × 透明度 0.05 / 0.35 / 0.6 × 7 种背景（纯白、纯黑、中灰、暖色壁纸、冷色壁纸、明暗分区壁纸、
紫蓝渐变） × 4 种表面（panel / card / input / overlay）。

结果：**所有必需文字（primary / secondary / muted / accent / urgent / warning / ok）在全部组合下 ≥ 4.5:1**，
最低三例为 `dark muted@card 4.53`、`dark ok@card 4.60`、`dark accent@card 4.75`
（均出现在「透明度最低 + 纯白背景」的最不利组合，也就是承托层最薄的时候）。
`disabled` 为 3.0–3.7，按需求允许弱化（仍可辨认，且不是必需信息）。

#### 已验证组合（浏览器预览）

| 维度 | 覆盖 |
| --- | --- |
| 主题 | 深色、浅色（跟随系统在预览中无法取得，属待验证） |
| 玻璃效果 | 开启、关闭（`after-widget-noglass-dark/light`） |
| 面板不透明度 | 0.05（最低，最不利）、0.35（默认）、0.6（高） |
| 背景 | 纯白、中灰、纯黑、暖/冷色复杂渐变壁纸（`op005-lightbg` 为「最低透明度 + 明亮背景」最不利组合） |
| 窗口尺寸 | 默认 340×440、最小 300×200、等效 125%（425×550）、等效 150%（510×660） |
| 界面 | 悬浮窗（含空态、快速添加、更多菜单）、管理窗口（深/浅）、设置、编辑弹窗 |

布局自检（DOM 实测）：所有截图场景 `documentElement.scrollWidth == clientWidth`，
即**无横向滚动**；最小 300×200 下统计栏三张卡片均不截字、不重叠，标签完整显示。

#### 截图清单（`docs/screenshots/readability-2026-10-26/`）

| 文件 | 内容 |
| --- | --- |
| `before-widget-dark.png` / `before-widget-light.png` | 改前悬浮窗（HEAD 基线，深/浅） |
| `before-manager-dark.png` | 改前管理窗口 |
| `before-settings-dark.png` | 改前设置页 |
| `before-taskmodal-light.png` | 改前编辑弹窗 |
| `after-widget-dark.png` / `after-widget-light.png` | 改后悬浮窗（深/浅） |
| `after-widget-min.png` | 改后最小窗口 300×200 |
| `after-widget-empty.png` | 改后空态 |
| `after-widget-op005-lightbg.png` / `after-widget-light-op005-darkbg.png` | 最低透明度 + 明亮/深色背景 |
| `after-widget-op06.png` | 高透明度叠加 |
| `after-widget-noglass-dark.png` / `after-widget-noglass-light.png` | 关闭玻璃效果 |
| `after-manager-dark.png` / `after-manager-light.png` | 改后管理窗口（深/浅） |
| `after-settings-dark.png` | 改后设置页 |
| `after-taskmodal-light.png` | 改后编辑弹窗 |
| `after-quickadd-dark.png` / `after-moremenu-dark.png` | 快速添加 / 更多菜单 |
| `after-widget-scale125.png` / `after-widget-scale150.png` | 等效 125% / 150% 缩放 |

#### 浏览器预览无法替代的部分（待验证）

- Windows 原生 Acrylic/Mica 的实际合成底色与开启「透明效果」系统设置时的观感；
- 真实 100% / 125% / 150% 系统缩放（预览是用 CSS 视口尺寸模拟，不是真实 DPI 缩放）；
- 混合 DPI 多显示器、系统主题切换（跟随系统）在原生窗口上的即时效果；
- 复杂动态壁纸（幻灯片/视频壁纸）在原生透明窗口后的可读性。

### 主按钮文字对比度修复与验证（2026-10-26 收尾续，本轮重点）

> **截图与核对环境声明**：本节所有对比度数值与截图均来自 **浏览器预览渲染**
> （Vite + 本机 Chrome headless/CDP，Windows 11，Chrome 稳定版，视口 340×440 / 900×650，
> 100% 缩放，固定样例数据），使用替身 `api`/Tauri 模块，**不是 Windows 原生安装版**。
> 原生 Acrylic 合成下的观感仍列入文末待验证项。

#### 问题

主按钮此前复用装饰性强调色 `--accent`（背景）+ `--accent-hover`（悬停）+ 白字，
按不透明色计算：深色默认 4.47:1、深色悬停 3.38:1、浅色悬停 4.47:1，均低于 4.5:1。
且装饰强调色一旦调整会连带影响按钮文字对比度。

#### 修复

- 新增**主按钮专用令牌**（深浅主题各一套）：`--btn-primary-bg` / `--btn-primary-hover` /
  `--btn-primary-active` / `--btn-primary-text`，与装饰性 `--accent` 系解耦
  （`src/styles/globals.css`）；
- 公共 Button `primary` 变体（`src/components/ui/Buttons.tsx`）、悬浮窗标题栏「+」按钮
  （`WidgetHeader.tsx`，独立实现）、设置页 Toggle 选中态轨道（`SettingsPanel.tsx`）切换至新令牌；
- 禁用（`disabled:opacity-60`）、加载中（`disabled={saving}`/`{submitting}` 在途保护）、
  键盘焦点（`focus-visible:outline`）、点击反馈（`active:scale` + 按下色）与防重复提交行为全部保持不变；
- 排查结论：DataGate 两处「重试」按钮为描边 / banner 样式（非 accent 背景），此前已核算通过；
  其余主按钮（新建任务、保存、添加、分类管理、新建分类）均走公共 Button primary，自动继承新令牌。

#### 新令牌取值（不透明色直接计算）

| 令牌 | 深色 | 与白字对比度 | 浅色 | 与白字对比度 |
| --- | --- | --- | --- | --- |
| `--btn-primary-bg` | `#5a5df0` | 4.93 | `#4f46e5` | 6.29 |
| `--btn-primary-hover` | `#6164f1` | 4.56 | `#6164f1` | 4.56 |
| `--btn-primary-active` | `#4a4de5` | 6.02 | `#4338ca` | 7.90 |

悬停反馈为「微亮背景 + shadow-sm→shadow-md」组合：在 ≥4.5:1 约束下纯亮度变化空间有限，
叠加阴影增强使悬停可感知；按下降深背景并保留 `active:scale`。

#### 真实渲染核对（不是只核算颜色变量）

方法：Chrome headless CDP 打开预览页 → 读取目标按钮 `getComputedStyle` 的
`backgroundColor`/`color` → 计算 WCAG 对比度。悬停态由 CDP `Input.dispatchMouseEvent`
移动到元素中心触发；**按下态在按住左键期间读取**（`:active` 由浏览器命中测试触发）；
弹窗/快速添加面板由前置合成 `.click()` 打开后核对。悬停截图另做像素采样复核，
与计算样式完全一致（深 90,93,240 → 97,100,241；浅 79,70,229 → 97,100,241）。

核对矩阵（4 处主按钮 × 深/浅 × 默认/悬停/按下，共 24 组，**全部 ≥ 4.5:1**）：

| 按钮位置 | 主题 | 默认 | 悬停 | 按下 |
| --- | --- | --- | --- | --- |
| 悬浮窗「+」（WidgetHeader，独立实现） | 深 | 4.93 | 4.56 | 6.03 |
| 悬浮窗「+」 | 浅 | 6.29 | 4.56 | 7.90 |
| 管理窗口「新建任务」（Button primary sm） | 深 | 4.93 | 4.56 | 6.03 |
| 管理窗口「新建任务」 | 浅 | 6.29 | 4.56 | 7.90 |
| 编辑弹窗「保存」（Button primary，13px/500） | 深 | 4.93 | 4.56 | 6.03 |
| 编辑弹窗「保存」 | 浅 | 6.29 | 4.56 | 7.90 |
| 快速添加「添加」（Button primary sm，12px/500） | 深 | 4.93 | 4.56 | 6.03 |
| 快速添加「添加」 | 浅 | 6.29 | 4.56 | 7.90 |

（读数均为真实计算样式：深色默认 `rgb(90,93,240)`、悬停 `rgb(97,100,241)`、按下 `rgb(74,77,229)`；
浅色默认 `rgb(79,70,229)`、按下 `rgb(67,56,202)`。）

**覆盖状态说明**：本矩阵覆盖主按钮**启用态**的默认/悬停/按下三种状态；禁用态按需求允许弱化
（`opacity-60` 合成后约 2.6–3.7，仍可辨认）；键盘焦点态为 outline 描边（不改变文字/背景对比度）；
按钮文字均为白色不透明。不宣称「全部文字通过」——全局文字核算见上一节，
原生窗口下的实际观感见文末待验证项。

#### 截图清单（`docs/screenshots/button-contrast-2026-10-26/`，浏览器预览环境）

| 文件 | 内容 |
| --- | --- |
| `widget-dark-default.png` / `widget-dark-hover-add.png` | 悬浮窗深色，「+」默认 / 悬停 |
| `widget-light-default.png` / `widget-light-hover-add.png` | 悬浮窗浅色，「+」默认 / 悬停 |
| `widget-dark-quickadd-default.png` / `widget-dark-quickadd-hover-save.png` | 快速添加深色，「添加」默认 / 悬停 |
| `widget-light-quickadd-default.png` / `widget-light-quickadd-hover-save.png` | 快速添加浅色，「添加」默认 / 悬停 |
| `manager-dark-default.png` / `manager-dark-hover-create.png` | 管理窗口深色，「新建任务」默认 / 悬停 |
| `manager-light-default.png` / `manager-light-hover-create.png` | 管理窗口浅色，「新建任务」默认 / 悬停 |
| `manager-dark-modal-default.png` / `manager-dark-modal-hover-save.png` | 编辑弹窗深色，「保存」默认 / 悬停 |
| `manager-light-modal-default.png` / `manager-light-modal-hover-save.png` | 编辑弹窗浅色，「保存」默认 / 悬停 |

全部截图经布局自检（`scrollWidth == clientWidth`，无横向滚动）。

### 安装版实机验证通过（15:08 构建的安装包；正常交互登录账户 Zane，Windows 11）

1. **通知提交成功（取代历史 failed）**
   - 任务 1（offset 30s 与 0s）：`reminder_events` 两条均为 `state=submitted`、`attempts=0`
   - Windows 通知数据库 `%LOCALAPPDATA%\Microsoft\Windows\Notifications\wpndatabase.db`：
     handler `com.checkit.desktop`，payload 分别为
     `「通知验收任务」28秒后截止`（arrival 07:18:36）、`「通知验收任务」已逾期 1秒`（arrival 07:19:06）
   - 注册表 `HKCU\...\Notifications\Settings\com.checkit.desktop\LastNotificationAddedTime`
     = `2026-10-26 07:19:06 UTC`，与提交时间吻合
   - 应用日志 `%LOCALAPPDATA%\com.checkit.desktop\logs\checkit.log` 无提交错误
2. **安装态 AUMID 注册**：开始菜单与桌面快捷方式 `System.AppUserModel.ID` 均为 `com.checkit.desktop`，
   与 `tauri.conf.json` 的 `identifier`、`SystemNotificationTransport` 实际使用的 app_id 一致
3. **关闭期间到期 → 重启补发**：任务 2 在应用关闭期间到期，重启后 2 分钟内补发
   （`state=submitted`，通知库 order 3038 `「后台提醒验收」已逾期 1分钟`），符合 15 分钟宽限策略
4. **隐藏到托盘后台提醒**：关闭窗口（`WM_CLOSE`）后进程存活，调度器照常工作，
   到点提交 `state=submitted`（通知库 order 3039 `「后台隐藏提醒验收」已逾期 1秒`）
5. **单实例（安装版形态）**：窗口可见且 `Topmost=True`；关闭后 `visible=False` 但进程存活；
   启动第二实例（PID 34032）自动退出，进程数保持 1，原窗口恢复 `visible=True`；
   日志记录“检测到第二实例启动，唤醒已有窗口并退出第二实例”
6. **v2→v3 迁移**：构造起始 schema=2、含 `state='sent'`、`attempts=2`、`theme=light` 的库；
   启动后 `MAX(version)=3`，任务保留，`sent`→`submitted`，`attempts=2` 保留，设置保留
7. **卸载与重装数据保留**：`uninstall.exe /S` 后安装目录、开始菜单/桌面快捷方式、HKCU 卸载项均移除；
   `%APPDATA%\com.checkit.desktop\checkit.db` 保留 3 个任务；重装后 AUMID 一致、任务数 3、应用正常启动
8. **任务 CRUD 与持久化**：新增、编辑、完成、恢复、删除均有 UI 操作与数据库结果佐证；
   多窗口 `data-changed` 同步正常（2026-10-25 记录与本轮复测一致）

### 隔离环境行为验证（本轮新源码，2026-10-26 16:43）

使用与最终源码一致的构建（编译期 `appDirectoriesOverride` 把数据目录重定向到临时目录，
可执行文件复制到 `target` 之外，从而按安装版形态使用 `com.checkit.desktop` app_id），
设置 `close_to_tray=true`、创建任务 `due_at = 08:43:01Z`（本地 16:43:01）：

1. 启动后窗口 `IsWindowVisible = True`；
2. 发送 `WM_CLOSE`：进程存活，窗口 `IsWindowVisible = False`（进入托盘隐藏）；
3. 隐藏期间到点：`reminder_events` 为 `submitted, attempts 0, updated_at 2026-10-26T08:43:02.504Z`；
   Windows 通知库新增唯一一条 `「Tray-check task」已逾期 1秒`，`ArrivalTime` 换算 16:43:02.45，
   与 `updated_at` 相差 55 ms（提交成功 + 通知中心登记）；
4. 启动第二实例：立即退出，隐藏窗口恢复 `IsWindowVisible = True`，`checkit` 进程数为 1，
   且未产生第二条 `submitted` 或第二条通知；
5. 隔离日志 732 字节，无提交失败记录。

说明：通知库中更早的 order 3050（16:34:38）来自本脚本一次更早的中断运行（当时任务 due 16:34:37），
不属于单次运行内的重复提交；干净复跑时通知库计数为 `15 → 16`，只新增上述一条。

结论：**关闭窗口进入托盘后仍按时提醒、单实例唤醒隐藏窗口两项在新源码上通过**。
横幅是否肉眼可见仍需人工确认（API 提交与通知中心登记不等于屏幕可见）。

### 通知失败：证据边界（结论已收紧）

历史 `db-dump.txt` 中的 `state=failed / attempts=3` 来自独立测试账户 CheckItTest
（该账户进程以 `runas` / `CreateProcessWithLogonW` 注入 Zane 的交互会话 SessionId=2）。

目前**能确认的证据**：

- 历史测试账户下发送失败（仅此一条事实，但没有留下当时的实际 HRESULT）；
- 正常交互登录账户下，安装版提交返回 `submitted`，Windows 通知库与注册表均有登记。

目前**尚缺的证据**：

- 历史失败发生时的实际 HRESULT（旧版本没有初始化 logger，错误信息未落盘）；
- 同一测试账户在正常登录后的对照结果。

因此结论只能是：

> **高度疑似测试账户运行环境问题；正常交互登录账户下已验证提交成功，历史失败的具体底层原因尚未完全确认。**

该结论不是“根因已定位”，也不能表述为“Toast 提交必然失败”。
若后续取得同账户对照结果或实际 HRESULT，应在本文件更新后再调整结论。

注意：`diagnostics.rs` 单元测试里的 `0x80070490`、`0x80070005` 是**手工构造**的示例错误码，
用于验证日志格式与保留行为，**不是**历史通知失败的真实错误码，不得引用为证据。

### 通知的三层语义（不得混用）

| 层次 | 含义 | 目前的证据 |
| --- | --- | --- |
| API 提交成功 | `notify-rust` 同步 `show()` 返回 `Ok`，`reminder_events.state = submitted` | 正常登录账户下安装版已验证 |
| 通知中心登记 | `wpndatabase.db` 中 `com.checkit.desktop` handler 出现对应 payload，注册表 `LastNotificationAddedTime` 更新 | 已验证（见上方实机记录第 1 项） |
| 横幅实际可见 | 屏幕上真实弹出横幅 | **待验证**（仅人工目视可确认，见下方人工项 1） |

### 附带发现并已修复的真实缺陷（诊断日志）

项目此前调用 `log::error!` 但从未初始化 logger，安装版失败时 HRESULT 完全丢失。
新增 `src-tauri/src/diagnostics.rs`，在 `.setup` 最先初始化。

保留策略（运行期生效，不只在启动时检查）：

- 活动文件 `%LOCALAPPDATA%\com.checkit.desktop\logs\checkit.log` 上限 **1 MiB**；
- 归档 `checkit.log.1` … `checkit.log.3` 共 **3 个**，轮转时删除最旧；
- 磁盘占用上限约 **4 MiB**（活动 + 3 个归档）；单条超长记录会在 UTF-8 边界截断到单文件上限；
- 每次写入前在同一把互斥锁内完成「检查大小 → 轮转 → 写入」，多线程并发安全；
- 打开/写入/轮转失败一律静默丢弃本轮日志，不崩溃、不递归记录；
- 记录内容仅包含时间、级别、来源 target、原始错误文本（含 HRESULT），不记录任务标题/描述等业务内容。

#### 本轮修复的两个日志边界问题（2026-10-26）

**问题 1：轮转失败后仍会突破容量限制。**
旧实现的 `rotate()` 在「改名失败且删除也失败」时把 `state.written` 清零，下一条记录于是把旧文件
当成空文件，跳过容量检查继续追加。复现方式：Windows 上另开一个句柄以
`FILE_SHARE_READ | FILE_SHARE_WRITE`（允许读写、拒绝删除/改名）占住 `checkit.log`，
上限 128 字节，连续写入后文件增长到 960 字节。

修复：失败分支改为回读真实文件大小（`fs::metadata`，读不到时退回已知大小）并保留在 `state.written`，
容量检查持续生效，后续记录一律丢弃；只有成功改名/删除（即成功轮转或创建空文件）才清零。
占用解除后下一次容量检查即可正常轮转恢复。

**问题 2：超长记录截断后超过上限。**
旧实现先把内容截到 `max_bytes`，再追加 `"…(truncated)\n"`（15 字节），因此实际写入超过上限。
复现：上限 128 字节，实际写入 143 字节。

修复：新增 `fit_line()`，先从上限中扣除截断标记与换行的字节预算，再按 `String::is_char_boundary`
回退到合法 UTF-8 边界截断；上限小于等于标记长度（含 1 字节的极端配置）时退化为「可容纳内容 + 换行」，
仍保证总字节数 ≤ 上限。

对应回归测试见上表第 44–46 行；`cargo test` 共 70 项通过。

### 提醒去重重复现象：调查结论与证据

独立验收观察到同一任务截止提醒在本地时间 **15:31:51** 与 **15:35:09** 各出现一次
（Windows 通知库 order 3039 / 3041，payload 同为「后台隐藏提醒验收」，分别为“已逾期 1秒”“已逾期 3分钟”）。
当时的验收过程做过数据库备份、恢复与重装。

证据链：

1. 备份只复制了 `checkit.db`，没有同时复制 `checkit.db-wal`（项目使用 WAL 模式）。
2. 对比读取同一份备份：只打开 `checkit.db` 时，`reminder_events` 缺少该任务 offset 0 的
   `submitted` 记录；`checkit.db` + `checkit.db-wal` 一起读取时才出现
   `(task 3, offset 0, due 2026-10-26T07:31:50Z, submitted, attempts 0, updated_at 2026-10-26T07:31:51.716Z)`。
3. 15:31:51 的首次通知与上述 `updated_at` 吻合；15:35:09 的第二次通知发生在恢复/重装之后，
   此时去重状态已被回滚为“未提交”，于是按正常 at-least-once 恢复逻辑再次提交。

结论：**重复通知由测试环境的备份/恢复方法造成（遗漏 WAL），不是应用去重状态机缺陷**。
因此不修改状态机；修正的是测试方法：备份必须使用 SQLite 一致性备份
（`VACUUM INTO` / 备份 API / 先 checkpoint 后复制整组文件），不得单独复制运行中的主文件。

#### 隔离环境去重复测（本轮新增，2026-10-26）

为避免触碰真实业务数据库，使用编译期 `appDirectoriesOverride` 把数据目录重定向到临时目录，
在 `%TEMP%\checkit-dedup\` 下独立运行（可执行文件复制出 `target` 后按安装版 AUMID `com.checkit.desktop` 运行）：

1. 新建任务 `Dedup-check task`，`due_at = 2026-10-26T08:21:10Z`，offset 0 提醒为 `pending`；
2. 首次运行：调度器提交成功，事件为
   `(1, 0, submitted, attempts 0, updated_at 2026-10-26T08:21:11.749Z)`；
   Windows 通知库新增唯一一条 `「Dedup-check task」已逾期 1秒`，`ArrivalTime = 134374764717355398`
   → 换算本地时间 **16:21:11.735**，与 `updated_at` 相差 14 ms；
3. 发送 `WM_CLOSE` 正常退出（exit code 0，连接正常关闭）；
4. 重启运行 45 秒，**数据库不做任何手工回滚**：`state/attempts/updated_at` 三者完全未变，
   通知库记录数保持 12 条（重启后新增 0 条）；
5. 对照：`before=11 → 首次运行后=12 → 重启后=12`；隔离日志 `data/logs/checkit.log` 无提交失败记录。

说明：`before` 快照中的 order 3048（`ArrivalTime` → 16:18:13）来自本轮更早的一次独立尝试，
不属于本次运行的重复提交；本次运行对应的新增记录只有 order 3049 一条。

**结论：在数据库不被回滚的前提下，重启不会重复提交通知。**

#### 仍未完成的部分（待验证）

- 同版本卸载/重装后“已提交提醒状态”是否保留：上一轮已实机验证卸载/重装后任务与设置保留（3 个任务），
  但未单独核对 `reminder_events` 的 `submitted` 行；本轮未重复执行卸载/重装（会中断当前安装），
  且不得为此恢复/覆盖真实业务数据库，**标记待验证**，需用户授权后按下方步骤执行。
  静态依据：卸载器只移除安装目录与快捷方式，不删除 `%APPDATA%\com.checkit.desktop` 下的数据文件。

## 需要人工实机验证的项目（CI 无法覆盖）

状态严格区分：**已通过（实机）** / **已通过（隔离环境）** / **待验证**。

| # | 项目 | 状态 | 说明 |
| --- | --- | --- | --- |
| 1 | 通知横幅视觉显示 | **待验证** | 已确认 API 受理 + 通知中心登记，但未目视确认横幅；受专注助手/通知横幅设置影响 |
| 2 | 关闭窗口进入托盘后到时提醒 | **已通过（隔离环境）** | 见上方隔离验证：隐藏后到点提交且通知中心登记 |
| 3 | 真实睡眠/唤醒：宽限内补发、超限静默、已提交不重复 | **待验证** | 仅用可注入时钟模拟“扫描时间过期”；未做真实睡眠唤醒 |
| 4 | 开机启动开关与系统状态一致 + 登录后真实自启 | **部分通过 / 待验证** | 开关对账已实现并有自动化；真实注销/登录后自启未实机验证 |
| 5 | 托盘菜单六项（打开/快速添加/管理/置顶/设置/退出） | **待验证** | 自动化只覆盖“锁外调用”路径；菜单项点击与置顶同步未逐项目视 |
| 6 | 安装版单实例与隐藏窗口唤醒 | **已通过（隔离环境）** | 第二实例退出、隐藏窗口恢复可见、无重复提交 |
| 7 | 同版本卸载/重装后任务、设置、已提交提醒状态保留 | **部分通过 / 待验证** | 上一轮已验证任务与设置保留（3 个任务）；本轮未核对 `submitted` 行，也未重复卸载/重装 |
| 8 | 旧版本安装包覆盖升级 | **未测** | 仓库无旧版安装包；不得用同版本重装冒充升级。v2→v3 迁移另有自动化与实机验证 |
| 9 | 混合 DPI / 多显示器位置恢复 | **待验证** | 有物理像素存储与可见性回退的自动化，缺真实混合缩放环境目视 |
| 10 | 长时间驻留内存/CPU | **待验证** | 未做数小时驻留采样 |
| 11 | 开发服务器（devCsp）React Refresh / HMR | **待验证** | 仅静态配置检查 |
| 12 | Acrylic 玻璃效果渲染质量与不透明降级 | **待验证** | 早期实机记录为观感通过，本轮未复测；本轮可读性截图全部来自浏览器预览，不能代替原生渲染 |
| 13 | 新安装包（2026-10-27 09:00）的窗口/菜单/通知/日志/自定义提醒冒烟 | **大部分通过** | 09:00 包已安装并实机验收（见「安装版实机验收」A1–A10）：任务 CRUD 创建/落库、自定义提醒保存与回显（UI + 数据库秒值）、默认提醒语义、提醒实际触发与通知提交（submitted + wpndatabase）、单实例唤醒、托盘退出、重启去重、验收记录清理均通过；横幅目视、关托盘后台提醒独立复测、完成/恢复 UI 仍未验证 |
| 14 | 原生窗口在真实 100% / 125% / 150% 缩放下的排版 | **待验证** | 预览只用 CSS 视口尺寸模拟（425×550 / 510×660），不是真实 DPI 缩放 |
| 15 | 原生窗口配合「跟随系统」主题与复杂壁纸的可读性 | **待验证** | 预览只能显式指定深/浅主题与固定渐变背景 |
| 16 | 已完成/禁用/占位符文字在原生透明窗口下的可辨认性 | **待验证** | 预览中已确认（`--text-muted` + 删除线），原生合成底色未复测 |

### 需要用户配合的具体步骤（会中断当前工作/改动系统安装）

以下步骤尚未执行（09:00 包的安装启动冒烟已于 2026-10-27 上午完成，从本清单移除）：

1. **通知横幅目视**：创建一个 1 分钟后到期的任务，保持专注助手关闭、通知横幅开启，
   在屏幕上确认横幅出现（截图留存）。本轮已取得前两层证据（submitted + wpndatabase 登记），
   仅缺横幅目视确认。
2. **关托盘后台提醒独立复测**：创建临近截止任务 → 托盘退出全部窗口显示（或关闭窗口到托盘）
   → 等待提醒到期 → 核对 reminder_events 与 wpndatabase。本轮因管理窗口关闭后无法通过
   自动化重新打开而未完成；支撑证据见「安装版实机验收」A6 与上轮 15:08 包记录。
3. **原生可读性目视**：在安装版中分别切换深色/浅色/跟随系统、玻璃开/关、
   面板不透明度最低/默认/最高，并在明亮、纯色、深色、照片壁纸下各截图一张；
   再在 Windows 显示设置中切 100% → 125% → 150%，确认统计栏三张卡片与底部进度不截字、不重叠。
4. **真实睡眠唤醒**：确保有 15 分钟内到期的提醒 → 合盖/睡眠 → 唤醒 → 确认补发；
   再把提醒调到超过 15 分钟 → 睡眠 → 唤醒 → 确认静默。
5. **登录后自启**：打开“开机启动”开关 → 注销/重启 → 登录后确认应用自动启动。
6. **托盘菜单逐项目视**：本轮已验证「任务管理」与「退出 CheckIt」两项（自动化驱动）；
   打开 / 快速添加 / 置顶 / 设置四项与置顶勾选同步仍待逐项目视。
7. **卸载/重装保留状态**：先用一致性备份保存 `%APPDATA%\com.checkit.desktop`
   （SQLite `VACUUM INTO` 或先 checkpoint 后复制整组文件，禁止只复制主库），
   记录 `reminder_events` 的 `submitted` 行；卸载 → 重装 → 确认任务、设置与已提交状态一致。
8. **日志边界实测（可选，增强证据）**：安装后用另一个进程以允许读写、拒绝删除的方式占住
   `checkit.log`，持续产生错误日志（如断网触发通知失败），确认文件不超过 1 MiB；
   释放占用后确认生成 `checkit.log.1`。仅需观察文件大小，不需要改动业务数据。

## 历史实机验证记录（2026-10-25，Windows 11，构建产物 checkit.exe）

以下通过 OS 级真实鼠标键盘输入（Win32 SendInput）与数据库内容核验完成，非自动化断言：

1. ✅ 应用启动：进程存活（内存 ~30MB），SQLite 自动创建于 `%APPDATA%\com.checkit.desktop\`（WAL 模式），默认分类自动播种
2. ✅ 悬浮窗真实渲染：玻璃态背景模糊、状态概览、任务列表、倒计时（逐秒刷新）、逾期红色标记、优先级指示条、底部进度条
3. ✅ 真实点击 + 按钮 → 快速添加面板打开
4. ✅ 真实键盘输入 + Enter → 任务创建（数据库核验），面板自动关闭，待办统计 1→2 实时更新
5. ✅ 真实点击「全部任务」→ 管理窗口打开（async 修复后），侧栏（全部/今日/未来7天/已完成/分类列表/设置）完整渲染，任务列表显示真实数据
6. ✅ 管理窗口编辑任务 → 修改落库 → 悬浮窗同步刷新（data-changed 广播生效）
7. ✅ 管理窗口删除流程实机全链路：双击任务行 → 编辑弹窗 → 真实点击"删除" → 原生确认框 Enter → 管理窗口与悬浮窗同时清零（data-changed 双窗口同步）→ 重启后 DB/WAL 均无该任务（删除持久化）
8. ⚠️ 提醒调度端到端（**证据边界说明**）：曾观测到 reminder_events 在截止当秒被领取。
   该证据仅证明调度器领取了提醒并进入提交流程，不构成"通知 API 已成功调用"或"横幅已显示"的证明。
   v3 后，`state='submitted'` 才代表同步通知 API 返回成功——仍不保证用户看到横幅。
   横幅视觉显示与 API 成功率需人工在系统通知设置正常的环境复测（见上方待验证第 1、2 项）。
9. ✅ IPC 稳定性：CDP 探测 t0/10s/60s/180s 全部 5-8ms 响应；通知触发后持续稳定
10. ✅ 空态/超时等边界：空任务列表空态文案正常

### 已知测试工具限制（非应用缺陷）

- 附加 DevTools 协议（--remote-debugging-port）时，页面上下文的 invoke 响应可能不送达（命令本身执行成功）——调试器干扰，无调试器时正常（真实输入路径验证面板正常关闭）
- SendKeys 输入会被微软拼音输入法组合，首个 Enter 被用于确认候选词——测试脚本需双 Enter 或剪贴板粘贴

## 已知边界（如实说明）

- 应用真正退出或电脑关机期间的提醒无法送达（进程不存在）；重启后在 15 分钟宽限内的会补发，超出的静默标记
- 扫描批次共用一个 `now` 做领取/恢复；每条动作在真正提交前重新读取实时钟并再次校验宽限，
  submitted/failed 的 `updated_at` 使用提交完成时间，失败重试间隔从实际失败完成时间起算。
  但**已经开始的系统通知调用无法撤回**：若 OS 已受理、状态尚未落库时崩溃，恢复后可能重复提交
- 通知状态为 at-least-once：崩溃窗口（OS 已受理、submitted 未落库）恢复后可能重复提交，不承诺 exactly-once
- 产品策略为**单实例**：第二实例在插件 setup 阶段转发唤醒请求并在数据库迁移/托盘/调度器启动前退出。
  提醒引擎本身不带跨进程原子领取租约，也不支持多个调度器并行；绕过单实例保护同时运行两个引擎会产生重复领取/提交
- Windows 通知的"点击聚焦"依赖插件/系统默认行为，个性化跳转（如直接打开任务详情）未在 v0.1.0 实现

## 本轮执行记录（2026-10-27 晚，贴边收纳球 + 临时高度持久化修复）

### 功能摘要

悬浮窗三形态 `Expanded` / `Collapsed`(64px) / `Docked`(40px 圆圈)。拖到工作区边缘 12 逻辑像素内松手收纳为圆圈，
圆圈显示全部 pending 计数（含逾期，0 / 1–99 / 99+），单击恢复、可拖动、可拖离恢复。
正常展开几何作为唯一权威保存在 Rust 托管状态，临时高度（64 / 40 / 320）绝不覆盖它。

### 自动测试（全部通过）

| 项目 | 命令 | 结果 |
| --- | --- | --- |
| 类型检查 | `npx tsc --noEmit` | exit 0 |
| 前端测试 | `npx vitest run` | 10 文件 / **97 项通过** |
| Rust 测试 | `cargo test` | **111 项通过**（新增 3 项） |
| Rust 编译 | `cargo check` | 通过，无 error |
| 生产构建 | `npx tauri build --bundles nsis` | 成功（包 1.93 MiB） |

本轮新增的 Rust 用例（`src-tauri/src/dock.rs`）：

- `click_on_bubble_is_not_a_drag`：圆圈上 ≤4 逻辑像素抖动必须判定为单击（不得走拖动分支）
- `drag_away_from_edge_exceeds_click_threshold`：真实拖离（400+ 像素）必须超过单击阈值
- `cross_screen_dpi_scale_does_not_look_like_drag`：跨屏时窗口因 DPI 整体变大（340→510），
  但鼠标几乎未动 → 位移必须按鼠标净位移判定，不得误判为拖动
- `offscreen_positive_x_falls_back_inside_work_area`：保存坐标落在当前显示器之外时，
  钳制后必须完整落在工作区内且**尺寸不变**（不得退化为最小尺寸）
- `clamp_keeps_rect_intact_when_only_position_is_offscreen`：仅位置越界时不改动尺寸

本轮新增的前端用例（`WidgetApp.test.tsx`，共 14 项）：

- 圆圈模式下托盘「快速添加」先恢复完整面板，再以 320 临时高度进入快速添加
- 从圆圈展开会清空快速添加标记，不残留窄高度面板

### 安装包元数据（2026-10-27 21:45:29 重建，最新）

| 项 | 值 |
| --- | --- |
| 路径 | `src-tauri/target/release/bundle/nsis/CheckIt_0.1.0_x64-setup.exe` |
| 大小 | 2,027,672 字节（1.93 MiB） |
| 构建时间 | 2026-10-27 21:45:29 +08:00 |
| SHA256 | `B9159FEADC6A494697B28C078256C887958FC57B9FF5AE2D5D04B9DA72F11ED3` |
| `checkit.exe` | 5,053,952 字节，SHA256 `C424504C126867221527CBBAC9386D6B12383EE945F5B9670D279E48244E7632` |

### 贴边收纳球：验证状态（如实区分，不得混写）

| 规格项 | 状态 | 依据 |
| --- | --- | --- |
| 贴边阈值 12 逻辑像素、四边、角落取最近边 | 已实现 + 自动测试通过 | `dock.rs`：`select_edge` / `nearest_edge` / `dock_anchor_places_bubble_on_each_edge` 等 |
| 拖动结束识别（不误判暂停/跨屏） | 已实现 + 自动测试通过 | `drag_watcher_loop` 轮询 `GetAsyncKeyState`；`drag_move_distance` 单测覆盖跨屏 DPI |
| 圆圈计数（含逾期，0/99/99+） | 已实现 + 自动测试通过 | `format_bubble_count` 单测 + `DockBubble` 组件测试 |
| 单击恢复 / 拖离恢复 / 圆圈内拖动 | 已实现 + 自动测试通过 | `handle_drag_end` 分支 + `is_click` 单测 |
| 原生窗口真正缩小到 40px | 已实现 + 自动测试通过 | `dock_to` 先 `set_min_size(40)` 再 `set_size(40)`（**实机尺寸待人工确认**） |
| 临时高度不覆盖展开几何 | 已实现 + 自动测试通过 | `should_persist_expanded` + 5 项 database 测试 |
| 重启恢复最后形态 / 锚点不可见回退 | 已实现 + 自动测试通过 | `restore_widget_geometry` Docked 分支 + 回退钳制 |
| 设置开关 `edge_dock_enabled` | 已实现 + 自动测试通过 | `edge_dock_setting_defaults_on_and_patch_persists` |
| **原生窗口实机拖动** | **未验证** | 见下方「自动化环境限制」 |

### 自动化环境限制（本轮实测结论，非应用缺陷）

本轮尝试用脚本注入鼠标事件完成原生窗口拖动验收，**失败**，并已通过对照实验确认原因在环境侧：

1. **本会话的任务注入：** 在被管控的自动化会话中，脚本注入的鼠标事件无法拖动**任何**窗口——
   用记事本作对照组同样无法拖动，因此不能用「拖动应用窗口失败」推断代码有缺陷。
2. **截屏与坐标：** 本机显示器原生为 2560×1600 @150%（应用使用 `GetMonitorInfoW` 物理像素），
   而截图侧进程默认被 DPI 虚拟化为 1707×1067。未声明 per-monitor DPI aware 时，
   `GetWindowRect` 拿到的坐标与应用不一致，会把**只截到窗口左上角一小块**误判为「窗口只有 300×200」。
   已提供 `scripts/shot-dpi-bom.ps1` / `scripts/move-shot-bom.ps1`（进程内声明 DPI 感知）作为正确工具。
3. **WebView2 内容截图：** 即使把窗口移到可见区，`CopyFromScreen` 也只能取到窗口底色；
   `PrintWindow(0x2)` 返回全黑。**对照实验**：把同一份极简 HTML（纯色背景 + 一行大字）
   用 Edge 打开后 `CopyFromScreen` 可完整截到内容（见 `scripts/edge2.png`），
   而同一文件在 Tauri WebView2 中仍无法截图。结论：**该会话不具备 WebView2 画面截取能力**，
   不是渲染缺陷。

因此本轮**没有取得**「原生窗口完整面板 / 贴边圆圈+计数 / 单击恢复 / 圆圈进入快速添加」的截图或录屏。

### 待人工验证清单（必须在正常交互登录的桌面完成）

1. 拖动悬浮窗到屏幕左/右/上/下边缘松手 → 收纳为 40×40 圆圈，位置紧贴边缘且完整可见
2. 角落松手 → 吸附到最近的一条边（结果稳定）
3. 拖动途中停顿 2–3 秒后继续拖动 → **不得**提前收纳（验证不误判暂停）
4. 跨屏（双显示器 + 不同缩放）拖动 → **不得**在拖动过程中突然缩小
5. 圆圈计数：0 个显示 `0`；≥100 个显示 `99+`；悬停 tooltip 显示完整数量
6. 单击圆圈 → 恢复完整面板，尺寸与收纳前一致且完整可见
7. 圆圈内拖动但仍在边缘松手 → 仍是圆圈（重贴边）；拖离边缘松手 → 恢复完整面板
8. 圆圈状态下托盘「打开」/ 托盘「添加任务」/ 再次启动应用（第二实例）→ 都恢复完整面板
9. 快速添加到 320 后关闭 → 恢复正常高度；随后重启应用 → 高度仍是正常值（不是 320）
10. 圆圈状态下重启 → 仍为圆圈；关闭显示器/切换缩放后再重启 → 回退完整面板且可见
11. 圆圈状态下有任务到期 → 系统通知照常弹出（后台提醒不受形态影响）

> 注意：本机显示器为 150% 缩放时，40 逻辑像素圆圈的实际物理边长约 60px，
> `GetWindowRect` 读到的是物理像素，验收时不要按 40px 物理像素判断失败。



## 窗口通信与失败回滚修复验收（2026-10-27）

本节记录本轮修复后的证据；上方 97/111 项记录属于修复前历史快照。

已修复：

- 生产 `generate_handler!` 补注册 `widget_expand`、`widget_collapse`、
  `widget_quick_add_closed`、`widget_get_mode`。
- 展开、收起、贴边收纳、重新贴边、重启恢复与临时高度恢复共用原生操作提交边界。
  原生设置后查询实际尺寸/位置/可缩放属性，确认成功后单事务保存正常几何、模式和锚点，
  最后提交托管状态并广播；失败尽力回滚每个原生属性并返回错误。
- 转换与几何防抖保存互斥；回滚失败后暂停几何自动保存，直到一次转换成功，
  避免把异常小尺寸覆盖正常展开尺寸。临时高度仍不作为正常高度落库。
- 前端取消假展开回退，圆圈失败后保留计数与重试入口；快速添加等待调高成功。
  先订阅模式事件再读取启动快照，旧快照/旧命令响应不能覆盖更新的后端模式事件。
  同一 WebView 的模式命令按请求顺序提交，卸载后不更新组件。

自动验证：

| 检查 | 本轮结果 |
| --- | --- |
| `npm run typecheck` | 通过 |
| `npm test` | 11 文件、104 项通过 |
| `cargo test --locked --lib`（在 src-tauri 目录） | 117 项通过 |
| `cargo check --locked --all-targets`（在 src-tauri 目录） | 通过 |
| `npm run build` | 通过 |

新增回归覆盖逐个原生步骤失败（展开/收纳/收起/临时高度恢复）、系统受理但实际尺寸不符、
数据库提交失败、回滚失败诊断、SQLite 批量键中途失败的整体回滚，以及前端失败重试和异步竞态。
`src/services/__tests__/commandRegistration.test.ts` 直接读取生产入口与前端 API，
检查前端命令是否均出现在实际注册列表；这是跨语言静态契约检查，**并非真实 WebView IPC 测试**。

本轮没有运行安装版或操作真实任务数据库，没有取得新的原生拖动、混合 DPI、通知横幅、
睡眠恢复等实机证据；上方人工验收清单仍须执行，不得仅凭自动测试将其标记通过。
构建产物的时间、大小与 SHA256 以 `docs/RELEASE.md` 的本轮候选包记录为准。

Git 状态另有阻断：现有 `.git` 缺失引用目录及 reflog 指向的 HEAD 提交对象，
`git status` 无法识别仓库。源码修复前已备份工作区源码和现有 Git 元数据；
本轮没有执行 reset、覆盖式初始化、提交或推送。此异常需在发布前独立恢复并核验。


## 拖动收纳后续修复（2026-10-27）

旧版 Moved + 按键轮询已替换为 Windows 原生移动/缩放会话监听，标题文字和圆圈数字的拖动命中已修正，
收起条也支持收纳。最新自动检查为前端 104 项、Rust 125 项，全部通过。
本轮已取得隔离 Tauri/WebView2 的原生收纳、单击恢复、收起态收纳、重启恢复和移离边缘恢复证据。
移动会话通过 Windows 系统菜单及键盘完成；普通鼠标四边拖动和混合 DPI 仍待复测。
详细步骤、真实几何、截图与限制见 [拖动修复验收记录](DRAG_FIX_2026-10-27.md)。


## 2026-10-28 本机安装版验收

验收发现圆圈拖动后误展开，已修复并新增回归。当前前端 106 项、Rust 125 项通过，最终 NSIS 包已安装。
本机本轮功能验收通过；输入方法、截图、业务数据保护和未覆盖项见 [本轮验收记录](ACCEPTANCE_2026-10-28.md)。
