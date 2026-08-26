//! 金字塔级别规划：Google 风格金字塔，level 0 为整图一张瓦片，
//! level N 为原始分辨率所在级（native level）。

use serde::{Deserialize, Serialize};

use super::error::{CoreError, CoreResult};

pub const DEFAULT_TILE_SIZE: u32 = 256;

/// 单个级别的计划
#[derive(Debug, Clone, Serialize)]
pub struct LevelPlan {
    pub level: u32,
    /// 该级整幅影像尺寸（像素）
    pub width: u32,
    pub height: u32,
    /// 该级瓦片网格
    pub tiles_x: u32,
    pub tiles_y: u32,
    /// 源像素 / 输出像素（>1 缩小，<1 放大）
    pub scale: f64,
}

/// 级别硬上限（对齐主流切片工具：0–22 自由设置）
pub const MAX_LEVEL_CAP: u32 = 22;

#[derive(Debug, Clone, Serialize)]
pub struct PyramidPlan {
    pub image_width: u32,
    pub image_height: u32,
    pub tile_size: u32,
    pub max_level: u32,
    pub min_level_requested: u32,
    pub max_level_requested: u32,
    pub levels: Vec<LevelPlan>,
    pub total_tiles: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scheme {
    /// {out}/{z}/{x}/{y}.png
    Xyz,
    /// {out}/{z}/{x}/{y}.png 且 y 翻转
    Tms,
}

impl Scheme {
    pub fn as_str(self) -> &'static str {
        match self {
            Scheme::Xyz => "xyz",
            Scheme::Tms => "tms",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Resample {
    Nearest,
    Bilinear,
}

/// 计算 native level：整张图恰好放进一张瓦片之上、原始分辨率所在的最低级别。
pub fn native_level(width: u32, height: u32, tile_size: u32) -> u32 {
    let longest = width.max(height).max(1);
    let ratio = longest as f64 / tile_size.max(1) as f64;
    ratio.log2().ceil().max(0.0) as u32
}

/// 计算某级别的显示尺寸与瓦片数。level > max_level 时按 2^n 上采样。
pub fn level_dims(width: u32, height: u32, level: u32, max_level: u32) -> (u32, u32) {
    if level <= max_level {
        let ds = 1u32 << (max_level - level).min(31);
        ((width + ds - 1) / ds, (height + ds - 1) / ds)
    } else {
        let up = 1u32 << (level - max_level).min(24);
        (
            (width as u64 * up as u64).min(u32::MAX as u64) as u32,
            (height as u64 * up as u64).min(u32::MAX as u64) as u32,
        )
    }
}

pub fn plan(
    image_width: u32,
    image_height: u32,
    tile_size: u32,
    min_level_req: Option<u32>,
    max_level_req: Option<u32>,
) -> CoreResult<PyramidPlan> {
    if image_width == 0 || image_height == 0 {
        return Err(CoreError::InvalidInput("图像尺寸为零".into()));
    }
    if !(64..=1024).contains(&tile_size) || !tile_size.is_power_of_two() {
        return Err(CoreError::InvalidInput(format!(
            "瓦片尺寸必须为 64~1024 的 2 的幂，当前 {tile_size}"
        )));
    }

    let max_level = native_level(image_width, image_height, tile_size);
    // 级别自由设置：[0, MAX_LEVEL_CAP]，zmax 可自由超出 native（放大输出）
    let req_min = min_level_req.unwrap_or(0);
    let req_max = max_level_req.unwrap_or(max_level);
    if req_min > req_max {
        return Err(CoreError::InvalidInput(format!(
            "级别区间无效: [{req_min}, {req_max}]"
        )));
    }
    if req_min > MAX_LEVEL_CAP {
        return Err(CoreError::InvalidInput(format!(
            "最小级别 {req_min} 超出上限 {MAX_LEVEL_CAP}"
        )));
    }
    let zmin = req_min;
    let zmax = req_max.min(MAX_LEVEL_CAP).max(zmin);

    let mut levels = Vec::new();
    let mut total: u64 = 0;
    for level in zmin..=zmax {
        let (w, h) = level_dims(image_width, image_height, level, max_level);
        let tx = (w + tile_size - 1) / tile_size;
        let ty = (h + tile_size - 1) / tile_size;
        total += tx as u64 * ty as u64;
        let exp = max_level as i64 - level as i64; // >0 缩小，<0 放大
        let scale = 2f64.powi(exp.clamp(-30, 30) as i32);
        levels.push(LevelPlan {
            level,
            width: w,
            height: h,
            tiles_x: tx,
            tiles_y: ty,
            scale,
        });
    }

    Ok(PyramidPlan {
        image_width,
        image_height,
        tile_size,
        max_level,
        min_level_requested: zmin,
        max_level_requested: zmax,
        total_tiles: total,
        levels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_level_basic() {
        assert_eq!(native_level(256, 256, 256), 0);
        assert_eq!(native_level(512, 256, 256), 1);
        assert_eq!(native_level(10000, 8000, 256), 6); // 39.06 -> ceil log2 = 6
        assert_eq!(native_level(100, 100, 256), 0);
    }

    #[test]
    fn plan_full_range() {
        let p = plan(2048, 1024, 256, None, None).unwrap();
        assert_eq!(p.max_level, 3);
        assert_eq!(p.levels.len(), 4);
        // L0: 256x128 -> 1x1; L1: 512x256 -> 2x1; L2: 4x2; L3: 8x4
        assert_eq!(p.levels[0].tiles_x * p.levels[0].tiles_y, 1);
        assert_eq!(p.levels[3].tiles_x, 8);
        assert_eq!(p.levels[3].tiles_y, 4);
        assert_eq!(p.total_tiles, 1 + 2 + 8 + 32);
    }

    #[test]
    fn plan_partial_range_and_validation() {
        let p = plan(4096, 4096, 256, Some(2), Some(4)).unwrap();
        assert_eq!(p.levels.len(), 3);
        assert!(matches!(
            plan(100, 100, 300, None, None),
            Err(CoreError::InvalidInput(_))
        ));
        assert!(matches!(
            plan(100, 100, 256, Some(3), Some(1)),
            Err(CoreError::InvalidInput(_))
        ));
        // clamp to hard cap 22
        let p = plan(512, 512, 256, Some(0), Some(99)).unwrap();
        assert_eq!(p.max_level_requested, MAX_LEVEL_CAP);
    }

    #[test]
    fn free_level_range_0_to_22() {
        // 自由选择：zmin 可 >0，zmax 可到 22
        let p = plan(4096, 4096, 256, Some(3), Some(22)).unwrap();
        assert_eq!(p.min_level_requested, 3);
        assert_eq!(p.max_level_requested, 22);
        assert_eq!(p.levels.len(), 20);
        // native = log2(4096/256)=4；L6 = ×4 放大
        let l6 = p.levels.iter().find(|l| l.level == 6).unwrap();
        assert_eq!((l6.width, l6.height), (16384, 16384));
        assert!((l6.scale - 0.25).abs() < 1e-9);
        // 上采样尺寸受 u32 保护不 panic（L22 = ×2^18）
        let l22 = p.levels.last().unwrap();
        assert!(l22.width >= 1 && l22.tiles_x >= 1);
    }

    #[test]
    fn upsample_levels_beyond_native() {
        // native(512,256)=1；请求到 L3 = 原始分辨率 ×2
        let p = plan(512, 256, 256, Some(0), Some(3)).unwrap();
        assert_eq!(p.levels.len(), 4);
        let l2 = &p.levels[2];
        assert_eq!((l2.width, l2.height), (1024, 512));
        assert!((l2.scale - 0.5).abs() < 1e-9);
        assert_eq!(l2.tiles_x, 4);
        // L1 = native，scale=1
        let l1 = &p.levels[1];
        assert_eq!((l1.width, l1.height), (512, 256));
        assert!((l1.scale - 1.0).abs() < 1e-9);
    }

    #[test]
    fn odd_dimensions_partial_tiles() {
        let p = plan(500, 300, 256, None, None).unwrap();
        assert_eq!(p.max_level, 1);
        let l1 = &p.levels[1];
        assert_eq!((l1.width, l1.height), (500, 300));
        assert_eq!((l1.tiles_x, l1.tiles_y), (2, 2));
    }
}
