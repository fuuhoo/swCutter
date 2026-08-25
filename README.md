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
| 📊 实时进度 | 每任务独立进度条、当前级别、速度、ETA；支持随时取消 |
| 🌐 浏览器预览 | 完成后生成零依赖 `preview.html`（拖拽平移/滚轮缩放/级别自适应） |
| 🧾 manifest.json | 记录源信息与全部参数，便于追溯 |

## 快速开始

```powershell
# 首次构建（含 Rust 测试与 Flutter 分析）
powershell -ExecutionPolicy Bypass -File scripts\build_windows.ps1

# Release 构建
powershell -ExecutionPolicy Bypass -File scripts\build_windows.ps1 -Release
```

产物位于 `build\windows\x64\runner\<Debug|Release>\sw_cutter.exe`。

## 目录结构

```
├─ lib/            Flutter UI（Material 3）
│  ├─ pages/       任务中心 / 新建任务 / 设置
│  ├─ state/       AppState + DraftStore（Riverpod）
│  └─ src/rust/    FRB 生成的绑定（勿手改）
├─ rust/           切片核心 crate
│  └─ src/engine/  meta / planner / source(分块+LRU) / alpha / cutter / writer
├─ scripts/        install_flutter.ps1 · build_windows.ps1
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

## 测试

```powershell
cd rust
cargo test                                   # 单元 + 端到端测试（合成 TIFF）

# 真实大文件冒烟（不清理输出，便于人工检查）
$env:SWCUTTER_REAL_FILE='F:\tiffUpload\siwei\0608.tiff'
cargo test --test real_file --release -- --nocapture
```
