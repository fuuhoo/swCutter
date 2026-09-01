//! 真实大文件冒烟测试：通过环境变量启用。
//!
//! ```powershell
//! $env:SWCUTTER_REAL_FILE='F:\tiffUpload\siwei\0608.tiff'
//! cargo test --test real_file -- --nocapture
//! ```

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rust_lib_sw_cutter::engine::alpha::AlphaMode;
use rust_lib_sw_cutter::engine::cutter::{run_cut, CutEvent, CutParams};
use rust_lib_sw_cutter::engine::planner::{Resample, Scheme};

#[test]
fn real_file_smoke() {
    let path = match std::env::var("SWCUTTER_REAL_FILE") {
        Ok(p) if !p.is_empty() => p,
        _ => {
            eprintln!("skipped: SWCUTTER_REAL_FILE not set");
            return;
        }
    };
    let out = PathBuf::from(&path)
        .parent()
        .unwrap()
        .join("_smoke_out");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();

    let params = CutParams {
        source: PathBuf::from(&path),
        output: out.clone(),
        tile_size: 256,
        zmin: None,
        zmax: None,
        scheme: Scheme::Xyz,
        alpha: AlphaMode::Keep,
        resample: Resample::Bilinear,
            skip_empty: false,
            mercator: false,
            precise: false,
            preview_overlays: None,
    };
    let events = Arc::new(Mutex::new(0usize));
    let ev2 = Arc::clone(&events);
    let sink = Arc::new(Mutex::new(move |ev: CutEvent| {
        if let CutEvent::Progress(p) = &ev {
            let mut n = ev2.lock().unwrap();
            *n += 1;
            if *n % 50 == 0 {
                println!(
                    "progress L{} {}/{} bytes={}",
                    p.level, p.tiles_done, p.total_tiles, p.bytes_written
                );
            }
        }
    }));
    let summary = run_cut(&params, sink);
    println!(
        "done={} cancelled={} errors={} tiles={} bytes={} elapsed={}ms",
        summary.errors.is_empty(),
        summary.cancelled,
        summary.errors.len(),
        summary.total_tiles,
        summary.bytes_written,
        summary.elapsed_ms
    );
    for l in &summary.levels {
        println!("  L{:>2}: {}x{} ({} tiles)", l.level, l.width, l.height, l.tiles);
    }
    assert!(summary.errors.is_empty(), "{:?}", summary.errors);
    assert!(!summary.cancelled);
    assert!(summary.total_tiles > 0);
    assert!(out.join("manifest.json").exists());
    assert!(out.join("preview.html").exists());
    // 不清理，便于人工查看输出目录
}

/// 续切：不清输出目录，直接重跑同一任务 → 应秒级完成（全部跳过）。
#[test]
fn real_file_resume() {
    let path = match std::env::var("SWCUTTER_REAL_FILE") {
        Ok(p) if !p.is_empty() => p,
        _ => {
            eprintln!("skipped: SWCUTTER_REAL_FILE not set");
            return;
        }
    };
    let out = PathBuf::from(&path).parent().unwrap().join("_smoke_out");
    if !out.join("manifest.json").exists() {
        eprintln!("skipped: no prior output (run real_file_smoke first)");
        return;
    }
    let params = CutParams {
        source: PathBuf::from(&path),
        output: out.clone(),
        tile_size: 256,
        zmin: None,
        zmax: None,
        scheme: Scheme::Xyz,
        alpha: AlphaMode::Keep,
        resample: Resample::Bilinear,
            skip_empty: false,
            mercator: false,
            precise: false,
            preview_overlays: None,
    };
    let sink = Arc::new(Mutex::new(|_: CutEvent| {}));
    let summary = run_cut(&params, sink);
    println!(
        "resume done tiles={} bytes={} ms={} errors={}",
        summary.total_tiles,
        summary.bytes_written,
        summary.elapsed_ms,
        summary.errors.len()
    );
    assert!(summary.errors.is_empty(), "{:?}", summary.errors);
    // 断点续切命中全部瓦片：应在数秒内完成
    assert!(
        summary.elapsed_ms < 30_000,
        "续切应远快于全量切片，实际 {}ms",
        summary.elapsed_ms
    );
}

/// 预览生成性能：大文件应在数秒内完成（单遍 chunk 采样）。
#[test]
fn real_file_preview_perf() {
    let path = match std::env::var("SWCUTTER_REAL_FILE") {
        Ok(p) if !p.is_empty() => p,
        _ => {
            eprintln!("skipped: SWCUTTER_REAL_FILE not set");
            return;
        }
    };
    let t0 = std::time::Instant::now();
    let png = rust_lib_sw_cutter::api::task_api::make_preview(
        path.clone(),
        1600,
        rust_lib_sw_cutter::engine::alpha::AlphaMode::Keep,
    )
    .unwrap();
    let ms = t0.elapsed().as_millis();
    println!("preview {} bytes in {} ms", png.len(), ms);
    assert!(png.len() > 10_000);
    // 单次全新进程调用含一次整图解码（LZW 并行解码 ≈8-18s，超大 tile 文件可达 ~25s）；
    // 应用内粗图+精图共享解码缓存，合计 ≈ 一次解码，目标 30s 内
    assert!(ms < 30_000, "预览应在 30s 内完成（一次整图解码），实际 {ms}ms");
}

