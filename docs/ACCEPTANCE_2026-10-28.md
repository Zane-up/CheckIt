# 2026-10-28 本机验收记录

本轮针对贴边收纳与圆圈交互的功能验收通过，代码可以上传 GitHub。
验证环境为 Windows、单显示器、150% 缩放；这不代表混合 DPI、真实睡眠恢复等全部发布项目已验收。
本次上传源代码和测试资料，不创建正式版本 tag 或 GitHub Release。

## 验收中发现并修复的问题

旧版圆圈在拖动后会立即误展开。原实现一按下就启动原生拖动，同时在 pointerup 时清除拖动标记，
随后合成 click 无条件执行展开。修复后先识别超过 4 逻辑像素的位移，再调用原生 startDragging；
拖动标记保留到下一次 pointerdown，拦截同一手势补发的 click，键盘激活仍可正常展开。
新增回归覆盖重复合成点击、pointercancel、后续单击和键盘激活。

## 验证范围与结果

| 项目 | 方法及结果 |
| --- | --- |
| 安装升级 | NSIS 静默升级成功，最终安装路径及二进制已核对 |
| 右侧贴边收纳 | 安装版鼠标输入完成原生移动，310×900 面板变成 40×40 圆圈 |
| 圆圈拖动不误展开 | 最终安装版单次鼠标拖动手势后保持圆圈；随后独立单击正常展开 |
| 沿边移动、移离边缘 | 隔离调试版鼠标输入验证沿右边移动后保持圆圈、移离后恢复完整面板 |
| 顶部、底部、左侧收起条 | 隔离版通过 Windows 系统菜单的原生移动会话逐项验证为 40×40 |
| 角落及途中暂停 | 移动会话暂停超过 2 秒仍保持面板；结束后按稳定规则收纳到左边 |
| 重启恢复 | 最终安装版圆圈状态终止进程并重启，仍为 40×40，计数保持正确 |
| 单击恢复原尺寸 | 最终安装版由圆圈恢复 310×900；隔离版恢复 340×440 |
| 正常尺寸持久化 | 收起条收纳后隔离库正常高度仍为 660 物理像素（440 逻辑像素） |
| 业务数据保护 | tasks、categories、reminders 与验收前一致性备份逐行比对完全相同 |
| Git 恢复 | 从远端完整对象库补齐缺失对象和 main/origin 引用，未重置工作区；完整性检查通过 |

鼠标工具的快速拖动调用可能在原生窗口接管之前就已松开；补充的一次独立点击不能一概当成拖动结束。
记录已区分鼠标输入与系统菜单/键盘原生移动。没有把键盘移动证据宣称为四边鼠标全过程覆盖。

## 自动检查

- 前端：11 文件、106 项通过。
- Rust：125 项通过。
- 类型检查、Rust 全部构建目标检查、生产构建、NSIS 打包通过。

## 当前安装包

- 构建时间：2026-10-28 01:04:30（Asia/Shanghai）
- 路径：`src-tauri/target/release/bundle/nsis/CheckIt_0.1.0_x64-setup-dragfix-20261028.exe`
- 大小：2,037,176 字节；SHA256：`4E7EA96E4D26546CA1E86C3603312BB0D7D7DF69E7DB4D213006C354A0D0B4A4`
- 构建 EXE SHA256：`784128A636460942E195C1FC84605BDC8A096F253AB7880F831F3B6BFEF36F31`
- 安装 EXE SHA256：`538F2D807213FFC76D3A40ED1BBCE6B00EB7564E4A5F8B1F2C2FAA78C4055137`
- 安装后与构建 EXE 仅 3 字节 bundle 类型标记不同，长度相同。

## 公开验收资料

公开截图来自无真实任务数据的隔离应用 `com.checkit.docktest`：

- [拖动保持圆圈](screenshots/acceptance-2026-10-28/bubble-drag-keeps-docked.jpg)
- [顶部收纳](screenshots/acceptance-2026-10-28/native-top.jpg)
- [底部收纳](screenshots/acceptance-2026-10-28/native-bottom.jpg)
- [收起条左侧收纳](screenshots/acceptance-2026-10-28/native-collapsed-left.jpg)
- [角落收纳](screenshots/acceptance-2026-10-28/native-corner.jpg)
- [重启保持圆圈](screenshots/acceptance-2026-10-28/restart-docked.jpg)
- [单击恢复尺寸](screenshots/acceptance-2026-10-28/click-restores-original-size.jpg)

未上传业务数据库、个人桌面截图、本地缓存或原始诊断日志。
混合 DPI、显示器插拔、真实睡眠唤醒等仍按发布清单保留待验证，不能据本轮记录勾选为通过。
