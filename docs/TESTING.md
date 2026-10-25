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
3. 系统通知弹出、点击聚焦行为（依赖系统通知权限）
4. 系统休眠唤醒后的实际补查表现
5. 托盘图标显示与菜单交互
6. 开机启动真实生效
7. 安装包安装 / 卸载 / 数据保留
8. 长时间驻留的内存/CPU 表现

## 已知边界（如实说明）

- 应用真正退出或电脑关机期间的提醒无法送达（进程不存在）；重启后在 15 分钟宽限内的会补发，超出的静默标记
- Windows 通知的"点击聚焦"依赖插件默认行为，个性化跳转（如直接打开任务详情）未在 v0.1.0 实现
