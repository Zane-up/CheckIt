# RELEASE — 发布说明与流程

## 版本策略

语义化版本（MAJOR.MINOR.PATCH）。当前：v0.1.0。

## 发布前检查清单

状态说明：`[x]` 有证据通过；`[ ]` 未完成/待验证；不得为了发布勾选未验证项。

1. [x] `npm run typecheck` 通过（2026-10-27，最终源码，含自定义提醒提前量功能后复跑）
2. [x] `npm run test` 全部通过（9 文件 / 80 用例）
3. [x] `cd src-tauri && cargo test` 全部通过（73 用例，含自定义提醒持久化与 60 天边界 3 项）
4. [x] `cd src-tauri && cargo check --all-targets` 通过
5. [x] `npm run build` 前端生产构建通过
6. [x] `npm run tauri build` 成功产出 NSIS 安装包（2026-10-27 09:00:44 完成，为当前验收对象）
7. [ ] Windows 实机冒烟测试：**大部分通过，仍有待验证项**
   - 已通过（09:00 包安装版，正常交互登录账户，2026-10-27 上午，证据见 `docs/TESTING.md`
     「安装版实机验收」A1–A10）：安装与启动（迁移 v1→v3）、任务创建与落库、
     自定义提醒保存与回显（UI 标签/移除按钮 + 数据库秒值 86400/1500）、默认提醒继承语义、
     提醒实际触发与通知提交（submitted + wpndatabase 同秒登记 + 诊断日志无错误）、
     单实例第二实例退出与窗口唤醒、托盘菜单正常退出、正常退出重启去重、验收记录清理
   - 已通过（15:08 旧包，同代码路径的调度器/通知基础设施）：关闭窗口后台提醒、
     关闭期间到期重启补发、安装态 AUMID/快捷方式、v2→v3 迁移、卸载/重装数据保留
   - 已通过（隔离环境）：重启去重（提交 1 次，重启后 0 次新增）
   - 已通过（浏览器预览）：全应用文字可读性（对比度核算全部必需文字 ≥ 4.5:1，
     深/浅主题、玻璃开/关、透明度 0.05/0.35/0.6、白/灰/黑/复杂壁纸、340×440 与 300×200 布局）；
     主按钮独立令牌对比度（4 处主按钮 × 深/浅 × 默认/悬停/按下 24 组全部 ≥ 4.5:1，
     真实渲染计算样式 + 像素采样复核，截图 `docs/screenshots/button-contrast-2026-10-26/`）；
     自定义提醒提前量界面（任务编辑与默认提醒设置，深/浅主题，截图 `docs/screenshots/custom-reminder-*.png`）
   - 待验证：通知横幅目视可见性（本轮已取得 submitted + wpndatabase 前两层证据）、
     关闭到托盘后台提醒独立复测、任务完成/恢复 UI 操作、设置页默认提醒 UI 独立操作、
     真实睡眠唤醒、开机启动登录后自启、旧版本安装包覆盖升级、托盘菜单逐项目视
     （本轮已验证「任务管理」「退出」两项）、原生 Acrylic 合成、
     真实 125%/150% 缩放与混合 DPI、长时间驻留资源占用、同版本卸载/重装后已提交提醒状态保留
   - 详见 `docs/TESTING.md`
8. [x] 更新 CHANGELOG.md
9. [x] 版本号三处同步：`package.json` / `src-tauri/Cargo.toml` / `src-tauri/tauri.conf.json` = 0.1.0
10. [ ] 第三方依赖许可证复核（已完成机器清单采集，**未人工逐项确认**，不得勾选通过）

## 当前验收安装包（2026-10-27 09:00:44 重建，已安装并实机验收）

- 路径：`src-tauri/target/release/bundle/nsis/CheckIt_0.1.0_x64-setup.exe`
- 构建时间：2026-10-27 09:00:44（晚于本轮全部源码修改，含自定义提醒提前量功能）
- 大小：2,015,054 字节
- SHA256：`AF5AE97701E0BE44E5548434C626A643A946D841CA913C04CF8415E9F263C697`
- 可执行文件（构建产物）：`src-tauri/target/release/checkit.exe` 5,023,232 字节，
  SHA256 `DF8D2A0CFF5C77AAE6D7B3CF8EC4B3B9877881D5D796FAC2DF79C5276D7BBE96`
