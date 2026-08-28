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
/// 所有坐标均已统一到 EPSG:3857（米），sx 为正、sy 为负（北向上）。
#[derive(Debug, Clone, Copy)]
pub struct GeoRef {
    /// 左上角 X（EPSG:3857 米）
    pub mx0: f64,
    /// 左上角 Y（EPSG:3857 米，北界，较大值）
    pub my_top: f64,
    /// 像素宽度（EPSG:3857 米，正值）
    pub sx: f64,
    /// 像素高度（EPSG:3857 米，负值——北向上时行号↓ = Y↓）
    pub sy: f64,
}

impl GeoRef {
    /// 返回 EPSG:3857 bounds [minx, miny, maxx, maxy]
    pub fn bounds3857(&self, w: u32, h: u32) -> [f64; 4] {
        let x0 = self.mx0;
        let x1 = x0 + w as f64 * self.sx;
        // sy 为负：y1 = my_top + h * sy = my_top - h*|sy| = 南界
        let y_south = self.my_top + h as f64 * self.sy;
        [x0.min(x1), y_south, x0.max(x1), self.my_top]
    }
}

// ───────── CRS 检测与坐标转换 ─────────

const GT_MODEL_TYPE_GEOKEY: u16 = 1024;
const PROJECTED_CS_TYPE_GEOKEY: u16 = 3072;
const GEOGRAPHIC_TYPE_GEOKEY: u16 = 2049;

const MODEL_TYPE_PROJECTED: u32 = 1;
const MODEL_TYPE_GEOGRAPHIC: u32 = 2;

/// 从 GeoKeyDirectory (tag 34735) 解析 EPSG 代码。
fn parse_georef_epsg(keys: &[u32]) -> Option<u32> {
    if keys.len() < 4 {
        return None;
    }
    let n_keys = keys[3] as usize;
    let mut model_type = 0u32;
    let mut projected_epsg = 0u32;
    let mut geographic_epsg = 0u32;

    for i in 0..n_keys {
        let base = 4 + i * 4;
        if base + 4 > keys.len() {
            break;
        }
        let key_id = keys[base] as u16;
        // tiff_tag_location == 0 表示值直接嵌在 value_offset 里
        let _tag_loc = keys[base + 1] as u16;
        let _count = keys[base + 2];
        let value = keys[base + 3];

        match key_id {
            GT_MODEL_TYPE_GEOKEY => model_type = value,
            PROJECTED_CS_TYPE_GEOKEY => projected_epsg = value,
            GEOGRAPHIC_TYPE_GEOKEY => geographic_epsg = value,
            _ => {}
        }
    }

    if model_type == MODEL_TYPE_PROJECTED && projected_epsg > 0 && projected_epsg < 32767 {
        Some(projected_epsg)
    } else if model_type == MODEL_TYPE_GEOGRAPHIC && geographic_epsg > 0 && geographic_epsg < 32767 {
        Some(geographic_epsg)
    } else {
        None
    }
}

/// EPSG:3857 常量
const ORIGIN_SHIFT: f64 = 20_037_508.342_789_244;
const EARTH_RADIUS: f64 = 6_378_137.0;

/// 地理坐标（度）→ EPSG:3857（米）
fn lonlat_to_3857(lon: f64, lat: f64) -> (f64, f64) {
    let x = lon * ORIGIN_SHIFT / 180.0;
    let y = ((90.0 + lat) * std::f64::consts::PI / 360.0).tan().ln() * ORIGIN_SHIFT / std::f64::consts::PI;
    (x, y)
}

/// EPSG:3857（米）→ 地理坐标（度）
fn _3857_to_lonlat(mx: f64, my: f64) -> (f64, f64) {
    let lon = mx / ORIGIN_SHIFT * 180.0;
    let lat = my / ORIGIN_SHIFT * 180.0;
    let lat = 180.0 / std::f64::consts::PI
        * (2.0 * (lat * std::f64::consts::PI / 180.0).exp().atan() - std::f64::consts::FRAC_PI_2);
    (lon, lat)
}