/// Mercator 切片：验证 overview 修复后低缩放级别有内容。
#[test]
fn real_file_mercator() {
    let path = match std::env::var("SWCUTTER_REAL_FILE") {
        Ok(p) if !p.is_empty() => p,
        _ => {
            eprintln!("skipped: SWCUTTER_REAL_FILE not set");
            return;
        }
    };
    let out = PathBuf::from(&path)
        .parent()
        .unwrap()
        .join("tile_0");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();

    let params = CutParams {
        source: PathBuf::from(&path),
        output: out.clone(),
        tile_size: 256,
        zmin: None,
        zmax: None,
        scheme: Scheme::Xyz,
        alpha: AlphaMode::Keep,
        resample: Resample::Bilinear,
        skip_empty: false,
        mercator: true,
        precise: true,
        preview_overlays: None,
    };
    let sink = Arc::new(Mutex::new(|_: CutEvent| {}));
    let summary = run_cut(&params, sink);
    println!(
        "mercator done tiles={} bytes={} ms={} errors={}",
        summary.total_tiles,
        summary.bytes_written,
        summary.elapsed_ms,
        summary.errors.len()
    );
    for l in &summary.levels {
        println!("  L{:>2}: {}x{} ({} tiles)", l.level, l.width, l.height, l.tiles);
    }
    assert!(summary.errors.is_empty(), "{:?}", summary.errors);
    assert!(!summary.cancelled);
    assert!(summary.total_tiles > 0);
    assert!(out.join("manifest.json").exists());
    assert!(out.join("preview.html").exists());
}

/// Mercator 概览级内容保全回归：ColorKey 白底 + skip_empty 时，
/// 深概览级（如 z13，native≈17-18）不得丢失内容。
/// 旧算法：alpha Nearest 相位采样（奇数相位内容整条消失）+ 直通 RGB 稀释（被键出背景污染），
/// 逐级 ColorKey 再误杀 → 深概览级大片内容变透明（显示不全）。
/// 修复：概览 2×2 预乘盒式平均 + 逐级屏障（子瓦片落盘后才合成父级）。
#[test]
fn real_file_mercator_overview_alpha() {
    let path = match std::env::var("SWCUTTER_REAL_FILE") {
        Ok(p) if !p.is_empty() => p,
        _ => {
            eprintln!("skipped: SWCUTTER_REAL_FILE not set");
            return;
        }
    };
    let out = std::env::temp_dir().join("swcutter_real_file_ck_out");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();

    let params = CutParams {
        source: PathBuf::from(&path),
        output: out.clone(),
        tile_size: 256,
        zmin: Some(13),
        zmax: None, // 到 native 级（基础级），z13 处于概览链深处
        scheme: Scheme::Xyz,
        alpha: AlphaMode::ColorKey { r: 255, g: 255, b: 255, tolerance: 10 },
        resample: Resample::Bilinear,
        skip_empty: true,
        mercator: true,
        precise: true,
        preview_overlays: None,
    };
    let sink = Arc::new(Mutex::new(|_: CutEvent| {}));
    let summary = run_cut(&params, sink);
    println!(
        "mercator-ck done tiles={} bytes={} ms={} errors={}",
        summary.total_tiles,
        summary.bytes_written,
        summary.elapsed_ms,
        summary.errors.len()
    );
    assert!(summary.errors.is_empty(), "{:?}", summary.errors);
    assert!(!summary.cancelled);

    // z13 概览级：统计不透明像素（键出背景后内容应保留）
    let mut opaque_px = 0u64;
    let mut solid_px = 0u64;
    let mut tiles_with_content = 0u32;
    let mut tile_files = 0u32;
    let z13 = out.join("13");
    if let Ok(rd) = std::fs::read_dir(&z13) {
        for x in rd.flatten() {
            if !x.path().is_dir() {
                continue;
            }
            for y in std::fs::read_dir(x.path()).into_iter().flatten().flatten() {
                tile_files += 1;
                if let Ok(img) = image::open(y.path()) {
                    let rgba = img.to_rgba8();
                    let op = rgba.pixels().filter(|p| p[3] > 0).count() as u64;
                    let solid = rgba.pixels().filter(|p| p[3] == 255).count() as u64;
                    if op > 0 {
                        tiles_with_content += 1;
                    }
                    opaque_px += op;
                    solid_px += solid;
                }
            }
        }
    }
    println!(
        "z13: tile_files={} tiles_with_content={} opaque_px={} solid_px={}",
        tile_files, tiles_with_content, opaque_px, solid_px
    );
    assert!(
        opaque_px > 2000,
        "z13 概览级不应丢失内容（opaque_px={opaque_px}，旧算法该值接近 0）"
    );
    assert!(tiles_with_content >= 5, "z13 至少多数瓦片应含内容");
}
