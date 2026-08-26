//! 切片执行器：rayon 按瓦片并行渲染 + 进度快照 + 取消。

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::imageops::FilterType as ImgFilter;
use image::{ExtendedColorType, ImageEncoder, RgbaImage};
use rayon::prelude::*;

use super::alpha::AlphaMode;
use super::error::{CoreError, CoreResult};
use super::planner::{plan, LevelPlan, Resample, Scheme};
use super::source::SourceReader;
use super::writer;

#[derive(Debug, Clone)]
pub struct CutParams {
    pub source: PathBuf,
    pub output: PathBuf,
    pub tile_size: u32,
    pub zmin: Option<u32>,
    pub zmax: Option<u32>,
    pub scheme: Scheme,
    pub alpha: AlphaMode,
    pub resample: Resample,
    /// 透明处理后完全透明的瓦片不写文件
    pub skip_empty: bool,
    /// true = GDAL mercator 绝对级别模式（要求 GeoTIFF 地理参考，zmax 截断 native）
    pub mercator: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProgressSnapshot {
    pub level: u32,
    pub tiles_done: u64,
    pub total_tiles: u64,
    pub bytes_written: u64,
    /// 实时用时（毫秒），供 UI 显示总用时/ETA
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LevelSummary {
    pub level: u32,
    pub width: u32,
    pub height: u32,
    pub tiles: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CutSummary {
    pub output_dir: String,
    pub total_tiles: u64,
    pub bytes_written: u64,
    pub elapsed_ms: u64,
    pub cancelled: bool,
    pub errors: Vec<String>,
    pub levels: Vec<LevelSummary>,
}

#[derive(Debug, Clone)]
pub enum CutEvent {
    Start { total_tiles: u64 },
    LevelStart { level: u32, tiles: u64 },
    Progress(ProgressSnapshot),
    Done(CutSummary),
}

/// 任务运行控制句柄：取消 / 暂停。
#[derive(Debug, Default)]
pub struct TaskControl {
    pub cancel: AtomicBool,
    pub paused: AtomicBool,
}

impl TaskControl {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
}

/// 兼容入口：内部创建独立控制句柄。
pub fn run_cut(params: &CutParams, sink: Arc<Mutex<dyn FnMut(CutEvent) + Send>>) -> CutSummary {
    run_cut_with_control(params, TaskControl::new(), sink)
}

/// 读取输出目录旧 manifest；若关键参数与本次一致则返回它（用于断点续切）。
fn read_resumable_manifest(params: &CutParams) -> Option<writer::Manifest> {
    let raw = std::fs::read_to_string(params.output.join(writer::MANIFEST_NAME)).ok()?;
    let m: writer::Manifest = serde_json::from_str(&raw).ok()?;
    let same = m.source == params.source.display().to_string()
        && m.tile_size == params.tile_size
        && m.scheme == params.scheme.as_str();
    if same { Some(m) } else { None }
}

const ERROR_CAP: usize = 16;
/// 进度上报间隔
const TICK: Duration = Duration::from_millis(120);

struct Job {
    plan_idx: usize,
    tx: u32,
    ty: u32,
    /// 绝对级别号（relative 模式 = pyramid.levels[plan_idx].level）
    z: u32,
    /// TMS 翻转用总行数；mercator 模式 = 1<<z
    tiles_y: u32,
}

/// 执行切片（同步阻塞）。`sink` 会在工作线程上被调用，需自行保证线程安全。
/// 通过 `control` 支持外部取消与暂停。
pub fn run_cut_with_control(
    params: &CutParams,
    control: Arc<TaskControl>,
    sink: Arc<Mutex<dyn FnMut(CutEvent) + Send>>,
) -> CutSummary {
    let started = Instant::now();
    let emit = |ev: CutEvent| {
        if let Ok(mut f) = sink.lock() {
            f(ev);
        }
    };

    // ---- 规划 ----
    let summary_err = |msg: String| CutSummary {
        output_dir: params.output.display().to_string(),
        total_tiles: 0,
        bytes_written: 0,
        elapsed_ms: started.elapsed().as_millis() as u64,
        cancelled: false,
        errors: vec![msg],
        levels: vec![],
    };

    let (src_w, src_h) = match SourceReader::open(&params.source) {
        Ok(r) => (r.width, r.height),
        Err(e) => {
            let s = summary_err(e.to_string());
            emit(CutEvent::Done(s.clone()));
            return s;
        }
    };
    let mut geo_ref: Option<super::meta::GeoRef> = None;
    let merc_bounds: [f64; 4];
    let pyramid: Option<super::planner::PyramidPlan>;
    let mplan: Option<super::mercator::MercPlan>;
    if params.mercator {
        match super::meta::probe_georef(&params.source) {
            Ok(Some(g)) => {
                let b = g.bounds3857(src_w, src_h);
                let sx_m = (b[2] - b[0]) / src_w as f64;
                mplan = Some(match super::mercator::plan(
                    b,
                    sx_m,
                    params.tile_size,
                    params.zmin,
                    params.zmax,
                    super::planner::MAX_TOTAL_TILES_HARD,
                ) {
                    Ok(p) => p,
                    Err(e) => {
                        let s = summary_err(e.to_string());
                        emit(CutEvent::Done(s.clone()));
                        return s;
                    }
                });
                merc_bounds = b;
                geo_ref = Some(g);
            }
            Ok(None) => {
                let s = summary_err(
                    "影像缺少地理参考（PixelScale/Tiepoint），无法使用 GDAL 绝对级别模式".into(),
                );
                emit(CutEvent::Done(s.clone()));
                return s;
            }
            Err(e) => {
                let s = summary_err(e.to_string());
                emit(CutEvent::Done(s.clone()));
                return s;
            }
        }
        pyramid = None;
    } else {
        merc_bounds = [0.0; 4];
        mplan = None;
        pyramid = Some(match plan(src_w, src_h, params.tile_size, params.zmin, params.zmax) {
            Ok(p) => p,
            Err(e) => {
                let s = summary_err(e.to_string());
                emit(CutEvent::Done(s.clone()));
                return s;
            }
        });
    }

    if let Err(e) = writer::ensure_out_dir(&params.output) {
        let s = summary_err(super::error::io_err(params.output.display().to_string(), e).to_string());
        emit(CutEvent::Done(s.clone()));
        return s;
    }

    // ---- 任务列表（按级别升序；双模式统一 Job）----
    let mut jobs: Vec<Job> = Vec::new();
    match (&pyramid, &mplan) {
        (Some(py), _) => {
            for (pi, lp) in py.levels.iter().enumerate() {
                for ty in 0..lp.tiles_y {
                    for tx in 0..lp.tiles_x {
                        jobs.push(Job { plan_idx: pi, tx, ty, z: lp.level, tiles_y: lp.tiles_y });
                    }
                }
            }
        }
        (_, Some(mp)) => {
            // gdal2tiles 顺序：最高级（直接采样源）最先，低级别概览随后
            for lv in mp.levels.iter().rev() {
                let rows = 1u32 << lv.z.min(30);
                for ty in lv.ty0..=lv.ty1 {
                    for tx in lv.tx0..=lv.tx1 {
                        jobs.push(Job { plan_idx: usize::MAX, tx, ty, z: lv.z, tiles_y: rows });
                    }
                }
            }
        }
        _ => {}
    }
    let total = jobs.len() as u64;
    emit(CutEvent::Start { total_tiles: total });

    // ---- 断点续切：旧 manifest 参数一致时，跳过已存在的瓦片 ----
    let resumable = read_resumable_manifest(params).is_some();
    let mut skip: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    if resumable {
        for job in &jobs {
            let rel = writer::tile_rel_path(params.scheme, job.z, job.tx, job.ty, job.tiles_y);
            let full = params.output.join(&rel);
            if let Ok(md) = std::fs::metadata(&full) {
                if md.len() > 0 {
                    skip.insert(rel);
                }
            }
        }
    }

    // ---- 共享状态 ----
    let done_tiles = Arc::new(AtomicU64::new(0));
    let done_bytes = Arc::new(AtomicU64::new(0));
    let current_level = Arc::new(AtomicU32::new(u32::MAX));
    // 引擎侧取消/暂停统一走控制句柄（ticker 持有同一 Arc，见下方 TickerShared）
    let workers_done = Arc::new(AtomicBool::new(false));
    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    // ---- 进度心跳线程（先启动：大图首次解码期间也有进度心跳）----
    let ticker_state = TickerShared {
        sink: Arc::clone(&sink),
        done_tiles: Arc::clone(&done_tiles),
        done_bytes: Arc::clone(&done_bytes),
        current_level: Arc::clone(&current_level),
        control: Arc::clone(&control),
        workers_done: Arc::clone(&workers_done),
        total,
        started,
    };
    let ticker = thread::spawn(move || ticker_loop(ticker_state));

    // ---- 巨型条带源：全图光栅共享（一次解码，避免每瓦片重复解压整条带）----
    let shared_full: Arc<Mutex<Option<Vec<u8>>>> = Arc::new(Mutex::new({
        let probe = SourceReader::open(&params.source);
        match probe {
            Ok(p) => {
                let gb = p.giant_strip_bytes();
                let total_rgba = p.width as u64 * p.height as u64 * 4;
                if gb > 32 * 1024 * 1024 && total_rgba <= 3 * 1024 * 1024 * 1024 {
                    let mut r = p;
                    match r.read_full() {
                        Ok(buf) => Some(buf),
                        Err(e) => {
                            workers_done.store(true, Ordering::Relaxed);
                            let _ = ticker.join();
                            let s = summary_err(e.to_string());
                            emit(CutEvent::Done(s.clone()));
                            return s;
                        }
                    }
                } else {
                    None
                }
            }
            Err(_) => None,
        }
    }));

    let src_path = params.source.clone();
    let p_ref = params;
    let sf_full = Arc::clone(&shared_full);
    // mercator 模式的最高级（基础级）：该级直接采样源；更低级由子瓦片合成
    let base_z = mplan.as_ref().and_then(|mp| mp.levels.last().map(|l| l.z));

    jobs.par_iter().for_each(|job| {
        // 暂停停靠（取消可打断）
        while control.paused.load(Ordering::Relaxed) && !control.cancel.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(120));
        }
        if control.cancel.load(Ordering::Relaxed) {
            return;
        }
        let z_label = job.z;
        current_level.store(job.z, Ordering::Relaxed); // 最近开始的级别（诚实展示）

        // 断点续切命中：直接计入完成
        let rel = writer::tile_rel_path(p_ref.scheme, job.z, job.tx, job.ty, job.tiles_y);
        if !skip.is_empty() && skip.contains(&rel) {
            let bytes = std::fs::metadata(params.output.join(&rel))
                .map(|m| m.len())
                .unwrap_or(0);
            done_tiles.fetch_add(1, Ordering::Relaxed);
            done_bytes.fetch_add(bytes, Ordering::Relaxed);
            return;
        }

        let result = if matches!(base_z, Some(bz) if job.z < bz) {
            // gdal2tiles 概览级：由上一级 4 块子瓦片降采样合成（源图只在基础级被读取）
            render_tile_overview(p_ref, job.z, job.tx, job.ty)
        } else if sf_full.lock().unwrap().is_some() {
            // 巨型条带源：直接读共享全图（无解码竞争，取消/暂停即时生效）
            let guard = sf_full.lock().unwrap();
            let full = guard.as_ref().expect("checked some");
            match (&pyramid, geo_ref) {
                (Some(py), _) => render_tile_from_full(
                    full,
                    src_w,
                    src_h,
                    p_ref,
                    &py.levels[job.plan_idx],
                    job.tx,
                    job.ty,
                ),
                (_, Some(g)) => render_tile_mercator_from_full(
                    full, src_w, src_h, p_ref, g, job.z, job.tx, job.ty,
                ),
                _ => Err(CoreError::InvalidInput("内部模式错误".into())),
            }
        } else {
            THREAD_READER.with(|cell| {
                ensure_thread_reader(&src_path)?;
                let mut slot = cell.borrow_mut();
                let reader = slot.as_mut().expect("thread reader prepared");
                match (&pyramid, geo_ref) {
                    (Some(py), _) => render_tile(
                        reader,
                        p_ref,
                        &py.levels[job.plan_idx],
                        py.image_width,
                        py.image_height,
                        job.tx,
                        job.ty,
                    ),
                    (_, Some(g)) => render_tile_mercator(
                        reader,
                        p_ref,
                        g,
                        merc_bounds,
                        job.z,
                        job.tx,
                        job.ty,
                    ),
                    _ => Err(CoreError::InvalidInput("内部模式错误".into())),
                }
            })
        };

        match result {
            Ok(bytes) => {
                done_tiles.fetch_add(1, Ordering::Relaxed);
                done_bytes.fetch_add(bytes, Ordering::Relaxed);
            }
            Err(e) => {
                let mut errs = errors.lock().unwrap();
                if errs.len() < ERROR_CAP {
                    errs.push(format!(
                        "Z{} tile({},{}) 失败: {e}",
                        z_label, job.tx, job.ty
                    ));
                }
                drop(errs);
                control.cancel.store(true, Ordering::Relaxed);
            }
        }
    });

    workers_done.store(true, Ordering::Relaxed);
    let _ = ticker.join();

    // 清理线程本地缓存（可选，释放内存）
    // THREAD_READER.with(...) 不跨线程强制清理；线程池复用时路径校验会自动重建。

    // ---- 收尾 ----
    let errs = errors.lock().unwrap().clone();
    // 统一 levels 摘要（双模式）
    let summary_levels: Vec<LevelSummary> = match (&pyramid, &mplan) {
        (Some(py), _) => py
            .levels
            .iter()
            .map(|lp| LevelSummary {
                level: lp.level,
                width: lp.width,
                height: lp.height,
                tiles: lp.tiles_x as u64 * lp.tiles_y as u64,
            })
            .collect(),
        (_, Some(mp)) => mp
            .levels
            .iter()
            .map(|lv| {
                let nx = (lv.tx1 - lv.tx0 + 1) as u32;
                let ny = (lv.ty1 - lv.ty0 + 1) as u32;
                LevelSummary {
                    level: lv.z,
                    width: nx.saturating_mul(params.tile_size),
                    height: ny.saturating_mul(params.tile_size),
                    tiles: nx as u64 * ny as u64,
                }
            })
            .collect(),
        _ => vec![],
    };
    let mut summary = CutSummary {
        output_dir: params.output.display().to_string(),
        total_tiles: done_tiles.load(Ordering::Relaxed),
        bytes_written: done_bytes.load(Ordering::Relaxed),
        elapsed_ms: started.elapsed().as_millis() as u64,
        cancelled: control.cancel.load(Ordering::Relaxed),
        errors: errs.clone(),
        levels: summary_levels.clone(),
    };

    if summary.errors.is_empty() && !summary.cancelled {
        let (min_lv, max_lv) = match (&pyramid, &mplan) {
            (Some(py), _) => (py.min_level_requested, py.max_level_requested),
            (_, Some(mp)) => (
                mp.levels.first().map(|l| l.z).unwrap_or(0),
                mp.levels.last().map(|l| l.z).unwrap_or(0),
            ),
            _ => (0, 0),
        };
        let manifest_levels: Vec<writer::ManifestLevel> = match (&pyramid, &mplan) {
            (Some(py), _) => py
                .levels
                .iter()
                .map(|lp| writer::ManifestLevel {
                    level: lp.level,
                    width: lp.width,
                    height: lp.height,
                    tiles: lp.tiles_x as u64 * lp.tiles_y as u64,
                })
                .collect(),
            (_, Some(mp)) => mp
                .levels
                .iter()
                .map(|lv| {
                    let nx = (lv.tx1 - lv.tx0 + 1) as u64;
                    let ny = (lv.ty1 - lv.ty0 + 1) as u64;
                    writer::ManifestLevel {
                        level: lv.z,
                        width: (nx as u32).saturating_mul(params.tile_size),
                        height: (ny as u32).saturating_mul(params.tile_size),
                        tiles: nx * ny,
                    }
                })
                .collect(),
            _ => vec![],
        };
        let manifest = writer::Manifest {
            app: "swCutter".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            source: params.source.display().to_string(),
            source_width: src_w,
            source_height: src_h,
            tile_size: params.tile_size,
            scheme: params.scheme.as_str().into(),
            min_level: min_lv,
            max_level: max_lv,
            levels: manifest_levels.clone(),
            total_tiles: summary.total_tiles,
            bytes_written: summary.bytes_written,
        };
        if let Err(e) = writer::write_manifest(&params.output, &manifest) {
            // manifest 写失败不推翻切片结果，仅记录
            summary.errors.push(format!("manifest 写入失败: {e}"));
        }
        // 生成浏览器预览页（失败仅记录，不影响结果）
        let pv = writer::PreviewInfo {
            source_w: src_w,
            source_h: src_h,
            tile_size: params.tile_size,
            zmin: min_lv,
            zmax: max_lv,
            tms: params.scheme == Scheme::Tms,
            levels: summary.levels.iter().map(|l| writer::ManifestLevel {
                level: l.level,
                width: l.width,
                height: l.height,
                tiles: l.tiles,
            }).collect(),
        };
        if let Err(e) = writer::write_preview_html(&params.output, &pv) {
            summary.errors.push(format!("preview.html 生成失败: {e}"));
        }
    }

    emit(CutEvent::Done(summary.clone()));
    summary
}

/// gdal2tiles 式概览瓦片：读取上一级 4 块子瓦片拼合后降采样。
/// 子块缺失（如跳过透明未写）按全透明处理，空白区域自然向下传播。
fn render_tile_overview(params: &CutParams, z: u32, tx: u32, ty: u32) -> CoreResult<u64> {
    let t = params.tile_size;
    let mut canvas = image::RgbaImage::new(t * 2, t * 2);
    for (dy, dx) in [(0u32, 0u32), (0, 1), (1, 0), (1, 1)] {
        let child_rel = writer::tile_rel_path(
            params.scheme,
            z + 1,
            tx * 2 + dx,
            ty * 2 + dy,
            1u32 << (z + 1).min(30),
        );
        let p = params.output.join(&child_rel);
        if let Ok(bytes) = std::fs::read(&p) {
            if let Ok(img) = image::load_from_memory(&bytes) {
                let rgba = img.to_rgba8();
                image::imageops::overlay(&mut canvas, &rgba, (dx * t) as i64, (dy * t) as i64);
            }
        }
    }
    let out = image::imageops::resize(
        &canvas,
        t,
        t,
        image::imageops::FilterType::Triangle,
    );
    let mut rgba = out.into_raw();
    let rel = writer::tile_rel_path(params.scheme, z, tx, ty, 1u32 << z.min(30));
    write_tile_png(params, rgba, t, t, rel)
}

// ---------------- 线程本地读取器 ----------------

thread_local! {
    static THREAD_READER: std::cell::RefCell<Option<SourceReader>> =
        const { std::cell::RefCell::new(None) };
}

fn ensure_thread_reader(path: &Path) -> CoreResult<()> {
    THREAD_READER.with(|cell| {
        let mut slot = cell.borrow_mut();
        let need_reopen = match slot.as_ref() {
            Some(r) => r.path() != path,
            None => true,
        };
        if need_reopen {
            *slot = Some(SourceReader::open(path)?);
        }
        Ok(())
    })
}

/// Mercator 绝对级别瓦片渲染：全球网格 → 源像素矩形 → 缩放至 tile。
/// 影像范围外的区域输出全透明（对齐 gdal2tiles 的空白透明 PNG 行为）。
#[allow(clippy::too_many_arguments)]
fn render_tile_mercator(
    reader: &mut SourceReader,
    params: &CutParams,
    g: super::meta::GeoRef,
    bounds: [f64; 4],
    z: u32,
    tx: u32,
    ty: u32,
) -> CoreResult<u64> {
    let _ = bounds; // bounds 仅用于规划；渲染由 georef 反算
    let t = params.tile_size as f64;
    let res = super::mercator::INIT_RESOLUTION / 2f64.powi(z as i32);
    // 该瓦片的 3857 范围
    let wx_l = tx as f64 * t * res - super::mercator::ORIGIN_SHIFT;
    let wx_r = (tx + 1) as f64 * t * res - super::mercator::ORIGIN_SHIFT;
    let wy_top = super::mercator::ORIGIN_SHIFT - ty as f64 * t * res;
    let wy_bot = super::mercator::ORIGIN_SHIFT - (ty + 1) as f64 * t * res;

    // 反算到源栅格像素坐标（浮点）
    let fx0 = (wx_l - g.mx0) / g.sx;
    let fx1 = (wx_r - g.mx0) / g.sx;
    // my_top 是北边界；sy 为负时直接线性映射行号
    let fy_top = (wy_top - g.my_top) / g.sy;
    let fy_bot = (wy_bot - g.my_top) / g.sy;

    let rx = fx0.min(fx1).floor() as i64;
    let rw = ((fx0.max(fx1)).ceil() as i64 - rx).max(1) as u32;
    let ry = fy_top.min(fy_bot).floor() as i64;
    let rh = ((fy_top.max(fy_bot)).ceil() as i64 - ry).max(1) as u32;

    // 与源图求交（完全在外 → 全透明瓦片）
    let ix0 = rx.clamp(0, reader.width as i64);
    let iy0 = ry.clamp(0, reader.height as i64);
    let ix1 = (rx + rw as i64).clamp(0, reader.width as i64);
    let iy1 = (ry + rh as i64).clamp(0, reader.height as i64);
    let iw = (ix1 - ix0).max(0) as u32;
    let ih = (iy1 - iy0).max(0) as u32;

    let crop = if iw == 0 || ih == 0 {
        vec![0u8; 4]
    } else {
        reader.read_rect(ix0, iy0, iw, ih)?
    };
    let mut img = image::RgbaImage::from_raw(iw.max(1), ih.max(1), crop)
        .ok_or_else(|| CoreError::Encoding("mercator 矩形缓冲不匹配".into()))?;

        let flt = resample_filter(params.resample);
    let out_img = if img.width() == params.tile_size && img.height() == params.tile_size {
        img
    } else if needs_smart(img.width(), img.height(), params.tile_size, params.tile_size) {
        smart_downscale(img, params.tile_size, params.tile_size, flt)
    } else {
        image::imageops::resize(&img, params.tile_size, params.tile_size, flt)
    };
    let rgba = out_img.into_raw();
    let rel = writer::tile_rel_path(params.scheme, z, tx, ty, 1u32 << z.min(30));
    write_tile_png(params, rgba, params.tile_size, params.tile_size, rel)
}

// ---------------- 进度心跳 ----------------

struct TickerShared {
    sink: Arc<Mutex<dyn FnMut(CutEvent) + Send>>,
    done_tiles: Arc<AtomicU64>,
    done_bytes: Arc<AtomicU64>,
    current_level: Arc<AtomicU32>,
    control: Arc<TaskControl>,
    workers_done: Arc<AtomicBool>,
    total: u64,
    /// 任务起始时刻（用于实时 elapsed/ETA）
    started: Instant,
}

fn ticker_loop(st: TickerShared) {
    let mut last_level: Option<u32> = None;
    loop {
        thread::sleep(TICK);
        let lvl_raw = st.current_level.load(Ordering::Relaxed);
        let lvl = if lvl_raw == u32::MAX { 0 } else { lvl_raw };
        if Some(lvl) != last_level {
            last_level = Some(lvl);
            let ev = CutEvent::LevelStart { level: lvl, tiles: 0 };
            if let Ok(mut f) = st.sink.lock() {
                f(ev);
            }
        }
        let snap = ProgressSnapshot {
            level: lvl,
            tiles_done: st.done_tiles.load(Ordering::Relaxed),
            total_tiles: st.total,
            bytes_written: st.done_bytes.load(Ordering::Relaxed),
            elapsed_ms: st.started.elapsed().as_millis() as u64,
        };
        if let Ok(mut f) = st.sink.lock() {
            f(CutEvent::Progress(snap));
        }
        if st.workers_done.load(Ordering::Relaxed) || st.control.cancel.load(Ordering::Relaxed) {
            break;
        }
    }
}

// ---------------- 单瓦片渲染 ----------------

/// 从共享全图光栅复制矩形（语义同 SourceReader::read_rect：越界填 0）。
fn copy_rect_from_full(
    full: &[u8],
    w: u32,
    h: u32,
    rx: i64,
    ry: i64,
    rw: u32,
    rh: u32,
) -> Vec<u8> {
    let mut out = vec![0u8; rw as usize * rh as usize * 4];
    if rx >= w as i64 || ry >= h as i64 || rx + rw as i64 <= 0 || ry + rh as i64 <= 0 {
        return out;
    }
    let x0 = rx.clamp(0, w as i64 - 1);
    let y0 = ry.clamp(0, h as i64 - 1);
    let x1 = (rx + rw as i64 - 1).min(w as i64 - 1);
    let y1 = (ry + rh as i64 - 1).min(h as i64 - 1);
    for row in y0..=y1 {
        let src = ((row as u32 * w) + x0 as u32) as usize * 4;
        let dst_row = (row - ry) as usize;
        let dst_col = (x0 - rx) as usize;
        let dst = (dst_row * rw as usize + dst_col) * 4;
        let span = (x1 - x0 + 1) as usize * 4;
        out[dst..dst + span].copy_from_slice(&full[src..src + span]);
    }
    out
}

/// 统一收尾：alpha → 跳空判断 → PNG 编码落盘。
fn write_tile_png(
    params: &CutParams,
    mut rgba: Vec<u8>,
    out_w: u32,
    out_h: u32,
    rel: PathBuf,
) -> CoreResult<u64> {
    params.alpha.apply(&mut rgba);

    if params.skip_empty {
        let mut any_opaque = false;
        let mut i = 3;
        while i < rgba.len() {
            if rgba[i] != 0 {
                any_opaque = true;
                break;
            }
            i += 4;
        }
        if !any_opaque {
            return Ok(0);
        }
    }

    let path = params.output.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| super::error::io_err(parent.display().to_string(), e))?;
    }
    let mut png: Vec<u8> = Vec::with_capacity((rgba.len() / 4) as usize);
    {
        let encoder = PngEncoder::new_with_quality(
            Cursor::new(&mut png),
            CompressionType::Fast,
            FilterType::Sub,
        );
        encoder
            .write_image(&rgba, out_w, out_h, ExtendedColorType::Rgba8)
            .map_err(|e| CoreError::Encoding(e.to_string()))?;
    }
    std::fs::write(&path, &png).map_err(|e| super::error::io_err(path.display().to_string(), e))?;
    Ok(png.len() as u64)
}

