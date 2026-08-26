//! 对比验证：真实 GeoTIFF 的 mercator 计划 vs gdal2tiles_sw 参考输出。
//! 用法：SWCUTTER_MERC_FILE=F:/path/to.tif cargo test --test merc_probe -- --nocapture

use rust_lib_sw_cutter::engine::mercator;
use rust_lib_sw_cutter::engine::meta;

#[test]
fn probe_merc_plan() {
    // 模式 B：直接给定 3857 范围与 GSD（用于和参考输出逐级对账）
    if let Ok(spec) = std::env::var("SWCUTTER_MERC_BOUNDS") {
        // "minx,miny,maxx,maxy[,sx_m[,zmin,zmax]]"
        let v: Vec<f64> = spec.split(',').map(|s| s.trim().parse().unwrap()).collect();
        let b = [v[0], v[1], v[2], v[3]];
        let sx_m = if v.len() > 4 { v[4] } else { 0.2985821417 };
        let zmin = if v.len() > 5 { v[5] as u32 } else { 1 };
        let zmax = if v.len() > 6 { v[6] as u32 } else { 19 };
        let p = mercator::plan(b, sx_m, 256, Some(zmin), Some(zmax), u64::MAX).unwrap();
        println!("native_zoom = {}", p.native_zoom);
        let mut total = 0u64;
        for lv in &p.levels {
            let c = lv.count();
            total += c;
            println!(
                "Z{:<2} : {:>7}  x:{}..{} y:{}..{}",
                lv.z, c, lv.tx0, lv.tx1, lv.ty0, lv.ty1
            );
        }
        println!("TOTAL = {total}");
        return;
    }
    let src = match std::env::var("SWCUTTER_MERC_FILE") {
        Ok(p) if !p.is_empty() => p,
        _ => {
            eprintln!("skipped");
            return;
        }
    };
    let info = meta::probe(std::path::Path::new(&src)).unwrap();
    let g = meta::probe_georef(std::path::Path::new(&src))
        .unwrap()
        .expect("无地理参考");
    println!(
        "src {src}\n{}x{} sx={} sy={} degrees={} origin=({},{})",
        info.width, info.height, g.sx, g.sy, g.degrees, g.mx0, g.my_top
    );
    let b = g.bounds3857(info.width, info.height);
    println!("bounds3857 = {b:?}");
    let sx_m = (b[2] - b[0]) / info.width as f64;
    let p = mercator::plan(b, sx_m, 256, Some(1), Some(19), u64::MAX).unwrap();
    println!("native_zoom = {}", p.native_zoom);
    let mut total = 0u64;
    for lv in &p.levels {
        let c = lv.count();
        total += c;
        println!("Z{:>2} : {:>7}", lv.z, c);
    }
    println!("TOTAL(1..={}) = {}", p.levels.last().map(|l| l.z).unwrap_or(0), total);
}
