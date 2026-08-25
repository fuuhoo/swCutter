//! 透明值处理三模式。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", content = "value", rename_all = "snake_case")]
pub enum AlphaMode {
    /// 保留源 Alpha
    Keep,
    /// Alpha 阈值：a < below → 全透明
    Threshold {
        below: u8,
    },
    /// 颜色键：接近指定颜色的像素置透明
    ColorKey {
        r: u8,
        g: u8,
        b: u8,
        tolerance: u8,
    },
}

impl Default for AlphaMode {
    fn default() -> Self {
        AlphaMode::Keep
    }
}

impl AlphaMode {
    pub fn apply(&self, rgba: &mut [u8]) {
        match *self {
            AlphaMode::Keep => {}
            AlphaMode::Threshold { below } => {
                for px in rgba.chunks_exact_mut(4) {
                    if px[3] < below {
                        px[3] = 0;
                    }
                }
            }
            AlphaMode::ColorKey { r, g, b, tolerance } => {
                for px in rgba.chunks_exact_mut(4) {
                    let dr = px[0].abs_diff(r);
                    let dg = px[1].abs_diff(g);
                    let db = px[2].abs_diff(b);
                    if dr <= tolerance && dg <= tolerance && db <= tolerance {
                        px[3] = 0;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keep_is_noop() {
        let mut buf = vec![1, 2, 3, 4, 250, 250, 250, 10];
        AlphaMode::Keep.apply(&mut buf);
        assert_eq!(buf, vec![1, 2, 3, 4, 250, 250, 250, 10]);
    }

    #[test]
    fn threshold_zeroes_low_alpha() {
        let mut buf = vec![9, 9, 9, 5, 7, 7, 7, 6, 1, 1, 1, 200];
        AlphaMode::Threshold { below: 6 }.apply(&mut buf);
        assert_eq!(buf[3], 0); // 5 < 6 → 0
        assert_eq!(buf[7], 6); // 6 不小于阈值 → 保持
        assert_eq!(buf[11], 200);
    }

    #[test]
    fn color_key_matches_with_tolerance() {
        // 白底图：白色→透明，容忍度 12
        let mode = AlphaMode::ColorKey { r: 255, g: 255, b: 255, tolerance: 12 };
        let mut buf = vec![
            255, 255, 255, 255, // 白 → 透明
            247, 248, 250, 255, // 容差内 → 透明
            230, 240, 255, 255, // 超容差 → 保留
            0, 0, 0, 255, // 黑 → 保留
        ];
        mode.apply(&mut buf);
        assert_eq!(&buf[0..4], &[255, 255, 255, 0]);
        assert_eq!(&buf[4..8], &[247, 248, 250, 0]);
        assert_eq!(buf[7 + 4], 255);
        assert_eq!(buf[11], 255);
    }
}
