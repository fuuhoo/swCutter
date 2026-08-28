//! 源图黑边检测：扫描四周边界与整体统计，判断黑边是否来自数据。
//! 用法: $env:SWCUTTER_PREVIEW_FILES='F:\a.tif'; cargo test --test edge_probe -- --nocapture

use rust_lib_sw_cutter::engine::source::SourceReader;

#[test]
fn edge_probe() {
    let list = std::env::var("SWCUTTER_PREVIEW_FILES").unwrap_or_default();
    if list.is_empty() {
        eprintln!("skipped");
        return;
    }
    for p in list.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let mut r = SourceReader::open(std::path::Path::new(p)).unwrap();
        let (w, h) = (r.width, r.height);
        println!("== {p} ({w}x{h})");
        // 统计整幅中接近纯黑(<=8)像素占比
        let step = 4u32;
        let mut black = 0u64;
        let mut total = 0u64;
        for y in (0..h).step_by(step as usize) {
            let buf = r.read_rect(0, y as i64, w, 1).unwrap();
            for x in 0..w {
                let i = (x as usize) * 4;
                if buf[i] <= 8 && buf[i + 1] <= 8 && buf[i + 2] <= 8 {
                    black += 1;
                }
                total += 1;
            }
        }
        println!("  black-ish(<=8) ratio: {:.2}% ({black}/{total})", black as f64 * 100.0 / total as f64);
        // 四边 8px 边界带统计
        let band = 8u32;
        for (label, x0, y0, bw, bh) in [
            ("top", 0u32, 0u32, w, band),
            ("bottom", 0u32, h - band, w, band),
            ("left", 0u32, 0u32, band, h),
            ("right", w - band, 0u32, band, h),
        ] {
            let mut black = 0u64;
            let mut tot = 0u64;
            for y in 0..bh {
                let buf = r.read_rect(x0 as i64, (y0 + y) as i64, bw, 1).unwrap();
                for x in 0..bw {
                    let i = (x as usize) * 4;
                    if buf[i] <= 8 && buf[i + 1] <= 8 && buf[i + 2] <= 8 {
                        black += 1;
                    }
                    tot += 1;
                }
            }
            println!(
                "  {label} band({x0},{y0},{bw}x{bh}): black {:.2}%",
                black as f64 * 100.0 / tot as f64
            );
        }
        // 找内容边界（首个非黑行/列）
        let mut first_content_y = h;
        for y in 0..h {
            let buf = r.read_rect(0, y as i64, w, 1).unwrap();
            if buf.chunks(4).any(|p| p[0] > 8 || p[1] > 8 || p[2] > 8) {
                first_content_y = y;
                break;
            }
        }
        let mut last_content_y = 0u32;
        for y in (0..h).rev() {
            let buf = r.read_rect(0, y as i64, w, 1).unwrap();
            if buf.chunks(4).any(|p| p[0] > 8 || p[1] > 8 || p[2] > 8) {
                last_content_y = y;
                break;
            }
        }
        println!(
            "  first_content_row={first_content_y} last_content_row={last_content_y}",
        );
    }
}
