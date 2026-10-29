# CheckIt

> Check it. Get it done.
>
> 让任务始终清晰，让截止时间不再被遗忘。

**CheckIt** 是一款轻量、美观、可靠的 Windows 玻璃态桌面任务管理与截止时间提醒工具。以 Deadline 为中心设计：快速记录任务，桌面悬浮小组件持续显示倒计时，多级提醒确保截止时间不再被遗忘。

![技术栈](https://img.shields.io/badge/Tauri%202-React%2019-TypeScript-blue) ![平台](https://img.shields.io/badge/Platform-Windows%2010%2F11-lightgrey) ![许可证](https://img.shields.io/badge/License-MIT-green)

## 下载与安装（Windows 用户）

**[直接下载 v0.2.2 Windows 安装包](https://github.com/Zane-up/CheckIt/releases/download/v0.2.2/CheckIt_0.2.2_x64-setup.exe)** · [查看最新版与更新说明](https://github.com/Zane-up/CheckIt/releases/latest)

支持 Windows 10/11 64 位。下载 `.exe` 后双击，按向导安装即可，普通用户无需安装 Node.js、Rust 或开发工具。若从版本页面下载，请展开 **Assets** 并选择 `.exe` 安装包，`Source code` 是开发源码。

本次优化详情见 [v0.2.2 更新记录](CHANGELOG.md)。

## 功能特性

- **桌面悬浮小组件**：无边框玻璃拟态窗口，可拖动、可缩放、可置顶、可收起，位置与大小自动记忆
- **任务管理**：新增 / 编辑 / 删除 / 完成 / 恢复，支持分类、优先级、截止时间
- **动态倒计时**：按剩余时长自适应展示（天/小时/分钟/秒），紧急程度颜色提示，逾期标记
- **多级提醒**：每个任务可配置多个提醒（提前 1 天 / 3 小时 / 1 小时 / 30 分钟 / 10 分钟 / 截止时 / 自定义）
- **可靠调度**：Rust 后台调度器独立于窗口运行，快照键去重 + 状态机重试，重启与休眠恢复后按 15 分钟宽限补查；
  每条提醒提交前重新取时校验，状态时间戳使用真实完成时刻
- **单实例保护**：重复启动会唤醒已有悬浮窗，第二实例在数据库迁移与调度器启动前退出
- **Windows 原生通知**：基于系统通知中心，不再依赖网页弹窗
- **屏幕居中提醒（v0.2.0）**：设置 → 提醒中开启“屏幕居中弹窗提醒”，可单独使用或与系统通知同时开启；支持无任务预览、多条提醒合并、确认关闭，以及未确认提醒重启恢复。旧版升级后默认关闭此选项
- **系统托盘**：关闭窗口可驻留后台继续提醒，托盘菜单快速操作
- **SQLite 持久化**：全部数据本地存储于系统应用数据目录，完全离线可用
- **深浅色主题**：支持深色、浅色与跟随系统；管理窗口玻璃效果和不透明度可调，悬浮面板使用不透明背景保证清晰圆角

设置页底部可手动“检查更新”或打开官方下载页。更新需自行运行新版安装包，覆盖安装保留本机任务；不会自动下载或安装。检查更新需要访问 GitHub，任务管理与提醒仍可离线使用。

## 技术栈

| 层 | 技术 |
| --- | --- |
| 桌面框架 | Tauri 2（Rust） |
| 前端 | React 19 + TypeScript（strict） |
| 构建 | Vite 8 |
| 样式 | Tailwind CSS 4 |
| 数据库 | SQLite（rusqlite，WAL 模式） |
| 通知 / 自启动 | notify-rust（同步 WinRT 提交）/ tauri-plugin-autostart / tauri-plugin-single-instance |
| 测试 | Vitest（前端） + Rust 内置测试（后端） |
| CI | GitHub Actions |

## 开发环境要求

仅在从源码开发或构建 CheckIt 时需要：

1. **Node.js** ≥ 22.12（Vite 8 / Vitest 5 的最低要求，推荐 22 LTS）
2. **Rust** stable（MSVC 工具链）—— 通过 [rustup](https://rustup.rs) 安装
3. **Microsoft Visual Studio C++ Build Tools**（MSVC v143 + Windows 10/11 SDK）

## 开发运行

```bash
# 安装前端依赖
npm install

# 以开发模式启动桌面应用（悬浮小组件窗口）
npm run tauri dev
```

## 运行测试

```bash
npm run typecheck        # TypeScript 严格类型检查
npm run test             # 前端单元测试（Vitest）
npm run build            # 前端生产构建（含类型检查）

cd src-tauri
cargo test               # Rust 单元 / 集成测试（数据库、提醒引擎）
cargo check              # Rust 编译检查
```

## 构建安装包

```bash
npm run tauri build
```

当前源码的构建产物位于 `src-tauri/target/release/bundle/nsis/CheckIt_0.2.2_x64-setup.exe`。

## 安装与运行

1. 双击 `CheckIt_0.2.0_x64-setup.exe`，按向导完成安装（当前用户安装，无需管理员权限）。
2. 启动后桌面出现 CheckIt 悬浮小组件，可拖动到任意位置。
3. 点击标题栏 `+` 快速添加任务；点击「全部任务」打开完整管理窗口。
4. 关闭悬浮窗默认隐藏到系统托盘，提醒继续生效；可在设置中改为"关闭即退出"。

数据目录：`%APPDATA%\com.checkit.desktop\checkit.db`（SQLite，可随时备份）。

## 项目结构

```
CheckIt/
├── src/                        # 前端（React + TS）
│   ├── components/
│   │   ├── widget/             # 悬浮小组件（头部/状态/列表/快速添加/菜单）
│   │   ├── manager/            # 管理窗口（列表/表单/分类/设置）
│   │   ├── shared/             # 共享组件（任务项/倒计时徽章）
│   │   └── ui/                 # 基础组件（按钮/复选框/模态框）
│   ├── hooks/                  # 全局时钟 / 数据上下文 / 主题
│   ├── services/               # Tauri 命令封装（唯一通信入口）
│   ├── types/                  # 与 Rust 模型对应的类型
│   ├── utils/                  # 倒计时 / 时间 / 过滤排序 / 提醒预设（含测试）
│   ├── i18n/                   # 界面文本（简体中文，预留国际化）
│   └── styles/                 # 设计令牌与玻璃态样式
├── src-tauri/                  # Rust 后端
│   └── src/
│       ├── main.rs             # 入口
│       ├── lib.rs              # 组装：插件/托盘/调度器/命令
│       ├── commands.rs         # Tauri 命令层（写后广播同步）
│       ├── database/           # SQLite：迁移/CRUD/测试
│       ├── models/             # 数据模型（serde）
│       ├── reminders/          # 提醒引擎（状态机/快照去重/宽限恢复，含测试）
│       ├── notification.rs     # 通知传输适配器（同步提交结果，可注入测试）
│       ├── windows.rs          # 窗口管理（几何持久化/玻璃效果）
│       ├── tray.rs             # 系统托盘
│       └── settings.rs         # 开机启动同步
├── docs/                       # 开发文档（PRD/架构/数据库/测试/发布…）
├── scripts/                    # 图标生成等工具脚本
└── .github/workflows/          # CI：测试 + Windows 构建产物
```

## 设计与开发文档

详见 [docs/](docs/)：[PRD](docs/PRD.md) · [架构](docs/ARCHITECTURE.md) · [数据库](docs/DATABASE.md) · [测试指南](docs/TESTING.md) · [技术决策](docs/DECISIONS.md) · [路线图](docs/ROADMAP.md) · [发布说明](docs/RELEASE.md)

## 许可证

[MIT](LICENSE)