/// UTM → 地理坐标（lon/lat 度）
///
/// 实现 USGS Convert ECQ 内置的 UTM 正算反算。
/// `zone`: 1–60, `is_north`: true = 北半球
fn utm_to_lonlat(easting: f64, northing: f64, zone: u32, is_north: bool) -> (f64, f64) {
    // WGS84 椭球
    let a = 6_378_137.0_f64;
    let f = 1.0 / 298.257_223_563;
    let k0 = 0.9996;
    let e = f * (2.0_f64 - f).sqrt();
    let e2 = e * e;
    let ep2 = e2 / (1.0 - e2);

    let cm = (zone as f64 - 1.0) * 6.0 - 180.0 + 3.0; // 中央经线（度）
    let x = easting - 500_000.0; // 去掉假东
    let y = if is_north { northing } else { northing - 10_000_000.0 };

    let m = y / k0;
    let mu = m / (a * (1.0 - e2 / 4.0 - 3.0 * e2 * e2 / 64.0 - 5.0 * e2 * e2 * e2 / 256.0));

    let phi1 = mu
        + (3.0 * e / 2.0 - 27.0 * e * e * e / 32.0) * (2.0 * mu).sin()
        + (21.0 * e * e / 16.0 - 55.0 * e * e * e * e / 32.0) * (4.0 * mu).sin()
        + (151.0 * e * e * e / 96.0) * (6.0 * mu).sin()
        + (1097.0 * e * e * e * e / 512.0) * (8.0 * mu).sin();

    let n1 = a / (1.0 - e2 * phi1.sin() * phi1.sin()).sqrt();
    let t1 = phi1.tan();
    let t1sq = t1 * t1;
    let c1 = ep2 * phi1.cos() * phi1.cos();
    let r1 = a * (1.0 - e2) / (1.0 - e2 * phi1.sin() * phi1.sin()).powf(1.5);
    let d = x / (n1 * k0);

    let lat_rad = phi1
        - (n1 * t1 / r1)
            * (d * d / 2.0
                - (5.0 + 3.0 * t1sq + 10.0 * c1 - 4.0 * c1 * c1 - 9.0 * ep2) * d * d * d * d / 24.0
                + (61.0 + 90.0 * t1sq + 298.0 * c1 + 45.0 * t1sq * t1sq - 252.0 * ep2 - 3.0 * c1 * c1)
                    * d * d * d * d * d * d
                    / 720.0);

    let lon_rad = cm.to_radians()
        + (d
            - (1.0 + 2.0 * t1sq + c1) * d * d * d / 6.0
            + (5.0 - 2.0 * c1 + 28.0 * t1sq - 3.0 * c1 * c1 + 8.0 * ep2 + 24.0 * t1sq * t1sq)
                * d * d * d * d * d
                / 120.0)
            / phi1.cos();

    (lon_rad.to_degrees(), lat_rad.to_degrees())
}

/// 从 EPSG 代码判断是否为 UTM 投影，返回 (zone, is_north)。
fn epsg_to_utm(epsg: u32) -> Option<(u32, bool)> {
    match epsg {
        // 北半球 UTM: 32601–32660
        32601..=32660 => Some((epsg - 32600, true)),
        // 南半球 UTM: 32701–32760
        32701..=32760 => Some((epsg - 32700, false)),
        _ => None,
    }
}

