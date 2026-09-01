//! 任务管理 API：创建任务、并发调度、进度事件流、取消、图像信息与预览。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

use flutter_rust_bridge::frb;
use image::ImageEncoder as _;
use crate::engine::alpha::AlphaMode;
use crate::engine::cutter::{self, CutEvent, CutParams, TaskControl};
use crate::engine::meta;
use crate::engine::planner::{self, Resample, Scheme};
use crate::engine::source::SourceReader;
use crate::frb_generated::StreamSink;

// ---------------- DTO ----------------

/// 一个切片任务的全部配置（Dart 侧构造）。
#[derive(Debug, Clone)]
pub struct TaskConfig {
    pub source: String,
    pub output: String,
    pub tile_size: u32,
    pub zmin: Option<u32>,
    pub zmax: Option<u32>,
    pub scheme: Scheme,
    pub alpha: AlphaMode,
    pub resample: Resample,
    /// 全透明瓦片跳过写入
    pub skip_empty: bool,
    /// true = GDAL mercator 绝对级别模式（要求 GeoTIFF）
    pub mercator: bool,
    /// 精确反算：每个目的像素用 proj4rs 实时反算源投影坐标
    /// 默认 true（推荐）：消除非线性投影偏差。设为 false 时退化为老算法。
    pub precise: bool,
    /// 预览页底图/叠加层（JSON 数组字符串；None=空）
    pub preview_overlays: Option<String>,
}

/// 图像元信息 + 默认金字塔估算。
#[derive(Debug, Clone)]
pub struct ImageBrief {
    pub width: u32,
    pub height: u32,
    pub pixel_format: String,
    pub compression: String,
    pub chunk_type: String,
    pub chunk_hint: Vec<u32>,
    pub has_alpha: bool,
    pub rgba_bytes: u64,
    pub max_level: u32,
}

/// 单级别瓦片估算。
#[derive(Debug, Clone)]
pub struct LevelEstimate {
    pub level: u32,
    pub width: u32,
    pub height: u32,
    pub tiles_x: u32,
    pub tiles_y: u32,
    pub tiles: u64,
}

#[derive(Debug, Clone)]
pub struct TaskSummary {
    pub tiles_done: u64,
    pub total_tiles: u64,
    pub bytes_written: u64,
    pub elapsed_ms: u64,
    pub cancelled: bool,
    pub error: Option<String>,
}

/// 推送给 Dart 的任务事件。
#[derive(Debug, Clone)]
pub enum TaskEventKind {
    StatusChanged { status: String },
    Started { total_tiles: u64 },
    LevelStart { level: u32 },
    Progress {
        level: u32,
        tiles_done: u64,
        total_tiles: u64,
        bytes_written: u64,
        elapsed_ms: u64,
    },
    Finished { summary: TaskSummary },
}

#[derive(Debug, Clone)]
pub struct TaskEvent {
    pub task_id: u64,
    pub kind: TaskEventKind,
}

/// 任务列表条目快照。
#[derive(Debug, Clone)]
pub struct TaskDto {
    pub id: u64,
    pub source: String,
    pub output: String,
    pub tile_size: u32,
    pub scheme: Scheme,
    pub alpha: AlphaMode,
    pub resample: Resample,
    pub zmin: Option<u32>,
    pub zmax: Option<u32>,
    pub status: String,
    pub level: u32,
    pub tiles_done: u64,
    pub total_tiles: u64,
    pub bytes_written: u64,
    pub elapsed_ms: u64,
    pub error: Option<String>,
    /// 开始 / 完成时刻（Unix 毫秒；0 表示未知）
    pub started_at_ms: u64,
    pub finished_at_ms: u64,
}

// ---------------- 内部状态 ----------------

#[derive(Debug, Default, Clone)]
#[frb(ignore)]
struct Snap {
    level: u32,
    tiles_done: u64,
    total_tiles: u64,
    bytes_written: u64,
    elapsed_ms: u64,
}

