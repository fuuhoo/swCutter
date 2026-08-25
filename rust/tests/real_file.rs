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
