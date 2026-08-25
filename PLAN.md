# swCutter — TIFF 金字塔切片桌面应用 · 任务规划

> 技术栈：Flutter（Windows 桌面 UI） + Rust（切片核心），经 flutter_rust_bridge 通信。
> 本文档为执行蓝图，经用户确认后按里程碑推进。
>
> **已确认决策**：① 普通大图 TIFF（按像素坐标切片，不做地理投影）；② 排列方式支持 **XYZ 与 TMS**；③ 瓦片输出格式**仅 PNG**。

---

## 1. 目标与范围

构建一个现代感的 Windows 桌面应用，对大尺寸 TIFF 影像进行**金字塔瓦片切片**：

- 选择 TIFF 文件后可在界面内**预览**
- 可设置：输出路径、输出级别范围、瓦片排列方式、透明处理、瓦片格式
- 支持**多个切片任务并行**，每个任务独立进度条（进度/速度/ETA）
- 切片完成后可**一键跳转浏览器**，基于生成的 HTML 页面浏览各级瓦片
- 支持任务取消；完成后打开输出目录

非目标（一期）：GeoTIFF 投影重投影、分布式切片、编辑功能。

---

## 2. 环境现状（已核实）

| 组件 | 状态 |
|---|---|
| Rust 1.95.0 / Cargo | ✅ 就绪 |
| Visual Studio Community 2022（C++ 桌面工作负载） | ✅ 就绪 |
| Flutter SDK | ❌ 未安装 → M0 处理（winget 或官方 zip 安装到 `H:\dev\flutter`，启用 Windows 桌面支持） |
| 工作区 `H:\project_self\swCutter` | 空目录，直接作为项目根 |

---

## 3. 技术选型

| 层 | 选型 | 理由 |
|---|---|---|
| UI | Flutter stable + Material 3（深色为主的现代主题） | 要求指定；Windows 桌面支持成熟 |
| FFI 桥接 | **flutter_rust_bridge v2**（锁定版本） | 类型安全 API + 事件流（Stream）天然适合进度推送 |
| TIFF 解码 | Rust `tiff` crate | 纯 Rust、支持分块(chunk)读取，大文件低内存；支持常见压缩（无压缩/LZW/Deflate/PackBits） |
| 重采样与编码 | Rust `image` crate | nearest/bilinear 缩放；PNG/JPEG/WebP 编码 |
| 并行 | `rayon`（单任务内瓦片级并行） + 全局任务并发信号量 | 多核切片吞吐 |
| 运行时 | Rust 侧 `tokio`（任务管理、事件通道） | 流式进度、取消令牌 |
| 状态管理 | `flutter_riverpod` | 任务列表/设置的响应式管理 |
| 其他 Dart 包 | `file_picker`、`url_launcher`、`window_manager`、`path_provider` | 文件选择、浏览器跳转、无边框现代窗口、数据目录 |
| 浏览器预览 | 生成 `preview.html`（Leaflet 1.9 CDN） | 零额外服务，XYZ/TMS/自定义模板均可浏览 |

---

## 4. 总体架构

```
┌─────────────── Flutter (Dart) ───────────────┐
│ 任务中心页   新建任务页(表单+预览)   设置页    │
│        Riverpod 状态 / TaskStore             │
└──────────────△───────────────┬───────────────┘
        Stream<TaskEvent>      │ RustApi (FRB 生成)
┌──────────────┴───────────────▽───────────────┐
│                Rust Core                     │
│  manager: 多任务调度/并发上限/取消令牌        │
│  meta:    TIFF 元数据解析                    │
│  planner: 金字塔级别/瓦片数计算               │
│  cutter:  分块读取→重采样→瓦片渲染(并行)      │
│  alpha:   透明处理(保留/阈值/颜色键)          │
│  writer:  目录布局写入 + manifest.json        │
│  preview: 应用内缩略图 + 浏览器 preview.html  │
└──────────────────────────────────────────────┘
```

---

## 5. 关键功能设计

### 5.1 文件选择与预览
- `file_picker` 选择 `.tif/.tiff`；支持拖拽到窗口（DragDrop 托管区）
- 选择后 Rust 生成 **≤2048px 缩略图 PNG 字节流**返回（超大文件分块降采样，避免整图载入内存）
- 预览区 `InteractiveViewer` 可缩放平移；底部信息条：宽高、位深、有无 Alpha、压缩方式、建议级别范围

### 5.2 级别（Zoom Levels）
- 语义与 Google 金字塔一致：level 0 = 整图一张瓦片，向上逐级 2 倍分辨率；native level = 原始分辨率所在级
- UI 提供双端滑块选择 `[zmin, zmax]`，默认全级别；实时显示**预计瓦片数**与磁盘占用估算

### 5.3 排列方式（Tile Scheme）
- `XYZ`：`{out}/{z}/{x}/{y}.png`（Google/OSM 风格）【已选定】
- `TMS`：Y 轴翻转【已选定】