#[frb(ignore)]
struct TaskEntry {
    id: u64,
    cfg: TaskConfig,
    status: Mutex<String>,
    cancel: AtomicBool,
    control: Arc<TaskControl>,
    snap: Mutex<Snap>,
    error: Mutex<Option<String>>,
    started_at: Mutex<Option<std::time::Instant>>,
    /// Unix 毫秒时间戳
    started_ms: Mutex<u64>,
    finished_ms: Mutex<u64>,
    /// 瓦片边界（JSON，切片完成后写入；随历史持久化）
    bounds: Mutex<Option<String>>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[frb(ignore)]
struct Gate {
    max: AtomicU32,
    count: Mutex<u32>,
    cv: Condvar,
}

impl Gate {
    fn new(max: u32) -> Self {
        Self { max: AtomicU32::new(max), count: Mutex::new(0), cv: Condvar::new() }
    }
    /// 获得执行槽位；等待期间可被取消打断。返回 false 表示已取消。
    fn acquire(&self, cancel: &AtomicBool) -> bool {
        let mut cnt = self.count.lock().unwrap();
        loop {
            if cancel.load(Ordering::Relaxed) {
                return false;
            }
            let max = self.max.load(Ordering::Relaxed);
            if *cnt < max {
                *cnt += 1;
                return true;
            }
            let (guard, _) = self
                .cv
                .wait_timeout(cnt, Duration::from_millis(80))
                .unwrap();
            cnt = guard;
        }
    }
    fn release(&self) {
        let mut cnt = self.count.lock().unwrap();
        *cnt = cnt.saturating_sub(1);
        drop(cnt);
        self.cv.notify_all();
    }
}

#[frb(ignore)]
struct Manager {
    tasks: Mutex<Vec<Arc<TaskEntry>>>,
    next_id: AtomicU64,
    gate: Gate,
    sink: Mutex<Option<StreamSink<TaskEvent>>>,
}

fn manager() -> &'static Manager {
    static MANAGER: OnceLock<Arc<Manager>> = OnceLock::new();
    MANAGER.get_or_init(|| {
        let mgr = Arc::new(Manager {
            tasks: Mutex::new(Vec::new()),
            next_id: AtomicU64::new(1),
            gate: Gate::new(default_concurrency()),
            sink: Mutex::new(None),
        });
        load_history(&mgr);
        mgr
    })
}

// ---------------- 历史持久化（SQLite） ----------------

use super::history_store::{self, Rec as HistoryRec};

fn is_terminal(status: &str) -> bool {
    matches!(status, "done" | "error" | "cancelled")
}

/// 启动时恢复历史为只读冷任务（无工作线程）。
fn load_history(mgr: &Manager) {
    history_store::init();
    let recs = history_store::load();
    if recs.is_empty() {
        return;
    }
    let mut max_id = 0u64;
    let mut entries = Vec::new();
    for r in recs {
        max_id = max_id.max(r.id);
        // 非终态（上次运行中断）→ 标记为取消并注明
        let (status, error) = if is_terminal(&r.status) {
            (r.status.clone(), r.error.clone())
        } else {
            ("cancelled".into(), Some("应用退出导致中断（可重跑同参数续切）".to_string()))
        };
        entries.push(Arc::new(TaskEntry {
            id: r.id,
            cfg: TaskConfig {
                source: r.source,
                output: r.output,
                tile_size: r.tile_size,
                zmin: r.zmin,
                zmax: r.zmax,
                scheme: r.scheme,
                alpha: r.alpha,
                resample: r.resample,
                skip_empty: r.skip_empty,
                mercator: r.mercator,
                precise: r.precise,
                preview_overlays: None,
            },
            status: Mutex::new(status),
            cancel: AtomicBool::new(true),
            control: TaskControl::new(),
            snap: Mutex::new(Snap {
                level: r.level,
                tiles_done: r.tiles_done,
                total_tiles: r.total_tiles,
                bytes_written: r.bytes_written,
                elapsed_ms: r.elapsed_ms,
            }),
            error: Mutex::new(error),
            started_at: Mutex::new(None),
            started_ms: Mutex::new(r.started_ms),
            finished_ms: Mutex::new(r.finished_ms),
            bounds: Mutex::new(r.bounds_json),
        }));
    }
    mgr.next_id.store(max_id + 1, Ordering::Relaxed);
    *mgr.tasks.lock().unwrap() = entries;
}

