//! TIFF 元数据探测：只读文件头与标签，不解码像素。

use std::fs::File;
use std::path::Path;

use ::tiff::decoder::{Decoder, DecodingResult};
use ::tiff::tags::Tag;
use ::tiff::ColorType;

use super::error::{io_err, CoreResult};

#[derive(Debug, Clone, serde::Serialize)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    /// 每样本位数（多值时为各通道）
    pub bits_per_sample: Vec<u16>,
    pub samples: u32,
    pub has_alpha: bool,
    pub alpha_premultiplied: bool,
    /// 像素格式描述，如 "RGB8" / "RGBA16" / "Gray8"
    pub pixel_format: String,
    /// 压缩方式名称
    pub compression: String,
    /// 分块类型: strip | tile
    pub chunk_type: String,
    /// strip 高度或 tile 宽度（元信息展示用）
    pub chunk_hint: (u32, u32),
    /// 估算完全展开 RGBA8 的字节数
    pub rgba_bytes: u64,
}

impl ImageInfo {
    pub fn megapixels(&self) -> f64 {
        self.width as f64 * self.height as f64 / 1e6
    }
}

/// GeoTIFF 地理参考（PixelScale + Tiepoint，无旋转）。
#[derive(Debug, Clone, Copy)]
pub struct GeoRef {
    /// 左上角 X（经度或米）
    pub mx0: f64,
    /// 左上角 Y（纬度或米）
    pub my_top: f64,
    pub sx: f64,
    pub sy: f64,
    /// true = 单位为度（EPSG:4326），false = 米（EPSG:3857）
    pub degrees: bool,
}

impl GeoRef {
    /// 返回 EPSG:3857 bounds [minx, miny, maxx, maxy]
    pub fn bounds3857(&self, w: u32, h: u32) -> [f64; 4] {
        let (x0, y0) = (self.mx0, self.my_top);
        let x1 = x0 + w as f64 * self.sx;
        let y1 = y0 + h as f64 * self.sy; // 南边界（sy 为负）
        if self.degrees {
            let r = 6378137.0;
            let to_m_x = |lon: f64| lon.to_radians() * r;
            let to_m_y = |lat: f64| {
                let clamped = lat.clamp(-85.05112878, 85.05112878);
                (clamped.to_radians() * 0.5).tan().ln() * r
            };
            [
                to_m_x(x0),
                to_m_y(y1.min(y0)),
                to_m_x(x1),
                to_m_y(y1.max(y0)),
            ]
        } else {
            [x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)]
        }
    }
}

/// 提取地理参考；无 PixelScale/Tiepoint 时返回 None。
pub fn probe_georef(path: &Path) -> CoreResult<Option<GeoRef>> {
    let file = File::open(path).map_err(|e| io_err(path.display().to_string(), e))?;
    let mut dec = Decoder::new(file)?;
    let scale = dec
        .get_tag_f64_vec(Tag::Unknown(33550)) // ModelPixelScale
        .unwrap_or_default();
    let tie = dec
        .get_tag_f64_vec(Tag::Unknown(33922)) // ModelTiepoint
        .unwrap_or_default();
    if scale.len() < 2 || tie.len() < 6 || scale[0] <= 0.0 || scale[1] == 0.0 {
        return Ok(None);
    }
    let (mx0, my_top) = (tie[3], tie[4]);
    // 度 vs 米：按数量级判断（经度 ≤360 且像素尺寸远小于 1）
    let degrees = mx0.abs() <= 360.0 && my_top.abs() <= 90.0 && scale[0] < 1.0;
    Ok(Some(GeoRef {
        mx0,
        my_top,
        sx: scale[0],
        sy: scale[1],
        degrees,
    }))
}

fn compression_name(code: u16) -> String {
    match code {
        1 => "无压缩".into(),
        5 => "LZW".into(),
        7 => "JPEG".into(),
        8 | 32946 => "Deflate".into(),
        9 => "TIFF 6.0 FBW".into(),
        10 => "TIFF 6.0 FBW+".into(),
        32773 => "PackBits".into(),
        34712 => "JPEG2000".into(),
        34925 => "LZMA".into(),
        50000 => "ZSTD".into(),
        50001 => "WebP".into(),
        other => format!("未知({other})"),
    }
}