- 可执行文件（安装目录）：`%LOCALAPPDATA%\CheckIt\checkit.exe` 5,023,232 字节，
  SHA256 `212B1F79D024630A3B8879E7FEF83EC195992CC321205CB63C362886DF81B893`
  （与构建产物仅 3 字节差异：NSIS 安装版重写 bundle 类型标记 `UNK`→`NSS`，预期现象）
- 备注：本包已于 2026-10-27 09:00–09:04 安装并启动，实机验收 A1–A10 通过
  （详见 `docs/TESTING.md`）；00:59:24 旧包（2,015,910 字节 / `7A8B498B…038DA3`）与
  2026-10-26 21:47 包（`B9F45C05…E49A09`，2,015,097 字节）均被本包超越，不得作为验收对象。
  相对已实机验收的 15:08 包差异：`diagnostics.rs` 两个日志边界修复 + 全应用文字可读性调整
  + 主按钮对比度令牌修复 + 自定义提醒提前量功能（无数据库 schema 变更、无提醒调度器行为变更、无依赖升级）

## 依赖许可证复核（进行中）

机器可读清单已采集（2026-10-26）：

- Rust（`cargo metadata --locked`，共 500+ 包）：全部为 MIT / Apache-2.0 / BSD / ISC / Zlib /
  MPL-2.0 / Unicode-3.0 等宽松或双许可；无 GPL-only / AGPL / SSPL。
  `r-efi` 为 `MIT OR Apache-2.0 OR LGPL-2.1-or-later`，可按 MIT/Apache-2.0 使用。
- 前端（`package-lock.json`）：MIT 118、MPL-2.0 24、Apache-2.0 系 20、ISC 4、BSD 4、CC0/MIT-0/BlueOak 各 1；
  无 GPL/AGPL。

**尚未逐项人工确认**（尤其 MPL-2.0 的文件级 copyleft 义务、NOTICE 归属），因此清单第 10 项保持未勾选。

## 构建 Windows 安装包

```bash
npm install
npm run tauri build
```

产物：
- 安装包：`src-tauri/target/release/bundle/nsis/CheckIt_<version>_x64-setup.exe`
- 可执行文件：`src-tauri/target/release/checkit.exe`

NSIS 配置：currentUser 安装（免管理员）、简中/英文界面。

## GitHub 发布流程

1. `git tag vX.Y.Z && git push origin vX.Y.Z`
2. GitHub Actions 工作流（`.github/workflows/release.yml`）在 tag push 时自动：
   - windows-latest runner 构建安装包
   - 上传构建产物到 Release 草稿
3. 补充 Release Notes（复制 CHANGELOG 对应段落）后发布。

> 注意：不自动发布 Release；由维护者人工确认草稿内容后手动 Publish。

## 已知限制（发布说明中如实声明）

- v0.1.0 仅保证 Windows 10/11 x64；未在 macOS/Linux 测试
- 应用未签名（SmartScreen 可能提示"更多信息 → 仍要运行"）
- 应用退出 / 关机期间的提醒无法送达；重启后 15 分钟宽限内的错过的提醒会补发
- 通知状态 `submitted` 仅代表系统通知 API 已受理，不保证用户实际看到横幅；
  安装态 AUMID 与快捷方式注册已于 2026-10-26 在安装包上核验一致，
  横幅视觉可见性（专注助手/通知横幅设置）仍需人工目视确认
- 通知为 at-least-once：极端崩溃窗口内可能重复提交一次，不承诺 exactly-once；
  已开始的系统通知调用无法撤回
- 诊断日志保留策略：活动文件 `checkit.log` 1 MiB，另保留 `checkit.log.1`–`.3` 三个归档，
  磁盘占用约 4 MiB 封顶；仅记录时间/级别/来源/错误文本，不含任务标题等业务内容
- 产品采用单实例：重复启动会唤醒已有窗口；提醒引擎不支持多个调度器并行，
  绕过单实例保护可能导致重复领取/提交。崩溃窗口内的重复提交边界仍存在
