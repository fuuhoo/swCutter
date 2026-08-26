//! 复现「点击切片闪退」：直接驱动 start_task 全链路（真实大文件 + SQLite 持久化 + worker 线程）。

use std::path::PathBuf;
use std::time::Duration;

use rust_lib_sw_cutter::api::task_api::{list_tasks, start_task, TaskConfig};
use rust_lib_sw_cutter::engine::alpha::AlphaMode;
use rust_lib_sw_cutter::engine::planner::{Resample, Scheme};

#[test]
fn probe_start_task_chain() {
    let src = match std::env::var("SWCUTTER_REAL_FILE") {
        Ok(p) if !p.is_empty() => p,
        _ => {
            eprintln!("skipped");
            return;
        }
    };
    // 隔离 APPDATA，避免污染真实历史库
    let tmp = std::env::temp_dir().join(format!("swcutter_crash_probe_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::env::set_var("APPDATA", &tmp);

    let out = PathBuf::from(&src).parent().unwrap().join("_probe_out");
    let _ = std::fs::remove_dir_all(&out);

    let cfg = TaskConfig {
        source: src,
        output: out.display().to_string(),
        tile_size: 256,
        zmin: Some(0),
        zmax: Some(2), // 小范围快速通过崩溃窗口
        scheme: Scheme::Xyz,
        alpha: AlphaMode::Keep,
        resample: Resample::Bilinear,
            skip_empty: false,
            mercator: false,
    };
    let id = start_task(cfg).expect("start_task failed");
    println!("started id={id}");

    for i in 0..80 {
        std::thread::sleep(Duration::from_millis(250));
        let tasks = list_tasks().unwrap();
        let st = tasks.iter().find(|t| t.id as u64 == id).unwrap();
        if matches!(st.status.as_str(), "done" | "error" | "cancelled") {
            println!("final status={} error={:?} tiles={}/{} elapsed={}ms",
                st.status, st.error, st.tiles_done, st.total_tiles, st.elapsed_ms);
            break;
        }
        if i % 8 == 0 {
            println!(".. status={} tiles={}/{}", st.status, st.tiles_done, st.total_tiles);
        }
    }
    let _ = std::fs::remove_dir_all(&out);
}