/// 提取地理参考；无 PixelScale/Tiepoint 时返回 None。
///
/// 坐标统一转为 EPSG:3857：
/// - 已是 3857 → 直接使用
/// - 4326 (lon/lat) → Mercator 正算
/// - UTM 等投影 → 先反算到 lon/lat，再正算到 3857
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

    let raw_x = tie[3];
    let raw_y = tie[4];
    let px = scale[0]; // 像素宽（正）
    let py = scale[1]; // 像素高（正，需要转为负）

    // 读取 GeoKeyDirectory 确定 CRS（tag 34735 通常存储为 SHORT/u16）
    let geocode_epsg = dec
        .get_tag_u16_vec(Tag::Unknown(34735))
        .ok()
        .map(|v| v.into_iter().map(|x| x as u32).collect::<Vec<_>>())
        .or_else(|| {
            dec.get_tag_u32_vec(Tag::Unknown(34735)).ok()
        })
        .and_then(|v| parse_georef_epsg(&v));

    // 读取影像尺寸（用于度→米转换时计算每像素米数）
    let (src_w, src_h) = dec.dimensions().unwrap_or((1, 1));

    // 判断原始坐标类型——优先以 GeoKey EPSG 为准
    let is_3857 = geocode_epsg == Some(3857);
    let is_4326 = geocode_epsg == Some(4326);

    // 无 GeoKey 时回退到数量级启发式
    let heuristic_3857 = geocode_epsg.is_none()
        && raw_x.abs() <= ORIGIN_SHIFT && raw_y.abs() <= ORIGIN_SHIFT
        && raw_x.abs() > 360.0;
    let heuristic_4326 = geocode_epsg.is_none()
        && raw_x.abs() <= 360.0 && raw_y.abs() <= 90.0 && px < 1.0;

    if is_3857 || heuristic_3857 {
        // 已经是 EPSG:3857：sy 取负（北向上，行↓ = Y↓）
        return Ok(Some(GeoRef {
            mx0: raw_x,
            my_top: raw_y,
            sx: px,
            sy: -py,
        }));
    }

    if is_4326 || heuristic_4326 {
        // EPSG:4326（度）→ 3857（米）
        let lon0 = raw_x;
        let lat_top = raw_y;
        let lon1 = raw_x + src_w as f64 * px;
        let lat_bot = raw_y - src_h as f64 * py;
        let (m0, my_n) = lonlat_to_3857(lon0, lat_top);
        let (m1, _) = lonlat_to_3857(lon1, lat_top);
        let (_, my_s) = lonlat_to_3857(lon0, lat_bot);
        return Ok(Some(GeoRef {
            mx0: m0,
            my_top: my_n,
            sx: (m1 - m0) / src_w as f64,
            sy: (my_s - my_n) / src_h as f64, // 负值
        }));
    }

    if let Some(epsg) = geocode_epsg {
        // 其他投影 → 先反算到 lon/lat，再正算到 3857
        if let Some((zone, is_north)) = epsg_to_utm(epsg) {
            // UTM 反算：四个角点
            // 注意：像素 (0,0) = 左上（北），像素 (0,h) = 左下（南）
            // UTM Y 增大方向 = 北，所以南界 = raw_y - src_h * py
            let (lon0, lat_top) = utm_to_lonlat(raw_x, raw_y, zone, is_north);
            let (lon1, _) = utm_to_lonlat(raw_x + src_w as f64 * px, raw_y, zone, is_north);
            let (_, lat_bot) = utm_to_lonlat(raw_x, raw_y - src_h as f64 * py, zone, is_north);
            let (m0, my_n) = lonlat_to_3857(lon0, lat_top);
            let (m1, _) = lonlat_to_3857(lon1, lat_top);
            let (_, my_s) = lonlat_to_3857(lon0, lat_bot);
            return Ok(Some(GeoRef {
                mx0: m0,
                my_top: my_n,
                sx: (m1 - m0) / src_w as f64,
                sy: (my_s - my_n) / src_h as f64, // 负值
            }));
        }
        // 未知投影——回退为假设3857
        return Ok(Some(GeoRef {
            mx0: raw_x,
            my_top: raw_y,
            sx: px,
            sy: -py,
        }));
    }

    // 无 CRS 信息——按数量级猜测
    if raw_x.abs() > 360.0 && raw_y.abs() > 90.0 {
        // 疑似投影坐标（米），假设3857
        return Ok(Some(GeoRef {
            mx0: raw_x,
            my_top: raw_y,
            sx: px,
            sy: -py,
        }));
    }

    // 疑似地理坐标（度）→ 3857
    let lon0 = raw_x;
    let lat_top = raw_y;
    let lon1 = raw_x + src_w as f64 * px;
    let lat_bot = raw_y - src_h as f64 * py;
    let (m0, my_n) = lonlat_to_3857(lon0, lat_top);
    let (m1, _) = lonlat_to_3857(lon1, lat_top);
    let (_, my_s) = lonlat_to_3857(lon0, lat_bot);
    Ok(Some(GeoRef {
        mx0: m0,
        my_top: my_n,
        sx: (m1 - m0) / src_w as f64,
        sy: (my_s - my_n) / src_h as f64,
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