/// 打开并解析 TIFF 元数据。
pub fn probe(path: &Path) -> CoreResult<ImageInfo> {
    let file = File::open(path).map_err(|e| io_err(path.display().to_string(), e))?;
    let mut dec = Decoder::new(file)?;

    let (width, height) = dec.dimensions()?;
    let colortype = dec.colortype()?;

    let bits = dec
        .get_tag_u32_vec(Tag::BitsPerSample)
        .unwrap_or_default()
        .into_iter()
        .map(|v| v as u16)
        .collect::<Vec<_>>();
    let samples = dec.get_tag_u32(Tag::SamplesPerPixel).unwrap_or(1);
    let compression = dec.get_tag_u32(Tag::Compression).unwrap_or(1);
    let extra = dec.get_tag_u32_vec(Tag::ExtraSamples).unwrap_or_default();
    // ExtraSamples: 0=unspecified 1=associated(pre-multiplied) 2=unassociated
    let alpha_premultiplied = extra.first() == Some(&1);

    let (pixel_format, has_alpha) = match colortype {
        ColorType::Gray(b) => (format!("Gray{b}"), false),
        ColorType::GrayA(b) => (format!("GrayA{b}"), true),
        ColorType::RGB(b) => (format!("RGB{b}"), extra.len() > 0),
        ColorType::RGBA(b) => (format!("RGBA{b}"), true),
        ColorType::Palette(b) => (format!("Palette{b}"), extra.len() > 0),
        ColorType::CMYK(b) => (format!("CMYK{b}"), false),
        other => (
            format!("{other:?}"),
            false,
        ),
    };

    let chunk_type = match dec.get_chunk_type() {
        ::tiff::decoder::ChunkType::Strip => "strip",
        ::tiff::decoder::ChunkType::Tile => "tile",
    };
    // chunk_dimensions 直接返回元组
    let chunk_dims = dec.chunk_dimensions();
    let chunk_hint = if chunk_type == "strip" {
        let rows = dec.get_tag_u32(Tag::RowsPerStrip).unwrap_or(height);
        (width, rows.min(height))
    } else {
        chunk_dims
    };

    Ok(ImageInfo {
        width,
        height,
        bits_per_sample: if bits.is_empty() { vec![8] } else { bits },
        samples,
        has_alpha,
        alpha_premultiplied,
        pixel_format,
        compression: compression_name(compression as u16),
        chunk_type: chunk_type.into(),
        chunk_hint,
        rgba_bytes: width as u64 * height as u64 * 4,
    })
}

/// 将 `DecodingResult` 统一降采样到 u8 向量（按位深缩放）。
pub(crate) fn result_to_u8(res: &DecodingResult) -> Option<Vec<u8>> {
    match res {
        DecodingResult::U8(v) => Some(v.clone()),
        DecodingResult::I8(v) => Some(v.iter().map(|&x| x as u8).collect()),
        DecodingResult::U16(v) => Some(v.iter().map(|&x| (x >> 8) as u8).collect()),
        DecodingResult::I16(v) => Some(v.iter().map(|&x| (x >> 8) as u8).collect()),
        DecodingResult::U32(v) => Some(v.iter().map(|&x| (x >> 24) as u8).collect()),
        DecodingResult::I32(v) => Some(v.iter().map(|&x| (x >> 24) as u8).collect()),
        DecodingResult::F32(v) => Some(
            v.iter()
                .map(|&x| (x.clamp(0.0, 1.0) * 255.0).round() as u8)
                .collect(),
        ),
        DecodingResult::F64(v) => Some(
            v.iter()
                .map(|&x| (x.clamp(0.0, 1.0) * 255.0).round() as u8)
                .collect(),
        ),
        // F16 / U64：罕见格式，明确不支持
        _ => None,
    }
}