fn resample_filter(resample: Resample) -> ImgFilter {
    match resample {
        Resample::Nearest => ImgFilter::Nearest,
        Resample::Bilinear => ImgFilter::Triangle,
    }
}

/// 大比例降采样：先反复折半（每步 O(n)，窗口恒定 2px 支撑）到接近目标，
/// 再用指定滤波器做最后一步。避免直接 300MP→65KP 时滤波窗口爆炸的长尾。
fn smart_downscale(
    img: image::RgbaImage,
    tw: u32,
    th: u32,
    filter: ImgFilter,
) -> image::RgbaImage {
    let mut cur = img;
    // 折半直到任一边小于目标 4 倍（保留一步精细滤波空间）
    while cur.width() >= tw.saturating_mul(4) && cur.height() >= th.saturating_mul(4) {
        let nw = (cur.width() / 2).max(tw);
        let nh = (cur.height() / 2).max(th);
        cur = image::imageops::resize(
            &cur,
            nw,
            nh,
            image::imageops::FilterType::Triangle,
        );
    }
    if cur.width() == tw && cur.height() == th {
        cur
    } else {
        image::imageops::resize(&cur, tw, th, filter)
    }
}

/// 需要智能降采样时返回 true（源区域远大于输出）
fn needs_smart(w: u32, h: u32, tw: u32, th: u32) -> bool {
    w >= tw.saturating_mul(4) && h >= th.saturating_mul(4)
}

