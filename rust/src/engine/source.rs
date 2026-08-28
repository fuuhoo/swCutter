//! 分块读取的 TIFF 源读取器：按需解码 chunk + 小容量 LRU 缓存，
//! 对外提供任意源矩形 → RGBA8 的 `read_rect`，内存占用有界。

use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use ::tiff::decoder::{ChunkType, Decoder};
use ::tiff::ColorType;

use super::error::{io_err, CoreError, CoreResult};
use super::meta;

/// LRU 缓存字节预算（约 160MB）
const CACHE_BUDGET_BYTES: usize = 160 * 1024 * 1024;

struct ChunkCanvas {
    /// RGBA8 行主序
    rgba: Vec<u8>,
}

pub struct SourceReader {
    path: PathBuf,
    dec: Decoder<File>,
    pub width: u32,
    pub height: u32,
    colortype: ColorType,
    chunk_type: ChunkType,
    /// 条带高度（strip 模式）或瓦片边长（tile 模式）
    chunk_w: u32,
    chunk_h: u32,
    chunks_across: u32,
    chunk_count: u32,
    cache: HashMap<u32, ChunkCanvas>,
    /// 每个 chunk 的实际数据尺寸（边缘 chunk 小于标称值）
    dims: HashMap<u32, (u32, u32)>,
    order: VecDeque<u32>,
    cache_bytes: usize,
}

