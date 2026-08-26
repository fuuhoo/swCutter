//! 分块读取的 TIFF 源读取器：按需解码 chunk + 小容量 LRU 缓存，
//! 对外提供任意源矩形 → RGBA8 的 `read_rect`，内存占用有界。

use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::path::{Path, PathBuf};

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

    /// 单遍采样生成预览：每个 chunk 仅解码一次，把采样网格点拷入画布。
    /// 返回 (ow, oh, rgba8)。对任意条带/瓦片布局都是 O(文件大小) 单遍。
    pub fn preview_sample(&mut self, max_px: u32) -> CoreResult<(u32, u32, Vec<u8>)> {
        let max_px = max_px.clamp(128, 2048);
        let (w, h) = (self.width, self.height);
        let scale = ((w.max(h) as f64 / max_px as f64).ceil() as u32).max(1);
        let ow = w.div_ceil(scale);
        let oh = h.div_ceil(scale);
        let mut canvas = vec![0u8; ow as usize * oh as usize * 4];

        for ci in 0..self.chunk_count {
            let (cox, coy) = match self.chunk_type {
                ChunkType::Strip => (0u32, ci * self.chunk_h),
                ChunkType::Tile => (
                    (ci % self.chunks_across) * self.chunk_w,
                    (ci / self.chunks_across) * self.chunk_h,
                ),
            };
            if cox >= w || coy >= h || ci >= self.chunk_count {
                continue;
            }
            // 跳过不含采样点的块
            let ox0 = cox.div_ceil(scale);
            let oy0 = coy.div_ceil(scale);
            if ox0 >= ow || oy0 >= oh {
                continue;
            }
            self.ensure_chunk(ci)?;
            let (dw, dh) = self.dims[&ci];
            let cw_eff = dw.min(w - cox).max(1);
            let ch_eff = dh.min(h - coy).max(1);

            let ox1 = (((cox + cw_eff - 1) / scale) + 1).min(ow) ; // exclusive
            let oy1 = (((coy + ch_eff - 1) / scale) + 1).min(oh);

            let canvas_c = self.cache.get(&ci).expect("just ensured");
            for oy in oy0..oy1 {
                let sy = oy * scale;
                if sy < coy || sy >= coy + ch_eff { continue; }
                let lrow = (sy - coy) as usize;
                for ox in ox0..ox1 {
                    let sx = ox * scale;
                    if sx < cox || sx >= cox + cw_eff { continue; }
                    let si = (lrow * cw_eff as usize + (sx - cox) as usize) * 4;
                    let di = (oy as usize * ow as usize + ox as usize) * 4;
                    if si + 3 < canvas_c.rgba.len() && di + 3 < canvas.len() {
                        canvas[di..di + 4].copy_from_slice(&canvas_c.rgba[si..si + 4]);
                    }
                }
            }
        }
        Ok((ow, oh, canvas))
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
