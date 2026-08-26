//! Web-Mercator 绝对级别金字塔（对齐 gdal2tiles -p mercator 语义）。
//!
//! 级别为全球 XYZ 绝对缩放级：z 级世界宽 256·2^z px；
//! 每级瓦片数 = 与影像 3857 范围相交的全球网格单元数；
//! zmax 截断到 native zoom（不生成放大级），与 gdal2tiles 行为一致。

use crate::engine::error::{CoreError, CoreResult};

/// Web-Mercator 初始分辨率（米/px @z0，tile=256）
pub const INIT_RESOLUTION: f64 = 156543.03392804062;
/// Web-Mercator 半幅（±20037508.34…）
pub const ORIGIN_SHIFT: f64 = 20037508.342789244;

/// 某级别的全球网格瓦片范围（含端点）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MercLevel {
    pub z: u32,
    pub tx0: u32,
    pub ty0: u32,
    pub tx1: u32,
    pub ty1: u32,
}

impl MercLevel {
    pub fn count(&self) -> u64 {
        (self.tx1 - self.tx0 + 1) as u64 * (self.ty1 - self.ty0 + 1) as u64
    }
}

/// Mercator 金字塔计划
#[derive(Debug, Clone)]
pub struct MercPlan {
    /// 影像 GSD 对应的 native zoom（用户请求超出时被截断到此值）
    pub native_zoom: u32,
    pub levels: Vec<MercLevel>,
    pub total_tiles: u64,
}

/// gdal2tiles ZoomForPixelSize：最大的 z 使 Resolution(z) ≥ pixel_size
pub fn zoom_for_pixel_size(pixel_size: f64) -> u32 {
    for i in 0..=30u32 {
        let res = INIT_RESOLUTION / 2f64.powi(i as i32);
        if pixel_size > res {
            return i.saturating_sub(1);
        }
    }
    30
}

fn level_range(bounds: [f64; 4], z: u32, tile: u32) -> MercLevel {
    let res = INIT_RESOLUTION / 2f64.powi(z as i32);
    let px = |mx: f64| (mx + ORIGIN_SHIFT) / res;
    let py = |my: f64| (ORIGIN_SHIFT - my) / res;
    let t = tile as f64;
    let [minx, miny, maxx, maxy] = bounds;
    let tx0 = ((px(minx) / t).floor()).max(0.0) as u32;
    let ty0 = ((py(maxy) / t).floor()).max(0.0) as u32;
    let tx1 = (px(maxx) / t).floor().max(0.0) as u32;
    let ty1 = (py(miny) / t).floor().max(0.0) as u32;
    MercLevel {
        z,
        tx0,
        ty0,
        tx1,
        ty1,
    }
}

/// 规划 mercator 金字塔。
/// 对齐 gdal2tiles：用户显式指定的级别范围**不截断到 native**，
/// 超出 native 的级别按放大渲染（与 -z 1-22 请求超出时行为一致）。
pub fn plan(
    bounds3857: [f64; 4],
    sx_m: f64,
    tile: u32,
    req_min: Option<u32>,
    req_max: Option<u32>,
    max_total: u64,
) -> CoreResult<MercPlan> {
    let nz = zoom_for_pixel_size(sx_m.abs());
    let zmin = req_min.unwrap_or(0);
    let zmax = req_max.unwrap_or(nz).max(zmin);

    let mut levels = Vec::new();
    let mut total: u64 = 0;
    for z in zmin..=zmax {
        let lv = level_range(bounds3857, z, tile);
        total += lv.count();
        if total > max_total {
            return Err(CoreError::InvalidInput(format!(
                "级别范围 [{zmin}, {zmax}] 预计瓦片数超过 {max_total} 上限"
            )));
        }
        levels.push(lv);
    }
    Ok(MercPlan {
        native_zoom: nz,
        levels,
        total_tiles: total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_for_pixel_size_matches_gdal() {
        // z0 分辨率本身 → zoom 0
        assert_eq!(zoom_for_pixel_size(156543.034), 0);
        assert_eq!(zoom_for_pixel_size(156543.0), 0);
        // 恰好 z1 分辨率
        assert_eq!(zoom_for_pixel_size(156543.03392804062 / 2.0), 1);
        assert_eq!(zoom_for_pixel_size(1.0), 17); // ~1m/px → z17（与常用经验一致）
    }

    #[test]
    fn small_image_low_zoom_single_tile() {
        // 小范围影像在低级别必然只占 1 个全球瓦片（对齐参考输出 Z1..Z8 各 1）
        let b = [12900000.0, 4740000.0, 12960000.0, 4800000.0]; // ~6km × 6km
        let p = plan(b, 0.5971642834779395, 256, Some(1), Some(8), u64::MAX).unwrap();
        assert_eq!(p.levels.len(), 8);
        for lv in &p.levels[..7] {
            assert_eq!(lv.count(), 1, "z={} 应为单瓦片", lv.z);
        }
    }

    #[test]
    fn growth_quadruples_per_level_when_resolution_dominated() {
        let b = [12900000.0, 4740000.0, 12960000.0, 4800000.0];
        let p = plan(b, 0.2985821417, 256, Some(14), Some(19), u64::MAX).unwrap();
        let counts: Vec<u64> = p.levels.iter().map(|l| l.count()).collect();
        for w in counts.windows(2) {
            let ratio = w[1] as f64 / w[0] as f64;
            assert!((ratio - 4.0).abs() < 0.6, "相邻级别应约 ×4，实际 {ratio}");
        }
    }

    #[test]
    fn honors_requested_range_beyond_native() {
        // 对齐 gdal2tiles：显式指定范围不截断；超出 native 的级别继续 ×4 放大
        let b = [12900000.0, 4740000.0, 12960000.0, 4800000.0];
        let sx = 0.6; // 介于 z17(1.19) 与 z18(0.597) → native 17
        let p = plan(b, sx, 256, Some(1), Some(20), u64::MAX).unwrap();
        assert_eq!(p.levels.first().unwrap().z, 1);
        assert_eq!(p.levels.last().unwrap().z, 20);
        assert_eq!(p.native_zoom, 17);
        // 超出 native 后每级仍为全球网格相交计数（×4 增长）
        let c19 = p.levels.iter().find(|l| l.z == 19).unwrap().count();
        let c20 = p.levels.iter().find(|l| l.z == 20).unwrap().count();
        assert!((c20 as f64 / c19 as f64) > 3.0 && (c20 as f64 / c19 as f64) < 5.0);
    }
}
