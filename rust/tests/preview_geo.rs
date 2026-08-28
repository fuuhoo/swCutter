//! 回归：跨 make_preview 调用不得残留旧 Geo 参数（同一文件 320→1400 两连调都必须完整渲染）。
//! 历史 bug：PREVIEW_DECODER 线程本地缓存了调用级 Geo，第二次调用沿用旧 scale/ow/oh
//! 采样错位 → 精图"顶部乱码长条 + 下方空白"。
//! 用法: $env:SWCUTTER_PREVIEW_FILES='F:\a.tif'; cargo test --test preview_geo -- --nocapture

use rust_lib_sw_cutter::engine::source::preview_sample_parallel;

fn assert_full(path: &str, max_px: u32) {
    let (ow, oh, canvas) = preview_sample_parallel(std::path::Path::new(path), max_px).unwrap();
    // 期望：非零像素 ≈ 全画布（源图整幅有内容），且最后内容行接近底部
    let mut total = 0u64;
    let mut last = 0usize;
    for y in 0..oh {
        let mut row = 0u32;
        for x in 0..ow {
            let i = ((y * ow + x) * 4) as usize;
            if i + 3 < canvas.len() && (canvas[i] > 0 || canvas[i + 1] > 0 || canvas[i + 2] > 0) {
                row += 1;
            }
        }
        if row > 0 {
            last = y as usize;
            total += row as u64;
        }
    }
    println!(
        "  max_px={max_px}: {ow}x{oh} nonzero={total} last_row={last}/{}",
        oh - 1
    );
    assert!(
        last >= (oh * 9 / 10) as usize,
        "max_px={max_px} 预览应渲染到底部（last_row={last}/{})，疑似旧 Geo 采样错位",
        oh - 1
    );
    assert!(
        total > (ow as u64 * oh as u64) / 2,
        "max_px={max_px} 内容过少（nonzero={total}），疑似旧 Geo 采样错位"
    );
}

#[test]
fn preview_no_stale_geo_across_calls() {
    let list = std::env::var("SWCUTTER_PREVIEW_FILES").unwrap_or_default();
    if list.is_empty() {
        eprintln!("skipped");
        return;
    }
    for path in list.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        println!("== {path}");
        // 应用实际顺序：粗图 → 精图（同一文件，分辨率不同）
        assert_full(path, 320);
        assert_full(path, 1400);
        // 再次精图（缓存命中路径）
        assert_full(path, 1400);
    }
}

/// 预览必须应用透明模式：源黑边（若有）在 ColorKey 黑下变透明；非黑角点不受影响。
#[test]
fn preview_applies_alpha_mode() {
    let list = std::env::var("SWCUTTER_PREVIEW_FILES").unwrap_or_default();
    if list.is_empty() {
        eprintln!("skipped");
        return;
    }
    for path in list.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let keep = rust_lib_sw_cutter::api::task_api::make_preview(
            path.to_string(),
            320,
            rust_lib_sw_cutter::engine::alpha::AlphaMode::Keep,
        )
        .unwrap();
        let ck = rust_lib_sw_cutter::api::task_api::make_preview(
            path.to_string(),
            320,
            rust_lib_sw_cutter::engine::alpha::AlphaMode::ColorKey {
                r: 0,
                g: 0,
                b: 0,
                tolerance: 10,
            },
        )
        .unwrap();
        let img_keep = image::load_from_memory(&keep).unwrap().to_rgba8();
        let img_ck = image::load_from_memory(&ck).unwrap().to_rgba8();
        let (w, h) = (img_keep.width(), img_keep.height());
        let corners = [(0u32, 0u32), (w - 1, 0), (0, h - 1), (w - 1, h - 1)];
        let mut black_became_transparent = 0usize;
        let mut black_total = 0usize;
        for (x, y) in corners {
            let a = img_keep.get_pixel(x.min(w - 1), y.min(h - 1));
            let b = img_ck.get_pixel(x.min(w - 1), y.min(h - 1));
            let is_black = a[0] <= 10 && a[1] <= 10 && a[2] <= 10;
            if is_black {
                black_total += 1;
                assert_eq!(b[3], 0, "黑色角点应在 ColorKey 黑下透明: {path} corner ({x},{y})");
                black_became_transparent += 1;
            }
        }
        println!(
            "  {path} ColorKey black: {black_became_transparent}/{black_total} 黑色角点已透明",
        );
    }
}