fn render_tile(
    reader: &mut SourceReader,
    params: &CutParams,
    lp: &LevelPlan,
    src_w: u32,
    src_h: u32,
    tx: u32,
    ty: u32,
) -> CoreResult<u64> {
    let t = params.tile_size;
    let out_w = (lp.width - tx * t).min(t);
    let out_h = (lp.height - ty * t).min(t);
    if out_w == 0 || out_h == 0 {
        return Err(CoreError::InvalidInput("空瓦片".into()));
    }

    let sf = lp.scale; // 源px/输出px，可 <1（放大级别）
    let sx = (tx as f64 * t as f64 * sf).round() as i64;
    let sy = (ty as f64 * t as f64 * sf).round() as i64;

    // 源侧区域：不超过图像边界（避免边缘瓦片混入黑边）
    let sw = (((out_w as f64 * sf).ceil() as i64).min(src_w as i64 - sx)).max(1);
    let sh = (((out_h as f64 * sf).ceil() as i64).min(src_h as i64 - sy)).max(1);

    // 为重采样补一圈采样余量（越界部分由 read_rect 填充，随后裁掉）
    let pad: i64 = if sf > 1.0 { sf.ceil() as i64 } else { 1 };
    let rx = sx - pad;
    let ry = sy - pad;
    let rw = (sw + pad * 2) as u32;
    let rh = (sh + pad * 2) as u32;

    let buf = reader.read_rect(rx, ry, rw, rh)?;
    let full = RgbaImage::from_raw(rw, rh, buf)
        .ok_or_else(|| CoreError::Encoding("矩形缓冲尺寸不匹配".into()))?;

    // 裁回精确源区域
    let cx = (sx - rx) as u32;
    let cy = (sy - ry) as u32;
    let cropped =
        image::imageops::crop_imm(&full, cx, cy, sw as u32, sh as u32).to_image();

    // 重采样到目标尺寸（含放大级别）
    let out_img = if (sf - 1.0).abs() < f64::EPSILON && cropped.width() == out_w && cropped.height() == out_h {
        cropped
    } else {
        let filter = match params.resample {
            Resample::Nearest => ImgFilter::Nearest,
            Resample::Bilinear => ImgFilter::Triangle,
        };
        image::imageops::resize(&cropped, out_w, out_h, filter)
    };

    let mut rgba = out_img.into_raw();
    let rel = writer::tile_rel_path(params.scheme, lp.level, tx, ty, lp.tiles_y);
    write_tile_png(params, rgba, out_w, out_h, rel)
}