/// 把当前全部任务（含冷历史）写回 SQLite。
fn persist(mgr: &Manager) {
    let recs: Vec<HistoryRec> = mgr.tasks.lock().unwrap().iter().map(|e| {
        let snap = e.snap.lock().unwrap().clone();
        HistoryRec {
            id: e.id,
            source: e.cfg.source.clone(),
            output: e.cfg.output.clone(),
            tile_size: e.cfg.tile_size,
            scheme: e.cfg.scheme,
            alpha: e.cfg.alpha.clone(),
            resample: e.cfg.resample,
            zmin: e.cfg.zmin,
            zmax: e.cfg.zmax,
            skip_empty: e.cfg.skip_empty,
            mercator: e.cfg.mercator,
            precise: e.cfg.precise,
            status: e.status.lock().unwrap().clone(),
            level: snap.level,
            tiles_done: snap.tiles_done,
            total_tiles: snap.total_tiles,
            bytes_written: snap.bytes_written,
            elapsed_ms: snap.elapsed_ms,
            error: e.error.lock().unwrap().clone(),
            started_ms: *e.started_ms.lock().unwrap(),
            finished_ms: *e.finished_ms.lock().unwrap(),
            bounds_json: e.bounds.lock().unwrap().clone(),
        }
    }).collect();
    history_store::save(&recs);
}