### 5.4 透明值处理（三模式）
1. 不处理（保留源 Alpha）
2. **Alpha 阈值**：源 alpha < T → 输出全透明（T 可设 0–255，消除半透明晕圈）
3. **颜色键**：指定 RGB ± 容差 → 该颜色置为透明（白底图常用）

### 5.5 多任务与进度
- 任务队列：queued → running → done/error/canceled；全局并发数可配（默认 2）
- 单任务内 rayon 并行渲染瓦片；每任务独立事件流：
  `TaskEvent{ task_id, phase, level, tiles_done, total_tiles, bytes, speed, eta }`
- UI 进度条平滑动画 + 当前级别 + 速度/ETA；支持随时取消
- 输出目录写 `manifest.json`（源文件、参数、级别表），为后续断点续切预留

### 5.6 浏览器预览
- 任务完成自动在输出目录生成 `preview.html`（Leaflet，按所选 scheme 配置，含边界限制与级别范围）
- 任务卡片按钮「浏览器预览」→ `url_launcher` 打开；「打开文件夹」→ 资源管理器

### 5.7 设置页
- 全局并发数、默认输出目录、主题（深/浅/跟随系统）、界面语言（中文优先，预留 i18n）

---

## 6. 目录结构

```
H:\project_self\swCutter\
├─ pubspec.yaml            # Flutter 工程根
├─ lib\
│  ├─ main.dart
│  ├─ theme\               # Material 3 现代主题（深色为主、圆角卡片、强调渐变）
│  ├─ pages\               # home / new_task / settings / widgets
│  ├─ state\               # riverpod providers、TaskModel
│  └─ bridge\              # FRB 生成代码 + 封装
├─ rust\                   # Rust 原生 crate
│  └─ src\{api, meta, planner, cutter, alpha, writer, manager, preview}\
├─ web\  \windows\         # Flutter 平台工程
├─ scripts\build_windows.ps1  # 一键构建（flutter build windows + rust release）
└─ PLAN.md
```

---

## 7. 实施里程碑

| 里程碑 | 内容 | 完成标志 |
|---|---|---|
| **M0 环境准备** | 安装 Flutter SDK、启用 Windows 桌面、`flutter doctor` 通过 | doctor 无阻断项 |
| **M1 脚手架** | Flutter 工程 + FRB 集成 + 示例调用打通 + 构建脚本 | 空壳 App 能调用 Rust 返回字符串 |
| **M2 Rust 核心** | meta/planner/cutter/alpha/writer + 合成 TIFF 单元测试 | `cargo test` 全绿（验证瓦片数量、路径、像素正确性、透明变换） |
| **M3 API 与调度** | FRB API：创建/启动/取消任务 + 进度事件流 + 并发管理 | 演示程序多任务并行推流 |
| **M4 Flutter UI** | 主题框架与导航 → 新建任务页(含预览) → 任务列表与进度条 → 设置页 | 手工走通完整流程 |
| **M5 浏览器预览** | preview.html 生成器(XYZ/TMS/自定义) + 浏览器跳转 | 浏览器可逐级浏览瓦片 |
| **M6 打磨打包** | 错误处理/日志/空态/文案中文化/应用图标/README/发布 zip 脚本 | 交付可分发构建 |
| **M7 可选增强(P1)** | 暂停/恢复 + 断点续切、BigTIFF 检测提示、GeoTIFF 扩展评估 | — |

---

## 8. 验收清单

- [ ] 选择 TIFF 后界面内正确显示预览与元信息
- [ ] 修改级别/排列/透明/格式参数，瓦片数估算实时更新
- [ ] 切片产物符合所选 scheme 的目录结构与透明设置（单测断言像素值）
- [ ] ≥2 个任务同时运行，进度条各自独立刷新，速度/ETA 合理
- [ ] 取消正在运行的任务立即生效并清理状态
- [ ] 点击「浏览器预览」打开 Leaflet 页面，可缩放浏览所有级别瓦片
- [ ] 大文件（>1GB 级）切片内存占用受控（分块读取策略）

---

## 9. 风险与对策

| 风险 | 对策 |
|---|---|
| 特殊压缩 TIFF（JPEG-in-TIFF 等）解码失败 | 解码前探测压缩类型，明确报错并提示转换建议 |
| 超大文件内存峰值 | 优先 chunk 级读取 + 行带缓冲，禁止无条件整图解码 |
| FRB 版本兼容/代码生成问题 | 锁定版本，M1 先打通最小链路再扩展 |
| Flutter SDK 安装受网络影响 | 官方 zip 直链下载，失败时切换国内镜像环境变量 |
| GeoTIFF 投影需求（若用户要求） | 一期按普通影像像素坐标切片；Geo 重投影列为二期（引入 gdal/proj 依赖） |
