# RELEASE — 发布说明与流程

## 当前发布流程（v0.5.0 起）

仅使用 `.github/workflows/ci.yml`。主分支推送和 PR 执行检查；主分支推送另构建安装包，版本标签推送则在前端、Rust 检查和构建全部通过后自动公开 Release。

发布步骤：更新 package.json、package-lock.json、Cargo.toml、Cargo.lock、tauri.conf.json 中的应用版本，补充 CHANGELOG 对应版本，提交源码并推送 main；验收通过后创建并推送同一提交的 `vX.Y.Z` 标签。不要提前使用 `gh release create` 或网页创建 Release，也无需取消任何重复发布任务。

发布任务从同一流水线下载安装包，核对标签和应用版本，生成 SHA256SUMS.txt，提取更新日志，上传成功后公开发布。任何检查失败都不会进入发布步骤；任务失败可在修复后按实际提交重新发版，不替换已公开的版本文件。

以下记录为历史版本验收，不代表当前版本的验收状态。

## 版本策略

语义化版本（MAJOR.MINOR.PATCH）。当前：v0.1.0。

## 发布前检查清单

状态说明：`[x]` 有证据通过；`[ ]` 未完成/待验证；不得为了发布勾选未验证项。

1. [x] `npm run typecheck` 通过（2026-10-28，复核修复后复跑）
2. [x] `npm test` 全部通过（11 文件 / 110 用例）
3. [x] `cd src-tauri && cargo test --locked --lib` 全部通过（128 用例，含快速添加状态与收起条展开位置回归）
4. [x] `cd src-tauri && cargo check --locked --all-targets` 通过
5. [x] `npm run build` 前端生产构建通过
6. [x] Tauri 生产构建成功产出 NSIS 安装包（2026-10-28 13:27:06），已覆盖安装并检查启动
7. [ ] Windows 完整发布验收：**本轮完成隔离原生交互验收与新安装版升级/启动检查；混合 DPI/睡眠恢复等仍待验证。详见 REVIEW_FIXES_2026-10-28.md。以下为历史证据**
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
   - 已通过（2026-10-27 下午，布局修复）：快速添加三行布局在 340/300 宽度、深/浅主题下
     日期输入与添加按钮不重叠、不裁切（浏览器真实渲染 + **安装版原生 WebView2 150% 缩放**
     + 计算样式实测：日期输入 241/201px，左内边距 8px，面板未越过底部进度栏）；
     设备缩放 1.25/1.5 下布局一致；Tab 顺序与 Enter 提交正常；
     正常展开高度 282.67 时打开快速添加临时抬到 320、关闭后恢复 282.67；
     收起态（64）打开同样抬到 320、关闭恢复 282.67（原生实测 + 回归测试）。
     证据见 `docs/TESTING.md`「快速添加布局修复」/「原生 WebView2 实机验收」与
     `docs/screenshots/quickadd-layout-2026-10-27/`
   - 待验证：通知横幅目视可见性（本轮已取得 submitted + wpndatabase 前两层证据）、
     关闭到托盘后台提醒独立复测、任务完成/恢复 UI 操作、设置页默认提醒 UI 独立操作、
     真实睡眠唤醒、开机启动登录后自启、旧版本安装包覆盖升级、托盘菜单逐项目视
     （本轮已验证「任务管理」「退出」两项）、原生 Acrylic 合成、
     真实 100%/125% 系统缩放与混合 DPI（150% 已原生实测，其余比例未切换系统设置）、
     原生日期选择器浮层、长时间驻留资源占用、同版本卸载/重装后已提交提醒状态保留、
     「贴边收纳球」四边鼠标全过程与多屏交互（本轮原生键盘移动及历史右侧鼠标证据见验收记录）
   - 详见 `docs/TESTING.md`
8. [x] 更新 CHANGELOG.md
9. [x] 版本号三处同步：`package.json` / `src-tauri/Cargo.toml` / `src-tauri/tauri.conf.json` = 0.1.0
10. [ ] 第三方依赖许可证复核（已完成机器清单采集，**未人工逐项确认**，不得勾选通过）

## 当前候选包（2026-10-28 13:27:06，复核问题修复）

- 标准包：`src-tauri/target/release/bundle/nsis/CheckIt_0.1.0_x64-setup.exe`
- 同内容副本：`CheckIt_0.1.0_x64-setup-reviewfix-20261028.exe`
- 大小：2,037,712 字节；SHA256 `A068400B58F2F89A343D95F06D18FCBE04217067FABEF46FCDDF7CE95BC40F62`
- 已覆盖安装并恢复原圆圈模式与计数；110 项前端、128 项 Rust 测试通过。
- [修复、原生交互验收、安装与数据比对记录](REVIEW_FIXES_2026-10-28.md)。本轮上传源码与验收资料，不创建正式 Release。

## 历史候选包（2026-10-28 01:04:30，圆圈拖动与单击冲突修复）

- 标准包：`src-tauri/target/release/bundle/nsis/CheckIt_0.1.0_x64-setup.exe`
- 同内容独立文件名：`CheckIt_0.1.0_x64-setup-dragfix-20261028.exe`
- 安装包：2,037,176 字节，SHA256 `4E7EA96E4D26546CA1E86C3603312BB0D7D7DF69E7DB4D213006C354A0D0B4A4`
- 安装 EXE SHA256：`538F2D807213FFC76D3A40ED1BBCE6B00EB7564E4A5F8B1F2C2FAA78C4055137`
- 已安装，106 项前端测试、125 项 Rust 测试通过，本机本轮功能验收通过。
- [方法、范围与证据](ACCEPTANCE_2026-10-28.md)。只上传代码及验收资料，不据此创建正式 Release。
- Git 缺失对象与引用已恢复，工作区改动保留；任务数据比对未变。