impl SourceReader {
    pub fn open(path: &Path) -> CoreResult<Self> {
        let file = File::open(path).map_err(|e| io_err(path.display().to_string(), e))?;
        let mut dec: Decoder<File> = Decoder::new(file)?;
        let (width, height) = dec.dimensions()?;
        if width == 0 || height == 0 {
            return Err(CoreError::Unsupported("空图像".into()));
        }
        let colortype = dec.colortype()?;
        match colortype {
            ColorType::Gray(_) | ColorType::GrayA(_) | ColorType::RGB(_) | ColorType::RGBA(_)
            | ColorType::CMYK(_) => {}
            ColorType::Palette(_) => {
                return Err(CoreError::Unsupported(
                    "调色板(Palette) TIFF 暂不支持，请先转换为 RGB/RGBA".into(),
                ))
            }
            other => {
                return Err(CoreError::Unsupported(format!("像素格式 {other:?} 暂不支持")));
            }
        }
        if dec
            .get_tag_u32(::tiff::tags::Tag::PlanarConfiguration)
            .unwrap_or(1)
            != 1
        {
            return Err(CoreError::Unsupported("平面(Planar)分块暂不支持".into()));
        }

        let chunk_type = dec.get_chunk_type();
        let (chunk_w, chunk_h, chunks_across, chunk_count) = match chunk_type {
            ChunkType::Strip => {
                let rps = dec
                    .get_tag_u32(::tiff::tags::Tag::RowsPerStrip)
                    .unwrap_or(height)
                    .clamp(1, height);
                let count = dec.strip_count()?;
                (width, rps.max(1), 1, count.max(1))
            }
            ChunkType::Tile => {
                let (tw, th) = dec.chunk_dimensions();
                let across = width.div_ceil(tw);
                let count = dec.tile_count()?;
                (tw, th, across, count)
            }
        };

        Ok(Self {
            path: path.to_path_buf(),
            dec,
            width,
            height,
            colortype,
            chunk_type,
            chunk_w,
            chunk_h,
            chunks_across,
            chunk_count,
            cache: HashMap::new(),
            dims: HashMap::new(),
            order: VecDeque::new(),
            cache_bytes: 0,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 单条带解码字节数超过该阈值视为「巨型条带」（逐块解码会反复整图解压）。
    pub fn giant_strip_bytes(&self) -> u64 {
        if self.chunk_type != ChunkType::Strip {
            return 0;
        }
        (self.chunk_w as u64) * (self.chunk_h as u64) * 4
    }

    /// 整图 RGBA（一次性拼装全部 chunk，绕过 LRU）。
    /// 用于巨型条带源：与其每块瓦片重复解压，不如全图驻留一份共享。
    /// `cancel`：可选取消旗标——逐 chunk 检查，命中即返回 Cancelled。
    pub fn read_full_cancellable(&mut self, cancel: Option<&AtomicBool>) -> CoreResult<Vec<u8>> {
        let (w, h) = (self.width as usize, self.height as usize);
        let mut out = vec![0u8; w * h * 4];
        for ci in 0..self.chunk_count {
            if let Some(f) = cancel {
                if f.load(Ordering::Relaxed) {
                    return Err(CoreError::Cancelled);
                }
            }
            let (cox, coy) = self.chunk_origin(ci);
            if cox >= self.width || coy >= self.height {
                continue;
            }
            let (dw, dh) = self.dec.chunk_data_dimensions(ci);
            let res = self.dec.read_chunk(ci)?;
            let raw8 = meta::result_to_u8(&res)
                .ok_or_else(|| CoreError::Unsupported("64 位样本".into()))?;
            let rgba = convert_to_rgba(&raw8, self.colortype, dw as usize, dh as usize)?;
            let cw = dw.min(self.width - cox) as usize;
            let ch = dh.min(self.height - coy) as usize;
            for row in 0..ch {
                let src = row * dw as usize * 4;
                let dst = ((coy as usize + row) * w + cox as usize) * 4;
                let span = cw * 4;
                out[dst..dst + span].copy_from_slice(&rgba[src..src + span]);
            }
        }
        Ok(out)
    }

    /// 确保某 chunk 已解码并进入缓存。
    fn ensure_chunk(&mut self, index: u32) -> CoreResult<()> {
        if self.cache.contains_key(&index) {
            return Ok(());
        }
        // chunk_data_dimensions 直接返回元组（非 Result）
        let (dw, dh) = self.dec.chunk_data_dimensions(index);
        let res = self.dec.read_chunk(index)?;
        let raw8 =
            meta::result_to_u8(&res).ok_or_else(|| CoreError::Unsupported("64 位样本".into()))?;
        let rgba = convert_to_rgba(&raw8, self.colortype, dw as usize, dh as usize)?;

        let rgba_len = rgba.len();

        // LRU 驱逐（保留至少一个）
        while self.cache_bytes + rgba_len > CACHE_BUDGET_BYTES && self.order.len() > 1 {
            if let Some(victim) = self.order.pop_front() {
                if let Some(c) = self.cache.remove(&victim) {
                    self.cache_bytes -= c.rgba.len();
                }
                self.dims.remove(&victim);
            }
        }
        self.cache.insert(index, ChunkCanvas { rgba });
        self.dims.insert(index, (dw, dh));
        self.cache_bytes += rgba_len;
        self.order.push_back(index);
        Ok(())
    }

    /// 预览计划 + 含采样点的 chunk 索引集合。
    pub fn preview_plan(&self, max_px: u32) -> ((u32, u32, u32), Vec<u32>) {
        let max_px = max_px.clamp(96, 1600);
        let (w, h) = (self.width, self.height);
        let scale = ((w.max(h) as f64 / max_px as f64).ceil() as u32).max(1);
        let ow = w.div_ceil(scale);
        let oh = h.div_ceil(scale);
        let mut needed = Vec::new();
        for ci in 0..self.chunk_count {
            let (cox, coy) = self.chunk_origin(ci);
            if cox >= w || coy >= h {
                continue;
            }
            let ox_lo = cox.div_ceil(scale);
            let oy_lo = coy.div_ceil(scale);
            if ox_lo >= ow || oy_lo >= oh {
                continue;
            }
            let cw_eff = self.chunk_w.min(w.saturating_sub(cox)).max(1);
            let ch_eff = self.chunk_h.min(h.saturating_sub(coy)).max(1);
            let ox_end = (((cox + cw_eff - 1) / scale) + 1).min(ow);
            let oy_end = (((coy + ch_eff - 1) / scale) + 1).min(oh);
            if ox_lo < ox_end && oy_lo < oy_end {
                needed.push(ci);
            }
        }
        ((ow, oh, scale), needed)
    }

    fn chunk_origin(&self, ci: u32) -> (u32, u32) {
        match self.chunk_type {
            ChunkType::Strip => (0u32, ci * self.chunk_h),
            ChunkType::Tile => (
                (ci % self.chunks_across) * self.chunk_w,
                (ci / self.chunks_across) * self.chunk_h,
            ),
        }
    }

    /// 读取源矩形区域为 RGBA8 行主序缓冲（越界区域填 0）。
    pub fn read_rect(&mut self, rx: i64, ry: i64, rw: u32, rh: u32) -> CoreResult<Vec<u8>> {
        if rw == 0 || rh == 0 {
            return Ok(Vec::new());
        }
        let mut out = vec![0u8; rw as usize * rh as usize * 4];
        if rx >= self.width as i64 || ry >= self.height as i64 || rx + rw as i64 <= 0 || ry + rh as i64 <= 0 {
            return Ok(out); // 完全在图外
        }

        let x0 = rx.clamp(0, self.width as i64 - 1);
        let y0 = ry.clamp(0, self.height as i64 - 1);
        let x1 = (rx + rw as i64 - 1).min(self.width as i64 - 1);
        let y1 = (ry + rh as i64 - 1).min(self.height as i64 - 1);

        let first_cx = x0 as u32 / self.chunk_w;
        let last_cx = x1 as u32 / self.chunk_w;
        let first_cy = y0 as u32 / self.chunk_h;
        let last_cy = y1 as u32 / self.chunk_h;

        for cy in first_cy..=last_cy {
            for cx in first_cx..=last_cx {
                let idx = match self.chunk_type {
                    ChunkType::Strip => cy,
                    _ => cy * self.chunks_across + cx,
                };
                if idx >= self.chunk_count {
                    continue;
                }
                self.ensure_chunk(idx)?;

                // 先取完所有 self 字段，再短暂借用缓存画布做拷贝
                let ox = cx * self.chunk_w;
                let oy = cy * self.chunk_h;
                let (dw, dh) = self.dims[&idx];
                let cw_eff = dw.min(self.width.saturating_sub(ox)).max(1);
                let ch_eff = dh.min(self.height.saturating_sub(oy)).max(1);

                let sx = ox.max(x0 as u32);
                let sy = oy.max(y0 as u32);
                let ex = (ox + cw_eff).min(x1 as u32 + 1);
                let ey = (oy + ch_eff).min(y1 as u32 + 1);
                if sx >= ex || sy >= ey {
                    continue;
                }

                let canvas = self.cache.get(&idx).expect("just ensured");
                for row in sy..ey {
                    let src_off =
                        ((row - oy) as usize * cw_eff as usize + (sx - ox) as usize) * 4;
                    let dst_row = (row as i64 - ry) as usize;
                    let dst_col = (sx as i64 - rx) as usize;
                    let dst_off = (dst_row * rw as usize + dst_col) * 4;
                    let span = (ex - sx) as usize * 4;
                    out[dst_off..dst_off + span]
                        .copy_from_slice(&canvas.rgba[src_off..src_off + span]);
                }
            }
        }
        Ok(out)
    }
}

/// 几何元数据快照（供并行线程使用，不依赖 SourceReader）。
#[derive(Clone, Copy)]
struct Geo {
    w: u32,
    h: u32,
    chunk_type: ChunkType,
    chunk_w: u32,
    chunk_h: u32,
    chunks_across: u32,
    scale: u32,
    ow: u32,
    oh: u32,
}

impl Geo {
    fn origin(&self, ci: u32) -> (u32, u32) {
        match self.chunk_type {
            ChunkType::Strip => (0u32, ci * self.chunk_h),
            ChunkType::Tile => (
                (ci % self.chunks_across) * self.chunk_w,
                (ci / self.chunks_across) * self.chunk_h,
            ),
        }
    }
}

thread_local! {
    /// 线程本地解码器（仅路径复用；Geo 属调用级参数，不缓存——同一文件不同分辨率
    /// 调用若沿用旧 Geo 会导致采样错位，见 preview_sample_parallel 使用处）。
    static PREVIEW_DECODER: std::cell::RefCell<Option<(PathBuf, ColorType, Decoder<File>)>> =
        const { std::cell::RefCell::new(None) };
}

/// 进程级预览解码块缓存：跨 make_preview 调用共享（粗图/精图/重复预览只解码一次）。
/// LRU/FIFO 淘汰 + 字节预算封顶，避免长会话内存无限增长。
const PREVIEW_CHUNK_BUDGET: u64 = 4 * 1024 * 1024 * 1024; // 4GB

#[derive(Clone, PartialEq)]
struct PreviewChunkKey {
    path: PathBuf,
    ci: u32,
}

struct PreviewChunkCache {
    /// FIFO：头部最旧，尾部最新；记录块的实际数据尺寸（边缘块小于标称值）
    entries: Vec<(PreviewChunkKey, std::sync::Arc<Vec<u8>>, (u32, u32))>,
    bytes: u64,
}

static PREVIEW_CHUNK_CACHE: std::sync::OnceLock<std::sync::Mutex<PreviewChunkCache>> =
    std::sync::OnceLock::new();

fn preview_chunk_cache() -> &'static std::sync::Mutex<PreviewChunkCache> {
    PREVIEW_CHUNK_CACHE.get_or_init(|| {
        std::sync::Mutex::new(PreviewChunkCache { entries: Vec::new(), bytes: 0 })
    })
}

/// 取（或解码并缓存）某文件第 `ci` 个 chunk 的 RGBA 数据及其实际尺寸。
/// 命中缓存时复用，避免每次 make_preview 都整文件重解（当前 320 粗图 + 1400 精图 = 两次全量解码）。
fn preview_chunk_rgba(
    path: &Path,
    d: &mut Decoder<File>,
    ct: ColorType,
    ci: u32,
) -> CoreResult<(std::sync::Arc<Vec<u8>>, u32, u32)> {
    let key = PreviewChunkKey { path: path.to_path_buf(), ci };
    {
        let cache = preview_chunk_cache().lock().unwrap();
        if let Some(entry) = cache.entries.iter().find(|e| e.0 == key) {
            return Ok((std::sync::Arc::clone(&entry.1), entry.2 .0, entry.2 .1));
        }
    }
    let (dw, dh) = d.chunk_data_dimensions(ci);
    let res = d.read_chunk(ci)?;
    let raw8 =
        crate::engine::meta::result_to_u8(&res).ok_or_else(|| CoreError::Unsupported("64 位样本".into()))?;
    let rgba = convert_to_rgba(&raw8, ct, dw as usize, dh as usize)?;
    let arc = std::sync::Arc::new(rgba);
    {
        let mut cache = preview_chunk_cache().lock().unwrap();
        // 已有同名条目（并发写入）：直接复用已有缓存，避免重复占用
        if let Some(entry) = cache.entries.iter().find(|e| e.0 == key) {
            return Ok((std::sync::Arc::clone(&entry.1), entry.2 .0, entry.2 .1));
        }
        cache.bytes += arc.len() as u64;
        cache.entries.push((key, std::sync::Arc::clone(&arc), (dw, dh)));
        // 预算淘汰：从最旧（头部）开始移除，至少保留一条
        while cache.bytes > PREVIEW_CHUNK_BUDGET && cache.entries.len() > 1 {
            let oldest = cache.entries.remove(0);
            cache.bytes = cache.bytes.saturating_sub(oldest.1.len() as u64);
        }
    }
    Ok((arc, dw, dh))
}

/// 并行预览采样：多线程各持独立解码器，解码块经进程级缓存复用。
/// 首次调用完成整图解码（chunk 高度 > 采样步长时所有块都含采样点，无法跳过），
/// 后续调用（精图/重复预览）直接命中缓存，只做采样 → 粗图+精图合计 ≈ 一次解码。
pub fn preview_sample_parallel(path: &Path, max_px: u32) -> CoreResult<(u32, u32, Vec<u8>)> {
    let probe = SourceReader::open(path)?;
    let geo = Geo {
        w: probe.width,
        h: probe.height,
        chunk_type: probe.chunk_type,
        chunk_w: probe.chunk_w,
        chunk_h: probe.chunk_h,
        chunks_across: probe.chunks_across,
        scale: 0,
        ow: 0,
        oh: 0,
    };
    let ((ow, oh, scale), needed) = probe.preview_plan(max_px);
    drop(probe);

    let geo = Geo { scale, ow, oh, ..geo };

    let mut canvas = vec![0u8; ow as usize * oh as usize * 4];
    let path_buf = path.to_path_buf();

    // 每个块独立取（或解码）RGBA 并产出 (目标偏移, 像素) 列表，最后统一写入（区域互不相交）
    let results: Vec<Vec<(usize, [u8; 4])>> = {
        use rayon::prelude::*;
        needed
            .into_par_iter()
            .map(|ci| {
                PREVIEW_DECODER.with(|cell| -> CoreResult<Vec<(usize, [u8; 4])>> {
                    let mut slot = cell.borrow_mut();
                    let need_reopen = match slot.as_ref() {
                        Some((p, _, _)) => p != &path_buf,
                        None => true,
                    };
                    if need_reopen {
                        let file =
                            File::open(path).map_err(|e| io_err(path.display().to_string(), e))?;
                        let mut d = Decoder::new(file)?;
                        let ct = d.colortype()?;
                        *slot = Some((path_buf.clone(), ct, d));
                    }
                    // Geo 是本次调用的参数（scale/ow/oh），一律用闭包捕获的当前值，
                    // 绝不使用线程本地中旧调用的 Geo，避免分辨率变化后采样错位（花屏）。
                    let (_, ct, d) = slot.as_mut().expect("decoder prepared");
                    let (rgba, dw, dh) = preview_chunk_rgba(&path_buf, d, *ct, ci)?;
                    Ok(cells_from_rgba(&rgba, dw, dh, &geo, ci))
                })
                .unwrap_or_default()
            })
            .collect()
    };

    // 写入画布：偏移互不相交，顺序无关
    for cells in results {
        for (dst, px) in cells {
            let e = dst + 4;
            if e <= canvas.len() {
                canvas[dst..e].copy_from_slice(&px);
            }
        }
    }
    Ok((ow, oh, canvas))
}

/// 从已解码的 chunk RGBA 提取预览网格采样点 (canvas 偏移, RGBA)。
fn cells_from_rgba(
    rgba: &[u8],
    dw: u32,
    dh: u32,
    g: &Geo,
    ci: u32,
) -> Vec<(usize, [u8; 4])> {
    let (cox, coy) = g.origin(ci);
    if cox >= g.w || coy >= g.h {
        return vec![];
    }
    let cw_eff = dw.min(g.w - cox).max(1);
    let ch_eff = dh.min(g.h - coy).max(1);

    let ox_lo = cox.div_ceil(g.scale);
    let oy_lo = coy.div_ceil(g.scale);
    let ox_end = (((cox + cw_eff - 1) / g.scale) + 1).min(g.ow);
    let oy_end = (((coy + ch_eff - 1) / g.scale) + 1).min(g.oh);

    let mut out = Vec::new();
    for oy in oy_lo..oy_end {
        let sy = oy * g.scale;
        if sy < coy || sy >= coy + ch_eff {
            continue;
        }
        let lrow = (sy - coy) as usize;
        for ox in ox_lo..ox_end {
            let sx = ox * g.scale;
            if sx >= cox + cw_eff {
                break;
            }
            let si = (lrow * cw_eff as usize + (sx - cox) as usize) * 4;
            if si + 3 >= rgba.len() {
                continue;
            }
            let di = (oy as usize * g.ow as usize + ox as usize) * 4;
            out.push((di, [rgba[si], rgba[si + 1], rgba[si + 2], rgba[si + 3]]));
        }
    }
    out
}

/// 将单 chunk 的原始 u8 数据转为 RGBA8。
fn convert_to_rgba(raw: &[u8], ct: ColorType, w: usize, h: usize) -> CoreResult<Vec<u8>> {
    let n = w * h;
    let px = |i: usize| raw.get(i).copied().unwrap_or(0);
    let mut out = Vec::with_capacity(n * 4);

    match ct {
        ColorType::RGBA(8) => {
            if raw.len() >= n * 4 {
                out.extend_from_slice(&raw[..n * 4]);
            } else {
                return Err(CoreError::Unsupported("RGBA8 chunk 数据不足".into()));
            }
        }
        ColorType::RGB(8) => {
            for i in 0..n {
                out.extend_from_slice(&[px(i * 3), px(i * 3 + 1), px(i * 3 + 2), 255]);
            }
        }
        ColorType::GrayA(8) => {
            for i in 0..n {
                let g = px(i * 2);
                out.extend_from_slice(&[g, g, g, px(i * 2 + 1)]);
            }
        }
        ColorType::Gray(_) => {
            for i in 0..n {
                let g = px(i);
                out.extend_from_slice(&[g, g, g, 255]);
            }
        }
        ColorType::CMYK(8) => {
            for i in 0..n {
                out.extend_from_slice(&[
                    255 - px(i * 4),
                    255 - px(i * 4 + 1),
                    255 - px(i * 4 + 2),
                    255,
                ]);
            }
        }
        // 16 位变体：result_to_u8 已把 U16 缩到 u8，布局不变
        ColorType::RGB(16) => {
            for i in 0..n {
                out.extend_from_slice(&[px(i * 3), px(i * 3 + 1), px(i * 3 + 2), 255]);
            }
        }
        ColorType::RGBA(16) => {
            for i in 0..n {
                out.extend_from_slice(raw.get(i * 4..i * 4 + 4).unwrap_or(&[0, 0, 0, 255]));
            }
        }
        ColorType::GrayA(16) => {
            for i in 0..n {
                let g = px(i * 2);
                out.extend_from_slice(&[g, g, g, px(i * 2 + 1)]);
            }
        }
        other => {
            return Err(CoreError::Unsupported(format!(
                "像素格式组合 {other:?} 暂不支持"
            )))
        }
    }
    Ok(out)
}
