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
    let png = rust_lib_sw_cutter::api::task_api::make_preview(path.clone(), 1600).unwrap();
    let ms = t0.elapsed().as_millis();
    println!("preview {} bytes in {} ms", png.len(), ms);
    assert!(png.len() > 10_000);
    assert!(ms < 15_000, "预览应秒级完成，实际 {ms}ms");
}