## 历史候选包（2026-10-27 23:37:29，拖动收纳修复）

- 标准路径：`src-tauri/target/release/bundle/nsis/CheckIt_0.1.0_x64-setup.exe`
- 独立文件名副本：`src-tauri/target/release/bundle/nsis/CheckIt_0.1.0_x64-setup-dragfix-20261027.exe`（与标准包逐字节相同）
- 安装包大小：2,037,448 字节
- 安装包 SHA256：`C3D6175BBF0D4A5908ED9125AE712B3FAE02BE304750312AE235349776AABB32`
- 可执行文件大小：5,091,840 字节；SHA256：`3AF757BABB1F742EBF765BF0075964ECDDCED6E7FF59AFFB9DB4051FC95F95C3`
- 产品标识已核验为 `com.checkit.desktop`，不含隔离测试标识 `com.checkit.docktest`。
- 自动检查：前端 104 项、Rust 125 项、类型检查、全部 Rust 构建目标检查、生产构建通过。
- 隔离 Tauri/WebView2 原生验证通过：收纳、单击恢复、收起态收纳、重启恢复、移离边缘恢复。
  原生移动使用 Windows 系统菜单及方向键完成；普通鼠标四边拖动、混合 DPI 和本安装包的安装验收仍待完成。
- 现场运行进程仍是 12:45 历史安装版。使用本包时先从旧版托盘选择退出，再安装并重新启动。
- 依赖版本与数据库 schema 未变；本轮只补充已有 windows-sys 的 feature 声明，未修改真实任务数据、未提交或推送。
- [详细修复与原生验证证据](DRAG_FIX_2026-10-27.md)。Git 元数据异常、许可证复核等既有发布阻断仍保留。

## 历史候选包（2026-10-27 23:02:49，窗口通信与失败回滚修复，含最终恢复保护）

- 路径：`src-tauri/target/release/bundle/nsis/CheckIt_0.1.0_x64-setup.exe`
- 大小：2,035,310 字节
- SHA256：`214DE6912C9DA9D608C940C9BF666A05BC5E46B5B9A2FAD5D1A5CE127A6C2281`
- 可执行文件：`src-tauri/target/release/checkit.exe`，5,087,232 字节
- 可执行文件 SHA256：`04D7D2DA56FD36BAA9DF8572BB747EE62A418877F644CF5D794F7F8A68D795B7`
- 本包晚于本轮全部应用源码修改；构建未改动依赖、版本号、数据库 schema 或打包配置。
- 本轮未安装、未启动本包、未操作真实任务数据库；旧安装版的验收结果不能直接视为本包通过。
- 自动检查：前端 11 文件 / 104 项、Rust 117 项、类型检查、全部 Rust 构建目标检查、生产构建均通过。
- 发布判断：**可作为候选测试包，暂不建议直接正式发布**。需完成贴边/拖离、点击恢复、
  快速添加、重启恢复与混合 DPI 的安装版验收；既有睡眠恢复等待验证项和许可证复核仍保留。
  当前本地 `.git` 元数据缺失也会阻断正常 diff/提交/打 tag，需独立恢复后再走发布流程。
- 修复前备份：`C:/Users/Zane/AppData/Local/Temp/checkit-repair-backup-pzvbyang/workspace-before-fix.zip`。
  只包含本轮保存的源码、文档、配置及已有 Git 元数据，不包含真实业务数据库；没有执行自动提交或推送。

## 历史安装包记录（2026-10-27 12:45:29，已安装；不代表当前修复包）

- 路径：`src-tauri/target/release/bundle/nsis/CheckIt_0.1.0_x64-setup.exe`
- 构建时间：2026-10-27 12:45:29（晚于本轮全部源码修改，含快速添加三行布局与收起态高度回归）
- 大小：2,015,665 字节
- SHA256：`AE5F8468EB408A7D96EED7C9A1916EFEB4FF22E1F7F13B0A978C91EBA1D6F8C9`
- 可执行文件（构建产物）：`src-tauri/target/release/checkit.exe` 5,023,232 字节，
  SHA256 `7CB22E0D170E7736AE3B690D2B9E8EBFCF03BE51AF1D5FA949731EF90D1D182D`
- 可执行文件（安装目录）：`%LOCALAPPDATA%\CheckIt\checkit.exe` 5,023,232 字节，
  SHA256 `B3B245359F1B942240909FD9B3768EE605552D275C097B00C20E0A0228369412`
  （与构建产物仅 3 字节差异：NSIS 安装版重写 bundle 类型标记 `UNK`→`NSS`，预期现象）
- 备注：本包已于 2026-10-27 12:48 静默安装（ExitCode 0）；原生 WebView2（真实 150%
  系统缩放）验收通过：340/300 逻辑宽度下日期输入与添加按钮不重叠、面板不越过底部进度栏、
  左内边距实测 8px；正常高度 282.67 时打开快速添加临时抬到 320、关闭后恢复 282.67；
  收起态（64）打开同样抬到 320、关闭恢复 282.67（详见 `docs/TESTING.md`
  「原生 WebView2 实机验收」）。12:28:04 中间包（2,017,149 字节 / `E13B5F95…EB784`）、
  09:00:44 包（2,015,054 字节 / `AF5AE977…3C697`）及更早包均被本包超越，不得作为验收对象。
  相对 09:00 包的差异仅为快速添加布局与高度保障（无数据库 schema 变更、无提醒调度器行为变更、无依赖升级）。

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
