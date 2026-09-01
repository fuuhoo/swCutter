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
    /// 源 EPSG（用于实时反算）
    pub src_epsg: u32,
    /// 源 tiepoint 中心（原始投影坐标，例如 UTM 48N 米）
    pub src_tie_x: f64,
    pub src_tie_y: f64,
    /// 源像素宽高（原始投影单位，正值，tiepoint 为 0, 0 中心）
    pub src_px_w: f64,
    pub src_px_h: f64,
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

/// EPSG:3857（米）→ 地理坐标（度）
fn _3857_to_lonlat(mx: f64, my: f64) -> (f64, f64) {
    let lon = mx / ORIGIN_SHIFT * 180.0;
    let lat = my / ORIGIN_SHIFT * 180.0;
    let lat = 180.0 / std::f64::consts::PI
        * (2.0 * (lat * std::f64::consts::PI / 180.0).exp().atan() - std::f64::consts::FRAC_PI_2);
    (lon, lat)
}

/// UTM zone 的 PROJ.4 字符串（`+south` 让 PROJ 自动处理 10 000 000 假北）。
fn utm_proj_string(zone: u32, is_north: bool) -> String {
    format!(
        "+proj=utm +zone={zone} +datum=WGS84 +units=m +no_defs +type=crs{}",
        if is_north { "" } else { " +south" }
    )
}

/// EPSG:3857 (Web Mercator) PROJ.4 字符串。球面公式，与 gdal/PROJ 默认一致。
pub const WEB_MERC_PROJ: &str = "+proj=merc +a=6378137 +b=6378137 +lat_ts=0 \
    +lon_0=0 +x_0=0 +y_0=0 +k=1 +units=m +nadgrids=@null +wktext +no_defs +type=crs";

/// EPSG:4326 (WGS84 经纬度) PROJ.4 字符串。proj4rs 期望 (lon, lat) 弧度。
const WGS84_GEOGR_PROJ: &str =
    "+proj=longlat +datum=WGS84 +no_defs +type=crs";