/// 相对模式 · 共享全图路径（巨型条带源）
fn render_tile_from_full(
    full: &[u8],
    src_w: u32,
    src_h: u32,
    params: &CutParams,
    lp: &super::planner::LevelPlan,
    tx: u32,
    ty: u32,
) -> CoreResult<u64> {
    let t = params.tile_size;
    let out_w = (lp.width - tx * t).min(t);
    let out_h = (lp.height - ty * t).min(t);
    if out_w == 0 || out_h == 0 {
        return Err(CoreError::InvalidInput("空瓦片".into()));
    }
    let sf = lp.scale;
    let sx = (tx as f64 * t as f64 * sf).round() as i64;
    let sy = (ty as f64 * t as f64 * sf).round() as i64;
    let sw = (((out_w as f64 * sf).ceil() as i64).min(src_w as i64 - sx)).max(1);
    let sh = (((out_h as f64 * sf).ceil() as i64).min(src_h as i64 - sy)).max(1);
    let pad: i64 = if sf > 1.0 { sf.ceil() as i64 } else { 1 };
    let rx = sx - pad;
    let ry = sy - pad;
    let rw = (sw + pad * 2) as u32;
    let rh = (sh + pad * 2) as u32;

    let buf = copy_rect_from_full(full, src_w, src_h, rx, ry, rw, rh);
    let img_full = image::RgbaImage::from_raw(rw, rh, buf)
        .ok_or_else(|| CoreError::Encoding("缓冲不匹配".into()))?;
    let cropped = image::imageops::crop_imm(
        &img_full,
        (sx - rx) as u32,
        (sy - ry) as u32,
        sw as u32,
        sh as u32,
    )
    .to_image();
    let out_img = if (sf - 1.0).abs() < f64::EPSILON
        && cropped.width() == out_w
        && cropped.height() == out_h
    {
        cropped
    } else {
        image::imageops::resize(&cropped, out_w, out_h, resample_filter(params.resample))
    };
    let mut rgba = out_img.into_raw();
    let rel = writer::tile_rel_path(params.scheme, lp.level, tx, ty, lp.tiles_y);
    write_tile_png(params, rgba, out_w, out_h, rel)
}

