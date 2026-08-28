use rust_lib_sw_cutter::engine::source::SourceReader;
use rust_lib_sw_cutter::engine::meta;

#[test]
fn check_source_read() {
    let src = match std::env::var("SWCUTTER_MERC_FILE") {
        Ok(p) if !p.is_empty() => p,
        _ => { eprintln!("skipped"); return; }
    };
    let path = std::path::Path::new(&src);
    let info = meta::probe(path).unwrap();
    let georef = meta::probe_georef(path).unwrap().expect("no georef");
    let mut reader = SourceReader::open(path).unwrap();
    println!("source: {}x{} color={:?}", info.width, info.height, info.pixel_format);
    println!("georef: mx0={} my_top={} sx={} sy={}", georef.mx0, georef.my_top, georef.sx, georef.sy);

    // Read center of image
    let cx = info.width / 2;
    let cy = info.height / 2;
    let buf = reader.read_rect(cx as i64 - 2, cy as i64 - 2, 5, 5).unwrap();
    println!("Center ({}x{}): {:?}", cx, cy, &buf[..20]);

    // Check: sum of first 25 pixels (5x5 * RGBA)
    let sum: u64 = buf.chunks(4).map(|p| p[0] as u64 + p[1] as u64 + p[2] as u64).sum();
    println!("Center 5x5 RGB sum: {sum}");

    // Read top-left
    let buf2 = reader.read_rect(0, 0, 5, 5).unwrap();
    let sum2: u64 = buf2.chunks(4).map(|p| p[0] as u64 + p[1] as u64 + p[2] as u64).sum();
    println!("TopLeft 5x5 RGB sum: {sum2}");

    // Read the region that z18 tile (205978, 111991) would map to
    use rust_lib_sw_cutter::engine::mercator;
    let z = 18u32;
    let tx = 205978u32;
    let ty = 111991u32;
    let t = 256.0f64;
    let res = mercator::INIT_RESOLUTION / 2f64.powi(z as i32);
    let wx_l = tx as f64 * t * res - mercator::ORIGIN_SHIFT;
    let wx_r = (tx + 1) as f64 * t * res - mercator::ORIGIN_SHIFT;
    let wy_top = mercator::ORIGIN_SHIFT - ty as f64 * t * res;
    let wy_bot = mercator::ORIGIN_SHIFT - (ty + 1) as f64 * t * res;
    println!("Z18 tile ({tx},{ty}): wx=[{wx_l:.1},{wx_r:.1}] wy=[{wy_top:.1},{wy_bot:.1}]");

    let fx0 = (wx_l - georef.mx0) / georef.sx;
    let fx1 = (wx_r - georef.mx0) / georef.sx;
    let fy_top = (wy_top - georef.my_top) / georef.sy;
    let fy_bot = (wy_bot - georef.my_top) / georef.sy;
    println!("Source coords: fx=[{fx0:.1},{fx1:.1}] fy=[{fy_top:.1},{fy_bot:.1}]");

    let rx = fx0.min(fx1).floor() as i64;
    let rw = ((fx0.max(fx1)).ceil() as i64 - rx).max(1) as u32;
    let ry = fy_top.min(fy_bot).floor() as i64;
    let rh = ((fy_top.max(fy_bot)).ceil() as i64 - ry).max(1) as u32;
    println!("Read rect: rx={rx} ry={ry} rw={rw} rh={rh}");

    if rx >= 0 && ry >= 0 && (rx + rw as i64) <= info.width as i64 && (ry + rh as i64) <= info.height as i64 {
        let buf3 = reader.read_rect(rx, ry, rw, rh).unwrap();
        let total_pixels = (rw as usize) * (rh as usize);
        let sum3: u64 = buf3.chunks(4).map(|p| p[0] as u64 + p[1] as u64 + p[2] as u64).sum();
        let alpha_sum: u64 = buf3.chunks(4).map(|p| p[3] as u64).sum();
        let nonzero_rgb = buf3.chunks(4).filter(|p| p[0] > 0 || p[1] > 0 || p[2] > 0).count();
        println!("Z18 region: {rw}x{rh}={total_pixels}px, rgb_sum={sum3}, alpha_sum={alpha_sum}, nonzero_rgb={nonzero_rgb}");
        if total_pixels > 0 {
            println!("  avg_rgb_per_px: {:.1}", sum3 as f64 / total_pixels as f64);
            println!("  avg_alpha_per_px: {:.1}", alpha_sum as f64 / total_pixels as f64);
        }
        // Show first few pixels
        for i in 0..5.min(total_pixels) {
            let off = i * 4;
            println!("  pixel[{i}] = {:?}", &buf3[off..off+4]);
        }
    } else {
        println!("Z18 region is partially outside source bounds!");
        // Still try to read the valid portion
        let ix0 = rx.clamp(0, info.width as i64);
        let iy0 = ry.clamp(0, info.height as i64);
        let ix1 = (rx + rw as i64).clamp(0, info.width as i64);
        let iy1 = (ry + rh as i64).clamp(0, info.height as i64);
        let iw = (ix1 - ix0).max(0) as u32;
        let ih = (iy1 - iy0).max(0) as u32;
        println!("  clamped read: ({ix0},{iy0}) {iw}x{ih}");
        if iw > 0 && ih > 0 {
            let buf3 = reader.read_rect(ix0, iy0, iw, ih).unwrap();
            let total_pixels = (iw as usize) * (ih as usize);
            let sum3: u64 = buf3.chunks(4).map(|p| p[0] as u64 + p[1] as u64 + p[2] as u64).sum();
            let nonzero_rgb = buf3.chunks(4).filter(|p| p[0] > 0 || p[1] > 0 || p[2] > 0).count();
            println!("  clamped: rgb_sum={sum3}, nonzero_rgb={nonzero_rgb}/{total_pixels}");
        }
    }

    // Now scan the source to find where actual content starts
    println!("\n--- Source content scan ---");
    for y_frac in [0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0] {
        let y = (y_frac * (info.height - 1) as f64) as i64;
        let buf = reader.read_rect(0, y, info.width, 1).unwrap();
        let first_nonzero_x = buf.chunks(4).position(|p| p[0] > 0 || p[1] > 0 || p[2] > 0);
        let last_nonzero_x = buf.chunks(4).rposition(|p| p[0] > 0 || p[1] > 0 || p[2] > 0);
        let count: usize = buf.chunks(4).filter(|p| p[0] > 0 || p[1] > 0 || p[2] > 0).count();
        println!("  y={y} ({y_frac:.1}): first_x={:?} last_x={:?} count={}/{}", first_nonzero_x, last_nonzero_x, count, info.width);
    }
}
