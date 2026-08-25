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
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProgressSnapshot {
    pub level: u32,
    pub tiles_done: u64,
    pub total_tiles: u64,
    pub bytes_written: u64,
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

const ERROR_CAP: usize = 16;
/// 进度上报间隔
const TICK: Duration = Duration::from_millis(120);

struct Job {
    plan_idx: usize,
    tx: u32,
    ty: u32,
}

/// 执行切片（同步阻塞）。`sink` 会在工作线程上被调用，需自行保证线程安全。
pub fn run_cut(params: &CutParams, sink: Arc<Mutex<dyn FnMut(CutEvent) + Send>>) -> CutSummary {
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
    let pyramid = match plan(src_w, src_h, params.tile_size, params.zmin, params.zmax) {
        Ok(p) => p,
        Err(e) => {
            let s = summary_err(e.to_string());
            emit(CutEvent::Done(s.clone()));
            return s;
        }
    };

    if let Err(e) = writer::ensure_out_dir(&params.output) {
        let s = summary_err(super::error::io_err(params.output.display().to_string(), e).to_string());
        emit(CutEvent::Done(s.clone()));
        return s;
    }

    // ---- 任务列表（按级别升序） ----
    let mut jobs = Vec::with_capacity(pyramid.total_tiles as usize);
    for (pi, lp) in pyramid.levels.iter().enumerate() {
        for ty in 0..lp.tiles_y {
            for tx in 0..lp.tiles_x {
                jobs.push(Job { plan_idx: pi, tx, ty });
            }
        }
    }
    let total = jobs.len() as u64;
    emit(CutEvent::Start { total_tiles: total });

    // ---- 共享状态 ----
    let done_tiles = Arc::new(AtomicU64::new(0));
    let done_bytes = Arc::new(AtomicU64::new(0));
    let current_level = Arc::new(AtomicU32::new(u32::MAX));
    let cancelled = Arc::new(AtomicBool::new(false));
    let workers_done = Arc::new(AtomicBool::new(false));
    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    // ---- 进度心跳线程 ----
    let ticker_state = TickerShared {
        sink: Arc::clone(&sink),
        done_tiles: Arc::clone(&done_tiles),
        done_bytes: Arc::clone(&done_bytes),
        current_level: Arc::clone(&current_level),
        cancelled: Arc::clone(&cancelled),
        workers_done: Arc::clone(&workers_done),
        total,
    };
    let ticker = thread::spawn(move || ticker_loop(ticker_state));

    let src_path = params.source.clone();
    let p_ref = params;
    let py_ref = &pyramid;

    jobs.par_iter().for_each(|job| {
        if cancelled.load(Ordering::Relaxed) {
            return;
        }
        let lp = &pyramid.levels[job.plan_idx];
        current_level.fetch_max(lp.level, Ordering::Relaxed);

        let result = THREAD_READER.with(|cell| {
            ensure_thread_reader(&src_path)?;
            let mut slot = cell.borrow_mut();
            let reader = slot.as_mut().expect("thread reader prepared");
            render_tile(
                reader,
                p_ref,
                lp,
                py_ref.image_width,
                py_ref.image_height,
                job.tx,
                job.ty,
            )
        });

        match result {
            Ok(bytes) => {
                done_tiles.fetch_add(1, Ordering::Relaxed);
                done_bytes.fetch_add(bytes, Ordering::Relaxed);
            }
            Err(e) => {
                let mut errs = errors.lock().unwrap();
                if errs.len() < ERROR_CAP {
                    errs.push(format!(
                        "L{} tile({},{}) 失败: {e}",
                        lp.level, job.tx, job.ty
                    ));
                }
                drop(errs);
                cancelled.store(true, Ordering::Relaxed);
            }
        }
    });

    workers_done.store(true, Ordering::Relaxed);
    let _ = ticker.join();

    // 清理线程本地缓存（可选，释放内存）
    // THREAD_READER.with(...) 不跨线程强制清理；线程池复用时路径校验会自动重建。

    // ---- 收尾 ----
    let errs = errors.lock().unwrap().clone();
    let mut summary = CutSummary {
        output_dir: params.output.display().to_string(),
        total_tiles: done_tiles.load(Ordering::Relaxed),
        bytes_written: done_bytes.load(Ordering::Relaxed),
        elapsed_ms: started.elapsed().as_millis() as u64,
        cancelled: cancelled.load(Ordering::Relaxed),
        errors: errs.clone(),
        levels: pyramid
            .levels
            .iter()
            .map(|lp| LevelSummary {
                level: lp.level,
                width: lp.width,
                height: lp.height,
                tiles: lp.tiles_x as u64 * lp.tiles_y as u64,
            })
            .collect(),
    };

    if summary.errors.is_empty() && !summary.cancelled {
        let manifest = writer::Manifest {
            app: "swCutter",
            version: env!("CARGO_PKG_VERSION"),
            source: params.source.display().to_string(),
            source_width: src_w,
            source_height: src_h,
            tile_size: params.tile_size,
            scheme: params.scheme.as_str().into(),
            min_level: pyramid.min_level_requested,
            max_level: pyramid.max_level_requested,
            levels: pyramid
                .levels
                .iter()
                .map(|lp| writer::ManifestLevel {
                    level: lp.level,
                    width: lp.width,
                    height: lp.height,
                    tiles: lp.tiles_x as u64 * lp.tiles_y as u64,
                })
                .collect(),
            total_tiles: summary.total_tiles,
            bytes_written: summary.bytes_written,
        };
        if let Err(e) = writer::write_manifest(&params.output, &manifest) {
            // manifest 写失败不推翻切片结果，仅记录
            summary.errors.push(format!("manifest 写入失败: {e}"));
        }
    }

    emit(CutEvent::Done(summary.clone()));
    summary
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

// ---------------- 进度心跳 ----------------

struct TickerShared {
    sink: Arc<Mutex<dyn FnMut(CutEvent) + Send>>,
    done_tiles: Arc<AtomicU64>,
    done_bytes: Arc<AtomicU64>,
    current_level: Arc<AtomicU32>,
    cancelled: Arc<AtomicBool>,
    workers_done: Arc<AtomicBool>,
    total: u64,
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
        };
        if let Ok(mut f) = st.sink.lock() {
            f(CutEvent::Progress(snap));
        }
        if st.workers_done.load(Ordering::Relaxed) || st.cancelled.load(Ordering::Relaxed) {
            break;
        }
    }
}

// ---------------- 单瓦片渲染 ----------------

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

    let s = lp.downscale.max(1);
    let sx = (tx * t) as i64 * s as i64;
    let sy = (ty * t) as i64 * s as i64;

    // 源侧区域：不超过图像边界（避免边缘瓦片混入黑边）
    let sw = ((out_w as i64 * s as i64).min(src_w as i64 - sx)).max(1);
    let sh = ((out_h as i64 * s as i64).min(src_h as i64 - sy)).max(1);

    // 为重采样补一圈采样余量（越界部分由 read_rect 填充，随后裁掉）
    let pad: i64 = if s > 1 { s as i64 } else { 1 };
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

    // 重采样到目标尺寸
    let out_img = if s == 1 {
        cropped
    } else {
        let filter = match params.resample {
            Resample::Nearest => ImgFilter::Nearest,
            Resample::Bilinear => ImgFilter::Triangle,
        };
        image::imageops::resize(&cropped, out_w, out_h, filter)
    };

    let mut rgba = out_img.into_raw();
    params.alpha.apply(&mut rgba);

    // 编码 PNG
    let path = params
        .output
        .join(writer::tile_rel_path(params.scheme, lp.level, tx, ty, lp.tiles_y));
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
}