/// Mercator 模式 · 共享全图路径
#[allow(clippy::too_many_arguments)]
fn render_tile_mercator_from_full(
    full: &[u8],
    src_w: u32,
    src_h: u32,
    params: &CutParams,
    g: super::meta::GeoRef,
    z: u32,
    tx: u32,
    ty: u32,
) -> CoreResult<u64> {
    let t = params.tile_size as f64;
    let res = super::mercator::INIT_RESOLUTION / 2f64.powi(z as i32);
    let wx_l = tx as f64 * t * res - super::mercator::ORIGIN_SHIFT;
    let wx_r = (tx + 1) as f64 * t * res - super::mercator::ORIGIN_SHIFT;
    let wy_top = super::mercator::ORIGIN_SHIFT - ty as f64 * t * res;
    let wy_bot = super::mercator::ORIGIN_SHIFT - (ty + 1) as f64 * t * res;
    let fx0 = (wx_l - g.mx0) / g.sx;
    let fx1 = (wx_r - g.mx0) / g.sx;
    let fy_top = (wy_top - g.my_top) / g.sy;
    let fy_bot = (wy_bot - g.my_top) / g.sy;
    let rx = fx0.min(fx1).floor() as i64;
    let rw = ((fx0.max(fx1)).ceil() as i64 - rx).max(1) as u32;
    let ry = fy_top.min(fy_bot).floor() as i64;
    let rh = ((fy_top.max(fy_bot)).ceil() as i64 - ry).max(1) as u32;

    let crop = copy_rect_from_full(full, src_w, src_h, rx, ry, rw, rh);
    let w = rw.max(1);
    let h = rh.max(1);
    let img = image::RgbaImage::from_raw(w, h, crop)
        .ok_or_else(|| CoreError::Encoding("mercator 缓冲不匹配".into()))?;
    let out_img = if img.width() == params.tile_size && img.height() == params.tile_size {
        img
    } else {
        image::imageops::resize(
            &img,
            params.tile_size,
            params.tile_size,
            resample_filter(params.resample),
        )
    };
    let mut rgba = out_img.into_raw();
    let rel = writer::tile_rel_path(params.scheme, z, tx, ty, 1u32 << z.min(30));
    write_tile_png(params, rgba, params.tile_size, params.tile_size, rel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::tiff::encoder::{colortype, Compression, TiffEncoder};
    use std::fs::File;

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("swcutter_{}_{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// 生成 w×h RGB TIFF（PackBits → 每行一条 strip）。
    /// 像素公式：R=x*7%256, G=y*11%256, B=(x+y)%256；返回期望的整幅 RGBA。
    fn fixture(path: &Path, w: u32, h: u32) -> Vec<u8> {
        let n = (w * h) as usize;
        let mut data = vec![0u8; n * 3];
        let mut exp = vec![0u8; n * 4];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                let (r, g, b) = ((x * 7 % 256) as u8, (y * 11 % 256) as u8, ((x + y) % 256) as u8);
                data[i * 3] = r;
                data[i * 3 + 1] = g;
                data[i * 3 + 2] = b;
                exp[i * 4] = r;
                exp[i * 4 + 1] = g;
                exp[i * 4 + 2] = b;
                exp[i * 4 + 3] = 255;
            }
        }
        let f = File::create(path).unwrap();
        TiffEncoder::new(f)
            .unwrap()
            .with_compression(Compression::Packbits)
            .write_image::<colortype::RGB8>(w, h, &data)
            .unwrap();
        exp
    }

    fn noop_sink() -> Arc<Mutex<dyn FnMut(CutEvent) + Send>> {
        Arc::new(Mutex::new(|_: CutEvent| {}))
    }

    fn load_png(p: &Path) -> Vec<u8> {
        let img = image::open(p).expect("open png").to_rgba8();
        img.into_raw()
    }

    #[test]
    fn cut_end_to_end() {
        let dir = tmp_dir("e2e");
        let src = dir.join("源 文件.tif"); // 中文与空格路径
        let w = 600u32;
        let h = 400u32;
        let exp = fixture(&src, w, h);

        // ---- XYZ 全级别 ----
        let out = dir.join("out_xyz");
        let params = CutParams {
            source: src.clone(),
            output: out.clone(),
            tile_size: 256,
            zmin: None,
            zmax: None,
            scheme: Scheme::Xyz,
            alpha: AlphaMode::Keep,
            resample: Resample::Nearest,
            skip_empty: false,
            mercator: false,
        };
        let sum = run_cut(&params, noop_sink());
        assert!(sum.errors.is_empty(), "errors: {:?}", sum.errors);
        assert!(!sum.cancelled);
        // max_level = ceil(log2(600/256)) = 2
        // L0: 150x100 -> 1 瓦片 | L1: 300x200 -> 2 | L2: 600x400 -> 3x2=6
        assert_eq!(sum.total_tiles, 1 + 2 + 6);
        assert_eq!(sum.levels.len(), 3);
        assert!(out.join("manifest.json").exists());

        // 原生级 L2 直接裁切：瓦片 (0,0) 与源逐像素一致
        let t00 = load_png(&out.join("2").join("0").join("0.png"));
        assert_eq!(t00.len(), 256 * 256 * 4);
        for &(px, py) in &[(0usize, 0usize), (5, 7), (255, 255)] {
            let si = ((py as u32 * w + px as u32) * 4) as usize;
            let ti = (py * 256 + px) * 4;
            assert_eq!(&t00[ti..ti + 4], &exp[si..si + 4], "pixel ({px},{py})");
        }
        // 边缘瓦片 (2,0)：宽度 600-512=88
        let p20 = out.join("2").join("2").join("0.png");
        let t20 = load_png(&p20);
        let img20 = image::open(&p20).unwrap();
        assert_eq!((img20.width(), img20.height()), (88, 256));
        let si = ((0u32 * w + (512 + 87) as u32) * 4) as usize; // 源 (599, 0)
        let ti = (0 * 88 + 87) * 4;
        assert_eq!(&t20[ti..ti + 4], &exp[si..si + 4]);

        // ---- TMS：Y 翻转，内容与 XYZ 对应瓦片一致 ----
        let out_tms = dir.join("out_tms");
        let params_tms = CutParams {
            output: out_tms.clone(),
            scheme: Scheme::Tms,
            ..params.clone()
        };
        let sum_tms = run_cut(&params_tms, noop_sink());
        assert!(sum_tms.errors.is_empty(), "{:?}", sum_tms.errors);
        // L2 tiles_y=2 → ty=0 写入 y=1；翻转是双射，y=0 对应 XYZ 的 ty=1
        let xyz00 = std::fs::read(out.join("2").join("0").join("0.png")).unwrap();
        let tms01 = std::fs::read(out_tms.join("2").join("0").join("1.png")).unwrap();
        assert_eq!(xyz00, tms01);
        let xyz01 = std::fs::read(out.join("2").join("0").join("1.png")).unwrap();
        let tms00 = std::fs::read(out_tms.join("2").join("0").join("0.png")).unwrap();
        assert_eq!(xyz01, tms00);

        // 浏览器预览页已生成且包含配置注入
        let pv = std::fs::read_to_string(out.join("preview.html")).unwrap();
        assert!(pv.contains("swCutter 预览"));
        assert!(pv.contains(r#""w":600"#));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cut_with_alpha_colorkey_and_partial_levels() {
        let dir = tmp_dir("alpha");
        let src = dir.join("a.tif");
        // 300x300 白底 + 一条黑线
        let mut data = vec![255u8; 300 * 300 * 3];
        for x in 0..300 {
            let i = (150 * 300 + x) as usize;
            data[i * 3] = 0;
            data[i * 3 + 1] = 0;
            data[i * 3 + 2] = 0;
        }
        let f = File::create(&src).unwrap();
        TiffEncoder::new(f)
            .unwrap()
            .with_compression(Compression::Packbits)
            .write_image::<colortype::RGB8>(300, 300, &data)
            .unwrap();

        let out = dir.join("out");
        let params = CutParams {
            source: src,
            output: out.clone(),
            tile_size: 256,
            zmin: Some(1), // 只切中高两级
            zmax: None,
            scheme: Scheme::Xyz,
            alpha: AlphaMode::ColorKey { r: 255, g: 255, b: 255, tolerance: 2 },
            resample: Resample::Bilinear,
            skip_empty: false,
            mercator: false,
        };
        let sum = run_cut(&params, noop_sink());
        assert!(sum.errors.is_empty(), "{:?}", sum.errors);
        // max_level=1（原生级）；只切 L1: 300x300 → 2x2 = 4 瓦片
        assert_eq!(sum.total_tiles, 4);

        let t = load_png(&out.join("1").join("0").join("0.png"));
        // 原生级 1:1 裁切：白底像素 → 透明
        assert_eq!(t[(10 * 256 + 10) * 4 + 3], 0);
        // 黑线在源 y=150 → 不透明黑
        let idx = (150 * 256 + 128) * 4;
        assert_eq!(t[idx], 0);
        assert_eq!(t[idx + 3], 255);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resume_skips_existing_tiles() {
        let dir = tmp_dir("resume");
        let src = dir.join("r.tif");
        let exp = fixture(&src, 600, 400);

        let params = CutParams {
            source: src,
            output: dir.join("out"),
            tile_size: 256,
            zmin: None,
            zmax: None,
            scheme: Scheme::Xyz,
            alpha: AlphaMode::Keep,
            resample: Resample::Nearest,
            skip_empty: false,
            mercator: false,
        };

        // 首次完整切片
        let s1 = run_cut(&params, noop_sink());
        assert!(s1.errors.is_empty(), "{:?}", s1.errors);
        assert_eq!(s1.total_tiles, 9);

        let l0_tile = params.output.join("0").join("0").join("0.png");
        let mtime_before = std::fs::metadata(&l0_tile).unwrap().modified().unwrap();

        // 删除 L2 全部瓦片，模拟中断
        std::fs::remove_dir_all(params.output.join("2")).unwrap();

        // 续切：L0/L1 应被跳过（mtime 不变），L2 重建
        let s2 = run_cut(&params, noop_sink());
        assert!(s2.errors.is_empty(), "{:?}", s2.errors);
        assert_eq!(s2.total_tiles, 9);
        assert_eq!(s2.levels.len(), 3);

        let mtime_after = std::fs::metadata(&l0_tile).unwrap().modified().unwrap();
        assert_eq!(
            mtime_before, mtime_after,
            "已有瓦片不应被重写"
        );
        // L2 瓦片恢复且像素正确
        let t20 = load_png(&params.output.join("2").join("2").join("0.png"));
        let si = ((0u32 * 600 + (512 + 87) as u32) * 4) as usize;
        assert_eq!(&t20[(87) * 4..(87) * 4 + 4], &exp[si..si + 4]);

        // 参数变更 → 不再续切，全部重写
        let mut p2 = params.clone();
        p2.scheme = Scheme::Tms;
        let s3 = run_cut(&p2, noop_sink());
        assert_eq!(s3.total_tiles, 9);
        let mtime_tms = std::fs::metadata(
            p2.output.join("0").join("0").join("0.png"),
        )
        .unwrap()
        .modified()
        .unwrap();
        assert_ne!(mtime_before, mtime_tms, "参数变化应全量重切");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
