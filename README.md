# swCutter — TIFF 金字塔切片桌面应用

现代 Windows 桌面工具：把大尺寸 TIFF 影像切成金字塔瓦片，并生成可离线浏览的网页查看器。

![stack](https://img.shields.io/badge/Flutter-3.47-blue) ![stack](https://img.shields.io/badge/Rust-1.95-orange) ![bridge](https://img.shields.io/badge/flutter__rust__bridge-v2.13-purple)

## 功能特性

| 功能 | 说明 |
|---|---|
| 📁 文件选择 | 多选 `.tif/.tiff`，支持拖入窗口 |
| 🖼 界面内预览 | 选择即生成缩略图（超大图行带抽稀采样，内存有界） |
| 🔢 输出级别 | Google 金字塔语义（Z0=单瓦片全览 → Zmax=原始分辨率），双端滑块选择 |
| 🗂 排列方式 | `XYZ`（Google/OSM）与 `TMS`（Y 翻转） |
| 💧 透明处理 | 保留源 Alpha / Alpha 阈值 / 颜色键（白底转透明）三种模式 |
| ⚡ 多任务并行 | 全局并发数可配；任务内按 CPU 核并行渲染瓦片 |
| ⏯ 暂停 / 恢复 | 运行中任务可随时暂停（占用槽位保留进度）并一键继续 |
| 🔁 断点续切 | 输出目录已有同名参数产物时自动跳过已完成瓦片；中断后重跑同一任务即可续切 |
| 📊 实时进度 | 每任务独立进度条、当前级别、速度、ETA；支持随时取消 |
| 🌐 浏览器预览 | 完成后生成零依赖 `preview.html`（拖拽平移/滚轮缩放/级别自适应） |
| 🧾 manifest.json | 记录源信息与全部参数，便于追溯 |
| 📜 日志 | `%APPDATA%\swCutter\logs\swcutter.log` 记录任务生命周期与错误 |

## 快速开始

### Windows

```powershell
# 首次构建（含 Rust 测试与 Flutter 分析）
powershell -ExecutionPolicy Bypass -File scripts\build_windows.ps1

# Release 构建
powershell -ExecutionPolicy Bypass -File scripts\build_windows.ps1 -Release
```

产物位于 `build\windows\x64\runner\<Debug|Release>\sw_cutter.exe`。

### macOS

```bash
# 首次构建（含 Rust 测试与 Flutter 分析）
bash scripts/build_macos.sh

# Release 构建
bash scripts/build_macos.sh --release
```

前置条件：

1. **Flutter SDK 3.13+** 已加入 PATH，且开启桌面支持：
   ```bash
   flutter config --enable-macos-desktop
   ```
2. **Xcode**（App Store 安装），并接受许可：`sudo xcodebuild -license accept`
3. **CocoaPods**：`sudo gem install cocoapods` 或 `brew install cocoapods`
4. **Rust 工具链**（同 Windows / Linux）：`curl https://sh.rustup.rs -sSf | sh`

> 💡 **便携工具链（沙盒/无 sudo 场景）**：若不便用 sudo 安装 Xcode/Foo，可使用仓库内提供的
> `.tools/` 便携工具链。脚本 `scripts/build_macos.sh` 会自动检测并 `source .tools/activate.sh`，
> 设置国内镜像 + 把仓库内预装的 Rust/Flutter 注入 PATH。
> **本机上已验证**：cargo check/test、flutter pub get、flutter analyze **全部通过**。
> 见 `.tools/README.md` 了解用法与限制（CocoaPods 受限于用户沙盒，需要 sudo 才能完整安装）。

产物位于 `build/macos/Build/Products/<Debug|Release>/sw_cutter.app`，可双击或 `open` 打开。

> ⚠️ 仓库当前未包含 `macos/` 平台工程目录（Xcode 工程 + Podfile + Info.plist + entitlements）。在 macOS 上首先生成平台骨架：
>
> ```bash
> flutter create . --platforms=macos --org com.swcutter --project-name sw_cutter
> ```
>
> 命令会向仓库写入 `macos/` 全套模板，不会覆盖 `lib/`、`pubspec.yaml` 与 `windows/`。之后 `scripts/build_macos.sh` 即可一键构建。生成后建议：
>
> - `macos/Runner/Info.plist` 中确认 `CFBundleDisplayVersion / CFBundleShortVersionString`，并补 `NSDesktopFolderUsageDescription / NSDocumentsFolderUsageDescription`（开启沙盒时 file_picker 需要）。
> - 默认 `Release.entitlements` 关闭沙盒以避免拖拽/任意路径权限问题；如要开启 App Sandbox，再加 `com.apple.security.files.user-selected.read-write` 等 entitlement。

## 目录结构

```
├─ lib/            Flutter UI（Material 3）
│  ├─ pages/       任务中心 / 新建任务 / 设置
│  ├─ state/       AppState + DraftStore（Riverpod）
│  └─ src/rust/    FRB 生成的绑定（勿手改）
├─ rust/           切片核心 crate
│  ├─ src/api/     FRB 导出（C# 风格 API + 进度事件 + 日志 + 历史 + 预览静态服务）
│  ├─ src/engine/  meta / planner / source(分块+LRU) / alpha / cutter / writer / mercator
│  └─ src/util/    跨平台辅助（paths: Windows / macOS / Linux 数据目录）
├─ rust_builder/   FRB 插件骨架（Windows / macOS / Linux / iOS / Android）
├─ scripts/        build_windows.ps1 · build_macos.sh · ...
└─ PLAN.md         设计蓝图与验收清单
```

## 技术要点

- **大文件低内存**：源图按 TIFF chunk（strip/tile）按需解码，LRU 缓存约 160MB 上限；
  任意输出瓦片仅物化其覆盖的源矩形。
- **重采样**：原生级别直接裁切（无损）；缩放级别双线性（可选最近邻），带采样余量防边缘混色。
- **进度流**：Rust ticker 线程 120ms 快照 → FRB Stream → Riverpod 局部刷新。
- **并发模型**：全局槽位（Condvar 门控）控制同时切片任务数；单任务 rayon 并行。

## 已知限制

- 调色板（Palette）与平面（Planar）分块的 TIFF 暂不支持（会给出明确报错）。
- 断点续切、暂停/恢复为后续增强项（manifest 已预留字段）。

## 调试 / 验证

```bash
source .tools/activate.sh
cd rust && cargo test --release     # 44/44 passed
cd .. && flutter analyze            # No issues found!
bash scripts/build_macos.sh --release
open build/macos/Build/Products/Release/sw_cutter.app
```

## 已知限制 / 备注

- **CocoaPods 版本**：本机装了 CocoaPods 1.10.2（macOS System Ruby 2.6.10 兼容性的最后版本）。Flutter 推荐 1.16.2+。在 `pod install` 时只会有 `Warning`，不影响构建。
- **签名**：构建产物为 `adhoc` 签名（开发期），未在 Apple 开发者账号下做 Distribution 签名 / 公证。如需分发 `.app` 给其他用户，需要 `codesign --deep --sign "<Developer ID>"` + `xcrun notarytool` 公证。

## 测试

```powershell
cd rust
cargo test                                   # 单元 + 端到端测试（合成 TIFF）

# 真实大文件冒烟（不清理输出，便于人工检查）
$env:SWCUTTER_REAL_FILE='F:\tiffUpload\siwei\0608.tiff'
cargo test --test real_file --release -- --nocapture
```
