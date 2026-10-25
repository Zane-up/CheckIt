# TESTING — 测试指南

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
| 11 | 提醒去重 | `idempotent_no_double_fire` | 同上 |
| 12 | 修改截止时间重算 | `due_change_recomputes_reminders` | 同上 |
| 13 | 完成后停止提醒 | `completed_task_stops_reminders` | 同上 |
| 14 | 重启数据恢复 | `restart_recovers_data_from_file_db` | database/tests.rs |
| 15 | 睡眠恢复补查 | `missed_reminder_beyond_grace_is_silent` / `missed_within_grace_still_notifies` / `restart_recovers_pending_reminders` | reminders |
| 16 | 窗口设置持久化 | `window_geometry_roundtrip` / `settings_roundtrip` | database/tests.rs |

## 命令

```bash
# 前端
npm run typecheck          # tsc --noEmit（strict）
npm run test               # vitest run
npm run build              # 类型检查 + 生产构建

# Rust（在 src-tauri 下）
cargo test                 # 全部单元/集成测试
cargo check                # 编译检查
```

## 需要人工实机验证的项目（CI 无法覆盖）

1. 悬浮窗拖动 / 缩放手感与多显示器位置恢复
2. Windows 11 Acrylic 玻璃效果渲染质量与降级
3. **通知横幅的视觉显示**：应用已调用系统通知 API（fired_at 证据），但横幅是否弹出受系统"专注助手"/通知权限设置影响，截图窗口期未捕捉到横幅（可能已进入通知中心）——需人工确认系统通知设置后复测
4. 系统休眠唤醒后的实际补查表现
5. 托盘图标显示与菜单交互
6. 开机启动真实生效
7. 安装包安装 / 卸载 / 数据保留
8. 长时间驻留的内存/CPU 表现

## 已完成的实机验证（2026-10-25，Windows 11，构建产物 checkit.exe）

以下通过 OS 级真实鼠标键盘输入（Win32 SendInput）与数据库内容核验完成，非自动化断言：

1. ✅ 应用启动：进程存活（内存 ~30MB），SQLite 自动创建于 `%APPDATA%\com.checkit.desktop\`（WAL 模式），默认分类自动播种
2. ✅ 悬浮窗真实渲染：玻璃态背景模糊、状态概览、任务列表、倒计时（逐秒刷新）、逾期红色标记、优先级指示条、底部进度条
3. ✅ 真实点击 + 按钮 → 快速添加面板打开
4. ✅ 真实键盘输入 + Enter → 任务创建（数据库核验），面板自动关闭，待办统计 1→2 实时更新
5. ✅ 真实点击「全部任务」→ 管理窗口打开（async 修复后），侧栏（全部/今日/未来7天/已完成/分类列表/设置）完整渲染，任务列表显示真实数据
6. ✅ 管理窗口编辑任务 → 修改落库 → 悬浮窗同步刷新（data-changed 广播生效）
7. ✅ 管理窗口删除流程实机全链路：双击任务行 → 编辑弹窗 → 真实点击"删除" → 原生确认框 Enter → 管理窗口与悬浮窗同时清零（data-changed 双窗口同步）→ 重启后 DB/WAL 均无该任务（删除持久化）
8. ✅ 提醒调度端到端：创建 40 秒后截止 + 截止时提醒的任务 → reminder_events 的 fired_at（15:21:27.278Z）与截止时刻精确吻合 —— 调度器在截止当秒执行了通知发送调用（notification.show()）
9. ✅ IPC 稳定性：CDP 探测 t0/10s/60s/180s 全部 5-8ms 响应；通知触发后持续稳定
10. ✅ 空态/超时等边界：空任务列表空态文案正常

### 已知测试工具限制（非应用缺陷）

- 附加 DevTools 协议（--remote-debugging-port）时，页面上下文的 invoke 响应可能不送达（命令本身执行成功）——调试器干扰，无调试器时正常（真实输入路径验证面板正常关闭）
- SendKeys 输入会被微软拼音输入法组合，首个 Enter 被用于确认候选词——测试脚本需双 Enter 或剪贴板粘贴

## 已知边界（如实说明）

- 应用真正退出或电脑关机期间的提醒无法送达（进程不存在）；重启后在 15 分钟宽限内的会补发，超出的静默标记
- Windows 通知的"点击聚焦"依赖插件默认行为，个性化跳转（如直接打开任务详情）未在 v0.1.0 实现