fn default_concurrency() -> u32 {
    std::env::var("SWCUTTER_CONCURRENCY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2)
        .clamp(1, 16)
}

fn broadcast(mgr: &Manager, ev: TaskEvent) {
    if let Some(sink) = mgr.sink.lock().unwrap().as_ref() {
        let _ = sink.add(ev);
    }
}

// ---------------- API：信息与预览 ----------------

pub fn read_image_info(path: String) -> anyhow::Result<ImageBrief> {
    let info = meta::probe(Path::new(&path))?;
    Ok(ImageBrief {
        width: info.width,
        height: info.height,
        pixel_format: info.pixel_format,
        compression: info.compression,
        chunk_type: info.chunk_type,
        chunk_hint: vec![info.chunk_hint.0, info.chunk_hint.1],
        has_alpha: info.has_alpha,
        rgba_bytes: info.rgba_bytes,
        max_level: planner::native_level(info.width, info.height, planner::DEFAULT_TILE_SIZE),
    })
}

/// 金字塔估算结果（双模式）。
#[derive(Debug, Clone)]
pub struct PyramidEstimate {
    /// mercator 模式下为 GSD 对应 native zoom；relative 模式 None
    pub native_zoom: Option<u32>,
    pub levels: Vec<LevelEstimate>,
}

/// 按给定参数估算各级瓦片数（UI 滑块实时调用，纯计算无 IO）。
/// mercator=true 时走 GDAL 绝对级别语义（需要地理参考）。
pub fn estimate_pyramid_ex(
    source: String,
    width: u32,
    height: u32,
    tile_size: u32,
    zmin: Option<u32>,
    zmax: Option<u32>,
    mercator: bool,
) -> anyhow::Result<PyramidEstimate> {
    if !mercator {
        let p = planner::plan(width, height, tile_size, zmin, zmax)?;
        return Ok(PyramidEstimate {
            native_zoom: Some(planner::native_level(width, height, tile_size)),
            levels: p
                .levels
                .iter()
                .map(|lp| LevelEstimate {
                    level: lp.level,
                    width: lp.width,
                    height: lp.height,
                    tiles_x: lp.tiles_x,
                    tiles_y: lp.tiles_y,
                    tiles: lp.tiles_x as u64 * lp.tiles_y as u64,
                })
                .collect(),
        });
    }
    let g = crate::engine::meta::probe_georef(Path::new(&source))?
        .ok_or_else(|| anyhow::anyhow!("影像缺少地理参考，无法使用 GDAL 绝对级别模式"))?;
    let b = g.bounds3857(width, height);
    let sx_m = (b[2] - b[0]) / width as f64;
    let mp = crate::engine::mercator::plan(
        b,
        sx_m,
        tile_size,
        zmin,
        zmax,
        crate::engine::planner::MAX_TOTAL_TILES_HARD,
    )?;
    Ok(PyramidEstimate {
        native_zoom: Some(mp.native_zoom),
        levels: mp
            .levels
            .iter()
            .map(|lv| LevelEstimate {
                level: lv.z,
                width: (lv.tx1 - lv.tx0 + 1).saturating_mul(tile_size),
                height: (lv.ty1 - lv.ty0 + 1).saturating_mul(tile_size),
                tiles_x: lv.tx1 - lv.tx0 + 1,
                tiles_y: lv.ty1 - lv.ty0 + 1,
                tiles: lv.count(),
            })
            .collect(),
    })
}

/// 兼容旧接口：相对模式估算。
pub fn estimate_pyramid(
    width: u32,
    height: u32,
    tile_size: u32,
    zmin: Option<u32>,
    zmax: Option<u32>,
) -> anyhow::Result<Vec<LevelEstimate>> {
    Ok(estimate_pyramid_ex(String::new(), width, height, tile_size, zmin, zmax, false)?.levels)
}

/// 生成界面内预览缩略图（PNG 字节）。超大图按行带抽稀采样，内存有界。
/// 采样源图指定坐标的 RGBA 像素值（供预览点选取色）。
pub fn sample_pixel(source: String, x: i64, y: i64) -> anyhow::Result<Vec<u8>> {
    let mut reader = SourceReader::open(Path::new(&source))?;
    let (w, h) = (reader.width as i64, reader.height as i64);
    if x < 0 || y < 0 || x >= w || y >= h {
        anyhow::bail!("采样坐标越界 ({x},{y})，图像 {w}×{h}");
    }
    let buf = reader.read_rect(x, y, 1, 1)?;
    if buf.len() < 4 {
        anyhow::bail!("采样返回数据不足");
    }
    Ok(buf)
}

/// 生成界面内预览缩略图（PNG 字节）。超大图按行带抽稀采样，内存有界。
/// `alpha` 与切片一致地应用到预览图，使预览所见即所得（如 ColorKey 黑边→透明）。
pub fn make_preview(source: String, max_px: u32, alpha: AlphaMode) -> anyhow::Result<Vec<u8>> {
    let (ow, oh, canvas) =
        crate::engine::source::preview_sample_parallel(Path::new(&source), max_px)?;

    // 轻度平滑后编码为 PNG
    let img =
        image::RgbaImage::from_raw(ow, oh, canvas).ok_or_else(|| anyhow::anyhow!("预览缓冲异常"))?;
    let mut smooth = image::imageops::resize(
        &img,
        ow.min(max_px),
        oh.min(max_px),
        image::imageops::FilterType::Triangle,
    );
    // 与切片一致：在最终像素上应用透明模式（ColorKey 容差匹配边色→透明）
    alpha.apply(smooth.as_mut());
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(std::io::Cursor::new(&mut png))
        .write_image(
            smooth.as_raw(),
            smooth.width(),
            smooth.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| anyhow::anyhow!("PNG 编码失败: {e}"))?;
    Ok(png)
}

// ---------------- API：订阅与任务控制 ----------------

pub fn subscribe_events(sink: StreamSink<TaskEvent>) -> anyhow::Result<()> {
    *manager().sink.lock().unwrap() = Some(sink);
    Ok(())
}

pub fn start_task(cfg: TaskConfig) -> anyhow::Result<u64> {
    if !Path::new(&cfg.source).is_file() {
        anyhow::bail!("源文件不存在: {}", cfg.source);
    }
    let mgr = manager();
    let id = mgr.next_id.fetch_add(1, Ordering::Relaxed);
    let entry = Arc::new(TaskEntry {
        id,
        cfg: cfg.clone(),
        status: Mutex::new("queued".into()),
        cancel: AtomicBool::new(false),
        control: TaskControl::new(),
        snap: Mutex::new(Snap::default()),
        error: Mutex::new(None),
        started_at: Mutex::new(None),
        started_ms: Mutex::new(0),
        finished_ms: Mutex::new(0),
        bounds: Mutex::new(None),
    });
    mgr.tasks.lock().unwrap().push(Arc::clone(&entry));
    log(
        "info",
        &format!(
            "task#{id} queued source={} output={}",
            cfg.source, cfg.output
        ),
    );
    persist(mgr);
    broadcast(
        mgr,
        TaskEvent {
            task_id: id,
            kind: TaskEventKind::StatusChanged { status: "queued".into() },
        },
    );
    std::thread::Builder::new()
        .name(format!("swcut-task-{id}"))
        .spawn(move || worker(entry))?;
    Ok(id)
}

pub fn cancel_task(id: u64) -> anyhow::Result<bool> {
    let mgr = manager();
    let tasks = mgr.tasks.lock().unwrap();
    if let Some(e) = tasks.iter().find(|t| t.id == id) {
        e.cancel.store(true, Ordering::Relaxed);
        e.control.cancel.store(true, Ordering::Relaxed);
        log("info", &format!("task#{id} cancel requested"));
        drop(tasks);
        persist(mgr);
        Ok(true)
    } else {
        Ok(false)
    }
}

/// 暂停：引擎工作线程在下一块瓦片前停靠。
pub fn pause_task(id: u64) -> anyhow::Result<bool> {
    let mgr = manager();
    let tasks = mgr.tasks.lock().unwrap();
    if let Some(e) = tasks.iter().find(|t| t.id == id) {
        if *e.status.lock().unwrap() != "running" {
            return Ok(false);
        }
        e.control.paused.store(true, Ordering::Relaxed);
        *e.status.lock().unwrap() = "paused".into();
        log("info", &format!("task#{id} paused"));
        drop(tasks);
        persist(mgr);
        broadcast(
            mgr,
            TaskEvent {
                task_id: id,
                kind: TaskEventKind::StatusChanged { status: "paused".into() },
            },
        );
        Ok(true)
    } else {
        Ok(false)
    }
}

/// 恢复已暂停的任务。
pub fn resume_task(id: u64) -> anyhow::Result<bool> {
    let mgr = manager();
    let tasks = mgr.tasks.lock().unwrap();
    if let Some(e) = tasks.iter().find(|t| t.id == id) {
        if *e.status.lock().unwrap() != "paused" {
            return Ok(false);
        }
        e.control.paused.store(false, Ordering::Relaxed);
        *e.status.lock().unwrap() = "running".into();
        log("info", &format!("task#{id} resumed"));
        drop(tasks);
        persist(mgr);
        broadcast(
            mgr,
            TaskEvent {
                task_id: id,
                kind: TaskEventKind::StatusChanged { status: "running".into() },
            },
        );
        Ok(true)
    } else {
        Ok(false)
    }
}

pub fn remove_task(id: u64) -> anyhow::Result<bool> {
    let mgr = manager();
    let removed = {
        let mut tasks = mgr.tasks.lock().unwrap();
        if let Some(pos) = tasks.iter().position(|t| t.id == id) {
            let e = tasks.remove(pos);
            if *e.status.lock().unwrap() == "running" {
                tasks.insert(pos, e); // 放回去：运行中的不允许移除，先取消
                false
            } else {
                true
            }
        } else {
            false
        }
    };
    if removed {
        persist(mgr);
    }
    Ok(removed)
}

pub fn set_max_concurrency(n: u32) -> anyhow::Result<()> {
    let n = n.clamp(1, 16);
    let mgr = manager();
    mgr.gate.max.store(n, Ordering::Relaxed);
    mgr.gate.cv.notify_all();
    Ok(())
}

pub fn get_max_concurrency() -> anyhow::Result<u32> {
    Ok(manager().gate.max.load(Ordering::Relaxed))
}

pub fn list_tasks() -> anyhow::Result<Vec<TaskDto>> {
    Ok(manager()
        .tasks
        .lock()
        .unwrap()
        .iter()
        .map(to_dto)
        .collect())
}

fn to_dto(e: &Arc<TaskEntry>) -> TaskDto {
    let snap = e.snap.lock().unwrap().clone();
    let started = *e.started_at.lock().unwrap();
    let elapsed_ms = started
        .map(|t| t.elapsed().as_millis() as u64)
        .unwrap_or(0);
    TaskDto {
        id: e.id,
        source: e.cfg.source.clone(),
        output: e.cfg.output.clone(),
        tile_size: e.cfg.tile_size,
        scheme: e.cfg.scheme,
        alpha: e.cfg.alpha,
        resample: e.cfg.resample,
        zmin: e.cfg.zmin,
        zmax: e.cfg.zmax,
        status: e.status.lock().unwrap().clone(),
        level: snap.level,
        tiles_done: snap.tiles_done,
        total_tiles: snap.total_tiles,
        bytes_written: snap.bytes_written,
        elapsed_ms,
        error: e.error.lock().unwrap().clone(),
        started_at_ms: *e.started_ms.lock().unwrap(),
        finished_at_ms: *e.finished_ms.lock().unwrap(),
    }
}

// ---------------- 工作线程 ----------------

fn worker(entry: Arc<TaskEntry>) {
    let mgr = manager();

    // 排队等待槽位（可被打断）
    if !mgr.gate.acquire(&entry.cancel) {
        *entry.status.lock().unwrap() = "cancelled".into();
        *entry.finished_ms.lock().unwrap() = now_ms();
        persist(mgr);
        broadcast(
            mgr,
            TaskEvent {
                task_id: entry.id,
                kind: TaskEventKind::Finished { summary: TaskSummary {
                    tiles_done: 0, total_tiles: 0, bytes_written: 0,
                    elapsed_ms: 0, cancelled: true, error: None,
                }},
            },
        );
        return;
    }
    let _slot_release = SlotGuard;

    *entry.started_at.lock().unwrap() = Some(std::time::Instant::now());
    *entry.started_ms.lock().unwrap() = now_ms();
    *entry.status.lock().unwrap() = "running".into();
    persist(mgr);
    broadcast(
        mgr,
        TaskEvent {
            task_id: entry.id,
            kind: TaskEventKind::StatusChanged { status: "running".into() },
        },
    );

    let params = CutParams {
        source: PathBuf::from(&entry.cfg.source),
        output: PathBuf::from(&entry.cfg.output),
        tile_size: entry.cfg.tile_size,
        zmin: entry.cfg.zmin,
        zmax: entry.cfg.zmax,
        scheme: entry.cfg.scheme,
        alpha: entry.cfg.alpha,
        resample: entry.cfg.resample,
        skip_empty: entry.cfg.skip_empty,
        mercator: entry.cfg.mercator,
        precise: entry.cfg.precise,
        preview_overlays: entry.cfg.preview_overlays.clone(),
    };

    let entry2 = Arc::clone(&entry);
    let sink_fn = move |ev: CutEvent| {
        // 先按引用更新快照，再构造事件（避免部分移动）
        {
            let mut snap = entry2.snap.lock().unwrap();
            match &ev {
                CutEvent::Start { total_tiles } => snap.total_tiles = *total_tiles,
                CutEvent::LevelStart { level, .. } => snap.level = *level,
                CutEvent::Progress(p) => {
                    snap.level = p.level;
                    snap.tiles_done = p.tiles_done;
                    snap.total_tiles = p.total_tiles;
                    snap.bytes_written = p.bytes_written;
                    snap.elapsed_ms = p.elapsed_ms;
                }
                CutEvent::Done(_) => {}
            }
        }
        let kind = match &ev {
            CutEvent::Start { total_tiles } => TaskEventKind::Started { total_tiles: *total_tiles },
            CutEvent::LevelStart { level, .. } => TaskEventKind::LevelStart { level: *level },
            CutEvent::Progress(_) => {
                let s = entry2.snap.lock().unwrap().clone();
                TaskEventKind::Progress {
                    level: s.level,
                    tiles_done: s.tiles_done,
                    total_tiles: s.total_tiles,
                    bytes_written: s.bytes_written,
                    elapsed_ms: s.elapsed_ms,
                }
            }
            CutEvent::Done(_) => return, // 由 worker 统一上报 Finished
        };
        broadcast(manager(), TaskEvent { task_id: entry2.id, kind });
    };

    let summary = cutter::run_cut_with_control(
        &params,
        Arc::clone(&entry.control),
        Arc::new(Mutex::new(sink_fn)),
    );
    log(
        if summary.errors.is_empty() { "info" } else { "error" },
        &format!(
            "task#{} finished tiles={} bytes={} ms={} cancelled={} errors={}",
            entry.id,
            summary.total_tiles,
            summary.bytes_written,
            summary.elapsed_ms,
            summary.cancelled,
            summary.errors.len()
        ),
    );

    let cancelled = summary.cancelled;
    let err = summary.errors.first().cloned();
    // 瓦片边界落库（供预览页/外部工具定位）：世界网格 + 每级范围
    {
        let bounds = serde_json::json!({
            "scheme": params.scheme.as_str(),
            "tile": params.tile_size,
            "tms": params.scheme == planner::Scheme::Tms,
            "zmin": summary.levels.first().map(|l| l.level),
            "zmax": summary.levels.last().map(|l| l.level),
            "levels": summary.levels.iter().map(|l| serde_json::json!({
                "z": l.level, "ox": l.ox, "oy": l.oy, "wy": l.wy,
                "tx": (l.width as f64 / params.tile_size as f64).ceil() as u64,
                "ty": (l.height as f64 / params.tile_size as f64).ceil() as u64,
            })).collect::<Vec<_>>(),
        });
        *entry.bounds.lock().unwrap() = Some(bounds.to_string());
    }
    {
        let mut snap = entry.snap.lock().unwrap();
        snap.tiles_done = summary.total_tiles;
        snap.bytes_written = summary.bytes_written;
        snap.elapsed_ms = summary.elapsed_ms;
    }
    *entry.error.lock().unwrap() = err.clone();
    *entry.status.lock().unwrap() = if cancelled {
        "cancelled"
    } else if err.is_some() {
        "error"
    } else {
        "done"
    }
    .into();
    *entry.finished_ms.lock().unwrap() = now_ms();
    persist(mgr);

    broadcast(
        mgr,
        TaskEvent {
            task_id: entry.id,
            kind: TaskEventKind::Finished {
                summary: TaskSummary {
                    tiles_done: summary.total_tiles,
                    total_tiles: entry.snap.lock().unwrap().total_tiles,
                    bytes_written: summary.bytes_written,
                    elapsed_ms: summary.elapsed_ms,
                    cancelled,
                    error: err,
                },
            },
        },
    );
}

/// RAII：worker 结束时释放并发槽位。
#[frb(ignore)]
struct SlotGuard;
impl Drop for SlotGuard {
    fn drop(&mut self) {
        manager().gate.release();
    }
}

// ---------------- 轻量日志 ----------------

/// 追加一行日志到应用数据目录的 `logs/swcutter.log`（失败静默）。
///
/// 平台路径由 [`crate::util::paths::app_data_dir`] 解析：
/// - Windows: `%APPDATA%\swCutter\logs`
/// - macOS:   `$HOME/Library/Application Support/swCutter/logs`
/// - Linux:   `$XDG_DATA_HOME/swCutter/logs`（或 `~/.local/share/swCutter/logs`）
pub(crate) fn log(level: &str, msg: &str) {
    use std::io::Write as _;
    let Some(root) = crate::util::paths::app_data_dir() else {
        return;
    };
    let dir = root.join("logs");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let line = format!("[{ts}] [{level}] {msg}\n");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("swcutter.log"))
    {
        let _ = f.write_all(line.as_bytes());
    }
}