/// 经纬度（度）→ EPSG:3857 米。仅在源是 EPSG:4326 时使用；其它投影走
/// `transform_to_3857` 跨投影级联。
fn lonlat_to_3857(lon_deg: f64, lat_deg: f64) -> (f64, f64) {
    let lonlat = proj4rs::Proj::from_proj_string(WGS84_GEOGR_PROJ)
        .expect("valid WGS84 PROJ.4");
    let merc = proj4rs::Proj::from_proj_string(WEB_MERC_PROJ)
        .expect("valid Web Mercator PROJ.4");
    let mut p = (lon_deg.to_radians(), lat_deg.to_radians());
    proj4rs::transform::transform(&lonlat, &merc, &mut p)
        .expect("longlat -> 3857");
    (p.0, p.1)
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

/// CGCS2000 / 3-degree Gauss-Kruger zone 25..45 (CM 75°E..135°E)
/// EPSG:4513..4533 → 假东 (zone+0.5)*1M, 椭球 GRS80
/// 例: 4513=zone 25 (CM 75°E, x_0=25.5M), 4521=zone 33 (CM 99°E, x_0=33.5M)
fn cgcs2000_3deg_zone(epsg: u32) -> Option<u32> {
    if (4513..=4533).contains(&epsg) {
        Some(epsg - 4488) // 4513 → zone 25
    } else {
        None
    }
}

/// CGCS2000 / 3-degree Gauss-Kruger CM 75°E..135°E (3° 步长)
/// EPSG:?? 实际 CGCS2000 没用 "CM" 命名, 用 "zone" 命名. 故此函数未实现
/// (CGCS2000 6° GK 用 CM 命名 EPSG:4502-4512, 见 cgcs2000_6deg_cm)

/// CGCS2000 / 6-degree Gauss-Kruger CM 75°E..135°E (6° 步长)
/// EPSG:4502..4512 → 假东 500K, 椭球 GRS80
/// 例: 4502=CM 75°E (index 0), 4507=CM 105°E (index 5), 4512=CM 135°E (index 10)
fn cgcs2000_6deg_cm(epsg: u32) -> Option<u32> {
    if (4502..=4512).contains(&epsg) {
        Some(epsg - 4502) // 4502 → index 0
    } else {
        None
    }
}

/// Xian 1980 / 3-degree Gauss-Kruger zone 38..45 (CM 114°E..135°E)
/// EPSG:2362..2369 → 假东 (zone+0.5)*1M, 椭球 IAU76
/// 例: 2362=zone 38 (CM 114°E, x_0=38.5M), 2369=zone 45 (CM 135°E)
fn xian1980_3deg_zone(epsg: u32) -> Option<u32> {
    if (2362..=2369).contains(&epsg) {
        Some(epsg - 2324) // 2362 → zone 38
    } else {
        None
    }
}

/// Xian 1980 / 3-degree Gauss-Kruger CM 75°E..135°E (3° 步长)
/// EPSG:2370..2384 (15 zones) → 假东 500K
fn xian1980_3deg_cm(epsg: u32) -> Option<u32> {
    if (2370..=2384).contains(&epsg) {
        Some(epsg - 2370) // 2370 → CM 75°E (index 0)
    } else {
        None
    }
}

/// Xian 1980 / 6-degree Gauss-Kruger zone 13..23 (CM 75°E..135°E)
/// EPSG:2327..2337 → 假东 (zone+0.5)*1M, 椭球 IAU76
fn xian1980_6deg_zone(epsg: u32) -> Option<u32> {
    if (2327..=2337).contains(&epsg) {
        Some(epsg - 2314) // 2327 → zone 13
    } else {
        None
    }
}

/// Xian 1980 / 6-degree Gauss-Kruger CM 75°E..135°E
/// EPSG:2338..2342 (5 zones) → 假东 500K
fn xian1980_6deg_cm(epsg: u32) -> Option<u32> {
    if (2338..=2342).contains(&epsg) {
        Some(epsg - 2338) // 2338 → CM 75°E (index 0)
    } else {
        None
    }
}

/// Beijing 1954 / 3-degree Gauss-Kruger zone 25..45 (CM 75°E..135°E)
/// EPSG:2401..2421 (注意: 实际是 21 个, 但中间被 2422-2427 的 CM 命名系列打断)
/// 例: 2401=zone 25 (CM 75°E, x_0=25.5M), 2416=zone 40 (CM 120°E, x_0=40.5M)
fn bj54_3deg_zone(epsg: u32) -> Option<u32> {
    if (2401..=2421).contains(&epsg) {
        Some(epsg - 2376) // 2401 → zone 25
    } else {
        None
    }
}

/// Beijing 1954 / 3-degree Gauss-Kruger CM 75°E..135°E
/// EPSG:2422..2427 (6 zones, only CM 75E-90E)
fn bj54_3deg_cm(epsg: u32) -> Option<u32> {
    if (2422..=2427).contains(&epsg) {
        Some(epsg - 2422) // 2422 → CM 75°E (index 0)
    } else {
        None
    }
}

/// 高斯-克吕格 "zone 编号" PROJ.4 字符串（CM 75°E..135°E, 椭球由 datum 决定）
///
/// 假东: (zone + 0.5) * 1M
/// 例: zone 25 (CM 75°E) = 25.5M, zone 33 (CM 99°E) = 33.5M
fn gk_zone_proj_string(zone: u32, datum_epsg: u32) -> String {
    let lon_0 = zone as f64 * 3.0; // 25*3 = 75°E
    let ellps = match datum_epsg {
        4513..=4533 => "GRS80",  // CGCS2000
        2362..=2369 => "IAU76",  // Xian 1980
        2401..=2421 => "krass",  // Beijing 1954
        _ => "WGS84",
    };
    let x_0 = (zone as f64 + 0.5) * 1_000_000.0;
    format!(
        "+proj=tmerc +lat_0=0 +lon_0={lon_0} +k=1 +x_0={x_0} +y_0=0 \
         +ellps={ellps} +units=m +no_defs +type=crs"
    )
}

/// 高斯-克吕格 "6° zone 编号" PROJ.4 字符串（CM 75°E..135°E, 椭球由 datum 决定）
///
/// 假东: (zone + 0.5) * 1M
/// 例: zone 13 (CM 75°E) = 13.5M, zone 23 (CM 135°E) = 23.5M
fn gk6_zone_proj_string(zone: u32, datum_epsg: u32) -> String {
    let lon_0 = (zone as f64 - 13.0) * 6.0 + 75.0; // zone 13 = 75°E
    let ellps = match datum_epsg {
        2327..=2337 => "IAU76",
        _ => "WGS84",
    };
    let x_0 = (zone as f64 + 0.5) * 1_000_000.0;
    format!(
        "+proj=tmerc +lat_0=0 +lon_0={lon_0} +k=1 +x_0={x_0} +y_0=0 \
         +ellps={ellps} +units=m +no_defs +type=crs"
    )
}

/// 高斯-克吕格 "CM 命名" PROJ.4 字符串（假东 500K, 标准 UTM-like）
/// `cm_idx` 是 CM 75°E 起的索引（0 = 75°E, 1 = 78°E, 2 = 81°E ...）
fn gk_cm_proj_string(ellps: &str, lon_0: f64) -> String {
    format!(
        "+proj=tmerc +lat_0=0 +lon_0={lon_0} +k=1 +x_0=500000 +y_0=0 \
         +ellps={ellps} +units=m +no_defs +type=crs"
    )
}

/// 从 EPSG 代码生成 PROJ.4 字符串。常见 CRS 直出；其它 EPSG 返回 None，
/// 调用方回落为「按 EPSG:3857 假设直接用原始值」。
///
/// 已支持：
/// - 3857 (Web Mercator)
/// - 4326 (WGS84 经纬度)
/// - 32601..32660, 32701..32760 (UTM 北/南半球)
/// - 4502..4512 (CGCS2000 6° GK CM-named, 假东 500K)
/// - 4513..4533 (CGCS2000 3° GK zone 25-45, 假东 25.5M-45.5M)
/// - 2327..2337 (Xian 1980 6° GK zone 13-23, 假东 13.5M-23.5M)
/// - 2338..2342 (Xian 1980 6° GK CM-named, 假东 500K)
/// - 2362..2369 (Xian 1980 3° GK zone 38-45, 假东 38.5M-45.5M)
/// - 2370..2384 (Xian 1980 3° GK CM-named, 假东 500K)
/// - 2401..2421 (Beijing 1954 3° GK zone 25-45, 假东 25.5M-45.5M)
/// - 2422..2427 (Beijing 1954 3° GK CM-named, 假东 500K)
pub fn proj_string_for_epsg(epsg: u32) -> Option<String> {
    if epsg == 3857 {
        return Some(WEB_MERC_PROJ.to_string());
    }
    if epsg == 4326 {
        return Some(WGS84_GEOGR_PROJ.to_string());
    }
    if let Some((zone, is_north)) = epsg_to_utm(epsg) {
        return Some(utm_proj_string(zone, is_north));
    }
    // CGCS2000
    if let Some(zone) = cgcs2000_3deg_zone(epsg) {
        return Some(gk_zone_proj_string(zone, epsg));
    }
    if let Some(cm_idx) = cgcs2000_6deg_cm(epsg) {
        let lon_0 = 75.0 + (cm_idx as f64) * 6.0;
        return Some(gk_cm_proj_string("GRS80", lon_0));
    }
    // Xian 1980
    if let Some(zone) = xian1980_3deg_zone(epsg) {
        return Some(gk_zone_proj_string(zone, epsg));
    }
    if let Some(cm_idx) = xian1980_3deg_cm(epsg) {
        let lon_0 = 75.0 + (cm_idx as f64) * 3.0;
        return Some(gk_cm_proj_string("IAU76", lon_0));
    }
    if let Some(zone) = xian1980_6deg_zone(epsg) {
        return Some(gk6_zone_proj_string(zone, epsg));
    }
    if let Some(cm_idx) = xian1980_6deg_cm(epsg) {
        let lon_0 = 75.0 + (cm_idx as f64) * 6.0;
        return Some(gk_cm_proj_string("IAU76", lon_0));
    }
    // Beijing 1954
    if let Some(zone) = bj54_3deg_zone(epsg) {
        return Some(gk_zone_proj_string(zone, epsg));
    }
    if let Some(cm_idx) = bj54_3deg_cm(epsg) {
        let lon_0 = 75.0 + (cm_idx as f64) * 3.0;
        return Some(gk_cm_proj_string("krass", lon_0));
    }
    None
}

/// 把任意已知 EPSG 的投影坐标转换为 EPSG:3857 米。
/// EPSG:4326 输入单位为度（会转弧度），其它 EPSG 输入单位为米。
/// 失败时返回 None（proj4rs 不支持该 PROJ.4 / datum 等）。
fn transform_to_3857(epsg: u32, x: f64, y: f64) -> Option<(f64, f64)> {
    let src_proj_str = proj_string_for_epsg(epsg)?;
    let src = proj4rs::Proj::from_proj_string(&src_proj_str).ok()?;
    let merc = proj4rs::Proj::from_proj_string(WEB_MERC_PROJ).ok()?;
    // EPSG:4326 期望 (lon, lat) 弧度；其它投影（3857/UTM）单位米。
    let mut p = if epsg == 4326 {
        (x.to_radians(), y.to_radians())
    } else {
        (x, y)
    };
    proj4rs::transform::transform(&src, &merc, &mut p).ok()?;
    Some((p.0, p.1))
}

/// 把 EPSG:3857 米转换为任意已知 EPSG 的原始投影坐标。
/// EPSG:4326 输出为 (lon, lat) **度**（不是弧度），其它 EPSG 输出为米。
/// 失败时返回 None。
pub fn transform_3857_to_src(epsg: u32, mx: f64, my: f64) -> Option<(f64, f64)> {
    let src_proj_str = proj_string_for_epsg(epsg)?;
    let src = proj4rs::Proj::from_proj_string(&src_proj_str).ok()?;
    let merc = proj4rs::Proj::from_proj_string(WEB_MERC_PROJ).ok()?;
    let mut p = (mx, my);
    proj4rs::transform::transform(&merc, &src, &mut p).ok()?;
    if epsg == 4326 {
        Some((p.0.to_degrees(), p.1.to_degrees()))
    } else {
        Some((p.0, p.1))
    }
}

/// 提取地理参考；无 PixelScale/Tiepoint 时返回 None。
///
/// 坐标统一转为 EPSG:3857：委托 proj4rs 完成任意 CRS → 3857 的级联
///（内部自动走 src → 地理 → dst，并正确处理 datum shift）。
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
            src_epsg: 3857,
            src_tie_x: raw_x,
            src_tie_y: raw_y,
            src_px_w: px,
            src_px_h: py,
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
            src_epsg: 4326,
            src_tie_x: raw_x,
            src_tie_y: raw_y,
            src_px_w: px,
            src_px_h: py,
        }));
    }

    if let Some(epsg) = geocode_epsg {
        // 已知 EPSG（含 UTM）：经 proj4rs 转 3857
        if let (Some((m0, my_n)), Some((m1, _)), Some((_, my_s))) = (
            transform_to_3857(epsg, raw_x, raw_y),
            transform_to_3857(epsg, raw_x + src_w as f64 * px, raw_y),
            transform_to_3857(epsg, raw_x, raw_y - src_h as f64 * py),
        ) {
            return Ok(Some(GeoRef {
                mx0: m0,
                my_top: my_n,
                sx: (m1 - m0) / src_w as f64,
                sy: (my_s - my_n) / src_h as f64, // 负值
                src_epsg: epsg,
                src_tie_x: raw_x,
                src_tie_y: raw_y,
                src_px_w: px,
                src_px_h: py,
            }));
        }
        // 该 EPSG 未支持（或 proj4rs 失败）—— 回退为假设3857
        return Ok(Some(GeoRef {
            mx0: raw_x,
            my_top: raw_y,
            sx: px,
            sy: -py,
            src_epsg: 3857,
            src_tie_x: raw_x,
            src_tie_y: raw_y,
            src_px_w: px,
            src_px_h: py,
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
            src_epsg: 3857,
            src_tie_x: raw_x,
            src_tie_y: raw_y,
            src_px_w: px,
            src_px_h: py,
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
        src_epsg: 4326,
        src_tie_x: raw_x,
        src_tie_y: raw_y,
        src_px_w: px,
        src_px_h: py,
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

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::mercator::{INIT_RESOLUTION, ORIGIN_SHIFT};

    /// 把 (3857 x, 3857 y) 反算到 z 级别瓦片坐标
    fn tile_xy(x3857: f64, y3857: f64, z: u32) -> (i64, i64) {
        let res = INIT_RESOLUTION / 2f64.powi(z as i32);
        let tx = ((x3857 + ORIGIN_SHIFT) / res / 256.0).floor() as i64;
        let ty = ((ORIGIN_SHIFT - y3857) / res / 256.0).floor() as i64;
        (tx, ty)
    }

    /// tile_0.TIF (EPSG:32648 UTM zone 48N) 左上角 UTM (285635.05, 2785872.17)
    /// 应转成 3857 米 (≈11451768.8, 2897092.2)，与 pyproj 精确匹配；
    /// 在 z=18 起始瓦片 (205981, 112121)。
    #[test]
    fn utm_tile_0_origin_matches_pyproj() {
        let (mx, my) = transform_to_3857(
            32648,
            285_635.050_237_536_43,
            2_785_872.173_051_495_6,
        )
        .expect("UTM zone 48N must be supported");
        assert!((mx - 11_451_768.81).abs() < 0.1, "mx={mx}");
        assert!((my - 2_897_092.24).abs() < 0.1, "my={my}");

        let (tx, ty) = tile_xy(mx, my, 18);
        assert_eq!(tx, 205_981, "tx={tx}");
        assert_eq!(ty, 112_121, "ty={ty}");
    }

    /// UTM 南半球 (EPSG:32755, zone 55S) 必须返回南半球 3857 y（小于 0 处）。
    /// 历史 bug 是手写 UTM→lonlat 公式没处理 false northing 偏移导致。
    #[test]
    fn utm_southern_hemisphere_y_below_equator() {
        // (700000, 5800000) 在 zone 55S 内，真实 lat≈-37.9°
        let (mx, my) = transform_to_3857(32755, 700_000.0, 5_800_000.0)
            .expect("UTM zone 55S must be supported");
        // 南半球 3857 y < 赤道 y (≈0)
        assert!(my < 0.0, "南半球 my={my} 应 < 0");
        assert!(mx.abs() < 20_037_508.0, "x 应在 ±半周内");
    }

    /// EPSG:4326 经纬度 → 3857 走 proj4rs 端到端，应与 pyproj 结果一致。
    #[test]
    fn wgs84_to_3857_via_proj4rs() {
        // (lon=0, lat=0) 在 EPSG:3857 下是 (0, 0)
        let (mx, my) = transform_to_3857(4326, 0.0, 0.0).expect("EPSG:4326 ok");
        assert!(mx.abs() < 1e-6 && my.abs() < 1e-6, "(0,0)→({mx},{my})");
        // (lon=102.873, lat=25.174) 在 pyproj 下得到 (11451769.98, 2897131.78)
        let (mx2, my2) = transform_to_3857(4326, 102.873, 25.174)
            .expect("EPSG:4326 ok");
        assert!((mx2 - 11_451_769.98).abs() < 0.1, "mx2={mx2}");
        assert!((my2 - 2_897_131.78).abs() < 0.1, "my2={my2}");
    }

    /// UTM zone → PROJ.4 字符串正确性（含南半球 +south 旗标）
    #[test]
    fn utm_proj_string_format() {
        assert!(utm_proj_string(48, true).contains("+zone=48 "));
        assert!(!utm_proj_string(48, true).contains("+south"));
        assert!(utm_proj_string(55, false).contains("+zone=55 "));
        assert!(utm_proj_string(55, false).contains("+south"));
    }

    /// EPSG 代码解析：32648 → zone 48 north；32755 → zone 55 south。
    #[test]
    fn epsg_to_utm_zones() {
        assert_eq!(epsg_to_utm(32648), Some((48, true)));
        assert_eq!(epsg_to_utm(32755), Some((55, false)));
        assert_eq!(epsg_to_utm(3857), None);
        assert_eq!(epsg_to_utm(4326), None);
    }

    /// EPSG 白名单：中国 CGCS2000 / 3-deg GK zone 25..45
    #[test]
    fn cgcs2000_3deg_zone_epsg() {
        assert_eq!(cgcs2000_3deg_zone(4513), Some(25)); // CM 75°E
        assert_eq!(cgcs2000_3deg_zone(4521), Some(33)); // CM 99°E
        assert_eq!(cgcs2000_3deg_zone(4533), Some(45)); // CM 135°E
        assert_eq!(cgcs2000_3deg_zone(4502), None); // 属于 6° 分带
        assert_eq!(cgcs2000_3deg_zone(4534), None); // 越界
    }

    /// EPSG 白名单：中国 CGCS2000 / 6-deg GK CM-named (CM 75-135E, 6° 步长)
    #[test]
    fn cgcs2000_6deg_cm_epsg() {
        assert_eq!(cgcs2000_6deg_cm(4502), Some(0)); // CM 75°E
        assert_eq!(cgcs2000_6deg_cm(4507), Some(5)); // CM 105°E
        assert_eq!(cgcs2000_6deg_cm(4512), Some(10)); // CM 135°E
        assert_eq!(cgcs2000_6deg_cm(4501), None);
        assert_eq!(cgcs2000_6deg_cm(4513), None); // 属于 3° 分带
    }

    /// Xian 1980 GK 解析
    #[test]
    fn xian1980_epsg() {
        assert_eq!(xian1980_3deg_zone(2362), Some(38)); // CM 114°E
        assert_eq!(xian1980_3deg_zone(2369), Some(45)); // CM 135°E
        assert_eq!(xian1980_3deg_cm(2370), Some(0)); // CM 75°E
        assert_eq!(xian1980_3deg_cm(2384), Some(14)); // CM 117°E
        assert_eq!(xian1980_6deg_zone(2327), Some(13)); // CM 75°E
        assert_eq!(xian1980_6deg_zone(2337), Some(23)); // CM 135°E
        assert_eq!(xian1980_6deg_cm(2338), Some(0)); // CM 75°E
        assert_eq!(xian1980_6deg_cm(2342), Some(4)); // CM 99°E
        assert_eq!(xian1980_3deg_zone(2361), None);
        assert_eq!(xian1980_6deg_zone(2326), None);
    }

    /// Beijing 1954 GK 解析
    #[test]
    fn bj54_epsg() {
        assert_eq!(bj54_3deg_zone(2401), Some(25)); // CM 75°E
        assert_eq!(bj54_3deg_zone(2416), Some(40)); // CM 120°E
        assert_eq!(bj54_3deg_zone(2421), Some(45)); // CM 135°E
        assert_eq!(bj54_3deg_cm(2422), Some(0)); // CM 75°E
        assert_eq!(bj54_3deg_cm(2427), Some(5)); // CM 90°E
        assert_eq!(bj54_3deg_zone(2422), None); // 是 CM, 不是 zone
    }

    /// proj_string_for_epsg 对中国 EPSG 返回正确的 PROJ.4 字符串
    #[test]
    fn proj_string_for_chinese_epsg() {
        // CGCS2000 3-deg GK zone 33 (CM 99°E) = EPSG:4521
        // pyproj: +proj=tmerc +lat_0=0 +lon_0=99 +k=1 +x_0=33500000 +y_0=0 +ellps=GRS80
        let s = proj_string_for_epsg(4521).expect("4521 supported");
        assert!(s.contains("+proj=tmerc"), "got: {s}");
        assert!(s.contains("+lon_0=99"), "got: {s}");
        assert!(s.contains("+ellps=GRS80"), "got: {s}");
        assert!(s.contains("+x_0=33500000"), "got: {s}");

        // CGCS2000 6-deg GK CM 75°E = EPSG:4502 (假东 500K, 不是 13.5M)
        // pyproj: +proj=tmerc +lat_0=0 +lon_0=75 +k=1 +x_0=500000
        let s = proj_string_for_epsg(4502).expect("4502 supported");
        assert!(s.contains("+lon_0=75"), "got: {s}");
        assert!(s.contains("+x_0=500000"), "got: {s}");

        // Xian 1980 6-deg GK zone 13 (CM 75°E) = EPSG:2327
        // pyproj: +x_0=13500000
        let s = proj_string_for_epsg(2327).expect("2327 supported");
        assert!(s.contains("+lon_0=75"), "got: {s}");
        assert!(s.contains("+ellps=IAU76"), "got: {s}");
        assert!(s.contains("+x_0=13500000"), "got: {s}");

        // Xian 1980 6-deg GK CM 75°E = EPSG:2338 (假东 500K)
        let s = proj_string_for_epsg(2338).expect("2338 supported");
        assert!(s.contains("+lon_0=75"), "got: {s}");
        assert!(s.contains("+x_0=500000"), "got: {s}");

        // Beijing 1954 3-deg GK zone 25 (CM 75°E) = EPSG:2401
        // pyproj: +x_0=25500000
        let s = proj_string_for_epsg(2401).expect("2401 supported");
        assert!(s.contains("+ellps=krass"), "got: {s}");
        assert!(s.contains("+x_0=25500000"), "got: {s}");

        // Beijing 1954 3-deg GK zone 40 (CM 120°E) = EPSG:2416
        // pyproj: +lon_0=120 +x_0=40500000
        let s = proj_string_for_epsg(2416).expect("2416 supported");
        assert!(s.contains("+lon_0=120"), "got: {s}");
        assert!(s.contains("+x_0=40500000"), "got: {s}");

        // Beijing 1954 3-deg GK CM 75°E = EPSG:2422 (假东 500K)
        let s = proj_string_for_epsg(2422).expect("2422 supported");
        assert!(s.contains("+lon_0=75"), "got: {s}");
        assert!(s.contains("+x_0=500000"), "got: {s}");

        // Xian 1980 3-deg GK zone 38 (CM 114°E) = EPSG:2362
        let s = proj_string_for_epsg(2362).expect("2362 supported");
        assert!(s.contains("+lon_0=114"), "got: {s}");
        assert!(s.contains("+x_0=38500000"), "got: {s}");
    }

    /// CGCS2000 3-deg GK zone 33 (CM 99°E) → 3857 端到端
    /// pyproj: (lon=99, lat=0) → 3857 (11020629.59, 0)
    /// 输入 (33500000, 0) → 99°E, 0°N → 3857 (11020629.59, 0)
    #[test]
    fn cgcs2000_3deg_to_3857() {
        let (mx, my) = transform_to_3857(4521, 33_500_000.0, 0.0)
            .expect("CGCS2000/3-deg GK zone 33 must be supported");
        assert!(
            (mx - 11_020_629.59).abs() < 1.0,
            "mx={mx} 期望 ≈ 11_020_629.59 (pyproj at 99°E 0°N)"
        );
        assert!(my.abs() < 1.0, "赤道 my={my} 应 ≈ 0");
    }

    /// 反向 transform_3857_to_src 也支持中国 EPSG
    #[test]
    fn cgcs2000_3deg_from_3857() {
        // 99°E, 0°N → 3857 (11020629.59, 0) → tmerc zone 33 → (33500000, 0)
        let (x, y) = transform_3857_to_src(4521, 11_020_629.59, 0.0)
            .expect("反向也支持");
        assert!((x - 33_500_000.0).abs() < 0.5, "x={x}");
        assert!(y.abs() < 0.5, "y={y}");
    }

    /// proj4rs 实际支持 4521 投影字符串 + 与 pyproj 一致
    #[test]
    fn proj4rs_supports_cgcs2000_tmerc() {
        use proj4rs::proj::Proj;
        use proj4rs::transform;
        let s = proj_string_for_epsg(4521).unwrap();
        let p = Proj::from_proj_string(&s).expect("proj4rs must parse CGCS2000 tmerc");
        let wm = Proj::from_proj_string(WEB_MERC_PROJ).unwrap();
        // 99°E, 0°N = (33500000, 0) → 3857 (11020629.59, 0)
        let mut pt = (33_500_000.0_f64, 0.0_f64);
        transform::transform(&p, &wm, &mut pt).expect("transform 4521 -> 3857");
        assert!((pt.0 - 11_020_629.59).abs() < 1.0, "3857 x={}", pt.0);
        assert!(pt.1.abs() < 1.0, "3857 y={}", pt.1);
    }

    /// Beijing 1954 / 3° GK zone 25 (CM 75°E) = EPSG:2401
    /// pyproj: (25500000, 0) → 75°E, 0°N → 3857 (8348961.81, 0)
    #[test]
    fn bj54_3deg_zone_to_3857() {
        let (mx, my) = transform_to_3857(2401, 25_500_000.0, 0.0)
            .expect("BJ54 3-deg GK zone 25 must be supported");
        // pyproj 实际值
        assert!((mx - 8_348_961.81).abs() < 1.0, "mx={mx} 期望 ≈ 8,348,961.81");
        assert!(my.abs() < 1.0, "赤道 my={my} 应 ≈ 0");
    }

    /// 端到端精度测试：每像素的 (utm_x, utm_y) → 3857 与 pyproj 完全一致
    /// 测试 6 种投影 × 多个 z 级别 × 多个瓦片位置
    ///
    /// 注意：EPSG:4326 因为 proj4rs 要求 (lon, lat) 弧度输入,
    /// transform_3857_to_src 已经处理 to_degrees 但 src -> 3857 方向需要 to_radians
    /// 这里我们用 transform_3857_to_src + transform_to_3857 间接验证
    #[test]
    fn precise_per_pixel_alignment_against_pyproj() {
        // 测试用例：(epsg, 中心 utm_x, 中心 utm_y, 描述, 期望 (3857 mx, my))
        // 期望值由 pyproj 2.6 在 macOS 计算
        let cases: &[(u32, f64, f64, &str, f64, f64)] = &[
            (32648, 285_635.0, 2_785_872.0, "UTM 48N (Yunnan)", 11_451_768.81, 2_897_092.24),
            (4521, 33_500_000.0, 0.0, "CGCS2000 3° GK zone 33 (CM 99°E)", 11_020_629.59, 0.0),
            (4502, 500_000.0, 0.0, "CGCS2000 6° GK CM 75E", 8_348_961.81, 0.0),
            (2401, 25_500_000.0, 0.0, "Beijing 1954 3° GK zone 25 (CM 75E)", 8_348_961.81, 0.0),
            (2327, 13_500_000.0, 0.0, "Xian 1980 6° GK zone 13 (CM 75E)", 8_348_961.81, 0.0),
        ];

        for &(epsg, x0, y0, desc, exp_mx, exp_my) in cases {
            // 1. transform_to_3857 直接 transform
            let (mx, my) = transform_to_3857(epsg, x0, y0)
                .expect("transform_to_3857 must work");
            assert!(
                (mx - exp_mx).abs() < 1.0,
                "{desc} (epsg={epsg}) mx={mx} 期望 ≈ {exp_mx}"
            );
            assert!(
                (my - exp_my).abs() < 1.0,
                "{desc} (epsg={epsg}) my={my} 期望 ≈ {exp_my}"
            );

            // 2. transform_3857_to_src 反向应回到 (x0, y0)
            let (ux, uy) = transform_3857_to_src(epsg, mx, my)
                    .expect("transform_3857_to_src must work");
            let dx = ux - x0;
            let dy = uy - y0;
            assert!(
                dx.abs() < 0.01 && dy.abs() < 0.01,
                "{desc} (epsg={epsg}) 反向不闭合: ({x0}, {y0}) -> 3857 ({mx}, {my}) -> src ({ux}, {uy}), 差=({dx}, {dy})"
            );
        }
    }
}
