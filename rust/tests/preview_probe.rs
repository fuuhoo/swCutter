//! 阶段耗时探针：全图解码 / 采样 / PNG 编码 分别计时。
//! 用法: $env:SWCUTTER_PREVIEW_FILES='F:\a.tif'; cargo test --test preview_probe -- --nocapture

use std::time::Instant;

use image::ImageEncoder as _;

#[test]
fn preview_phase_probe() {
    let list = std::env::var("SWCUTTER_PREVIEW_FILES").unwrap_or_default();
    if list.is_empty() {
        eprintln!("skipped");
        return;
    }
    for p in list.split(',') {
        let path = p.trim().to_string();
        if path.is_empty() {
            continue;
        }
        println!("== {path}");
        // 顺序全量解码耗时（read_full_cancellable）
        {
            let mut r = rust_lib_sw_cutter::engine::source::SourceReader::open(std::path::Path::new(&path))
                .unwrap();
            let t0 = Instant::now();
            let buf = r.read_full_cancellable(None).unwrap();
            println!(
                "  sequential full decode: {} bytes in {} ms",
                buf.len(),
                t0.elapsed().as_millis()
            );
        }
        // 应用式两段调用（粗图+精图）总耗时
        let t0 = Instant::now();
        for max_px in [320u32, 1400] {
            let t1 = Instant::now();
            let png = rust_lib_sw_cutter::api::task_api::make_preview(
                path.clone(),
                max_px,
                rust_lib_sw_cutter::engine::alpha::AlphaMode::Keep,
            )
            .unwrap();
            let out = std::env::temp_dir().join(format!(
                "preview_dump_{}_{}.png",
                std::path::Path::new(&path)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default(),
                max_px
            ));
            std::fs::write(&out, &png).unwrap();
            println!(
                "  make_preview(max_px={max_px}): {} bytes in {} ms (cumulative {} ms) -> {}",
                png.len(),
                t1.elapsed().as_millis(),
                t0.elapsed().as_millis(),
                out.display()
            );
        }
        println!("  TOTAL two-call preview: {} ms", t0.elapsed().as_millis());
    }
}
