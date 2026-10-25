# RELEASE — 发布说明与流程

## 版本策略

语义化版本（MAJOR.MINOR.PATCH）。当前：v0.1.0。

## 发布前检查清单

1. [ ] `npm run typecheck` 通过
2. [ ] `npm run test` 全部通过
3. [ ] `cd src-tauri && cargo test` 全部通过
4. [ ] `npm run tauri build` 成功产出 NSIS 安装包
5. [ ] Windows 实机冒烟测试（见 TESTING.md 人工验证清单）
6. [ ] 更新 CHANGELOG.md
7. [ ] 版本号三处同步：`package.json` / `src-tauri/Cargo.toml` / `src-tauri/tauri.conf.json`
8. [ ] 第三方依赖许可证复核（全部 MIT/Apache-2.0 兼容）

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
