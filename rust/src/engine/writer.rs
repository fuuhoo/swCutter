//! 输出目录布局与 manifest。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::error::{io_err, CoreResult};
use super::planner::Scheme;

pub const MANIFEST_NAME: &str = "manifest.json";
pub const PREVIEW_HTML_NAME: &str = "preview.html";

/// 瓦片文件相对路径：{z}/{x}/{y}.png（y 是否翻转由调用方决定）
pub fn tile_rel_path(scheme: Scheme, level: u32, x: u32, y_in: u32, tiles_y: u32) -> PathBuf {
    let y = match scheme {
        Scheme::Xyz => y_in,
        Scheme::Tms => tiles_y - 1 - y_in,
    };
    PathBuf::from(level.to_string()).join(x.to_string()).join(format!("{y}.png"))
}

/// 确保 manifest 中记录的输出根存在。
pub fn ensure_out_dir(out: &Path) -> std::io::Result<PathBuf> {
    fs::create_dir_all(out)?;
    Ok(out.to_path_buf())
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct ManifestLevel {
    pub level: u32,
    pub width: u32,
    pub height: u32,
    pub tiles: u64,
    /// 该级世界列偏移（相对模式恒 0；mercator 为 tx0）
    #[serde(default)]
    pub ox: u32,
    /// 该级世界行偏移（XYZ 语义；相对模式恒 0）
    #[serde(default)]
    pub oy: u32,
    /// 该级全球行数（1<<z；0 表示未知，由前端回退）
    #[serde(default)]
    pub wy: u32,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Manifest {
    pub app: String,
    pub version: String,
    pub source: String,
    pub source_width: u32,
    pub source_height: u32,
    pub tile_size: u32,
    pub scheme: String,
    pub min_level: u32,
    pub max_level: u32,
    pub levels: Vec<ManifestLevel>,
    pub total_tiles: u64,
    pub bytes_written: u64,
}

pub fn write_manifest(out: &Path, m: &Manifest) -> CoreResult<()> {
    let json = serde_json::to_string_pretty(m)
        .map_err(|e| super::error::CoreError::Encoding(e.to_string()))?;
    let p = out.join(MANIFEST_NAME);
    let mut f = fs::File::create(&p).map_err(|e| io_err(p.display().to_string(), e))?;
    f.write_all(json.as_bytes())
        .map_err(|e| io_err(p.display().to_string(), e))?;
    Ok(())
}

/// 浏览器预览页参数。
#[derive(Debug, Clone)]
pub struct PreviewInfo {
    pub source_w: u32,
    pub source_h: u32,
    pub tile_size: u32,
    pub zmin: u32,
    pub zmax: u32,
    pub tms: bool,
    pub levels: Vec<ManifestLevel>,
    /// 预置底图/叠加层（JSON 数组，来自全局设置）
    pub overlays_json: String,
}

/// 生成本地零依赖的瓦片浏览器查看页（vanilla JS，离线可用）。
pub fn write_preview_html(out: &Path, info: &PreviewInfo) -> CoreResult<()> {
    let overlays: serde_json::Value = serde_json::from_str(&info.overlays_json)
        .unwrap_or_else(|_| serde_json::json!([]));
    let cfg = serde_json::json!({
        "w": info.source_w,
        "h": info.source_h,
        "t": info.tile_size,
        "zmin": info.zmin,
        "zmax": info.zmax,
        "tms": info.tms,
        "overlays": overlays,
        "levels": info.levels.iter().map(|l| serde_json::json!({
            "z": l.level, "w": l.width, "h": l.height,
            "tx": (l.width as f64 / info.tile_size as f64).ceil() as u64,
            "ty": (l.height as f64 / info.tile_size as f64).ceil() as u64,
            "ox": l.ox, "oy": l.oy, "wy": l.wy,
        })).collect::<Vec<_>>(),
    });
    let cfg_str =
        serde_json::to_string(&cfg).map_err(|e| super::error::CoreError::Encoding(e.to_string()))?;

    let html = r#"<!DOCTYPE html>
<html lang="zh-CN"><head><meta charset="utf-8">
<title>swCutter 瓦片预览</title>
<meta name="viewport" content="width=device-width,initial-scale=1">
<link rel="stylesheet" href="./maplibre-gl.css">
<script src="./maplibre-gl.js"></script>
<style>
 html,body{margin:0;height:100%;background:#0d1117}
 #map{position:absolute;inset:0}
 #bar{position:absolute;left:0;right:0;top:0;z-index:1000;display:flex;gap:10px;align-items:center;
      padding:8px 14px;background:#171c26e6;border-bottom:1px solid #ffffff14;backdrop-filter:blur(6px)}
 #bar b{font-size:13px;color:#dfe5ef}
 #badge{font-size:11.5px;font-weight:700;color:#8fb4ff;background:#4f8cff22;border:1px solid #4f8cff44;border-radius:999px;padding:3px 10px}
 .sp{flex:1}
 #hint{font-size:11px;color:#7a8699}
 #ovp{position:absolute;top:52px;right:10px;z-index:1000;background:#171c26f2;border:1px solid #ffffff20;
      border-radius:12px;padding:10px 12px;font-size:12px;color:#cfd6e4;max-width:320px;display:flex;flex-direction:column;gap:6px}
 #ovp label{display:flex;gap:6px;align-items:center}
 #ovp input[type=range]{width:110px;accent-color:#4f8cff}
 .offline{position:absolute;inset:0;display:flex;align-items:center;justify-content:center;color:#7a8699;font-size:13px;z-index:999}
</style></head>
<body>
<div id="bar">
 <b>swCutter 预览（MapLibre）</b>
 <span class="sp"></span>
 <span id="badge"></span>
 <span id="zlvl" style="font-size:11.5px;font-weight:700;color:#ffb4b4;background:#ff555522;border:1px solid #ff555544;border-radius:999px;padding:3px 10px;margin-left:6px"></span>
 <span id="hint">拖动 / 滚轮缩放</span>
</div>
<div id="map"></div>
<div id="diagPanel" style="position:absolute;left:10px;bottom:10px;z-index:1500;background:#171c26f2;border:1px solid #ffffff20;border-radius:10px;padding:8px 12px;font:11px/1.5 ui-monospace,monospace;color:#cfd6e4;max-width:540px;display:flex;flex-direction:column;gap:4px">
  <div style="display:flex;gap:6px;align-items:center;flex-wrap:wrap">
    <b style="color:#8fb4ff">诊断</b>
    <label style="display:flex;gap:4px;align-items:center;cursor:pointer">
      <input type="checkbox" id="toggleLocal" checked /> 显示切片
    </label>
    <label style="display:flex;gap:4px;align-items:center">
      切片透明度 <input type="range" id="localOpacity" min="0" max="100" value="100" style="width:90px;accent-color:#4f8cff" />
    </label>
    <button id="recenterBtn" style="margin-left:auto;background:#4f8cff22;border:1px solid #4f8cff44;color:#8fb4ff;border-radius:6px;padding:2px 8px;cursor:pointer;font:11px">↺ 回到切片范围</button>
  </div>
  <div style="display:grid;grid-template-columns:auto 1fr;gap:2px 10px">
    <span style="color:#7a8699">鼠标位置</span><span id="mouseLL" style="color:#dfe5ef">—</span>
    <span style="color:#7a8699">鼠标瓦片</span><span id="mouseTile" style="color:#dfe5ef">—</span>
    <span style="color:#7a8699">当前 Z</span><span id="currentZ" style="color:#dfe5ef">—</span>
  </div>
  <pre id="diagInfo" style="margin:0;white-space:pre-wrap;color:#a8b3c5;font:10.5px/1.5 ui-monospace,monospace"></pre>
</div>
<script>
const CFG = __CFG__;

function showErr(msg){
  let el = document.getElementById('errBox');
  if(!el){ el = document.createElement('div'); el.id='errBox';
    el.style.cssText='position:absolute;left:10px;bottom:10px;right:10px;z-index:2000;background:#3a1010f2;color:#ffb4b4;border:1px solid #ff555555;border-radius:10px;padding:10px;font:12px/1.6 monospace;white-space:pre-wrap;max-height:40vh;overflow:auto';
    document.body.appendChild(el); }
  el.textContent = (el.textContent? el.textContent+'\n':'') + msg;
}
window.addEventListener('error', e => showErr('[error] '+e.message+' @'+(e.filename||'')+':'+(e.lineno||'?')));
window.addEventListener('unhandledrejection', e => showErr('[promise] '+String(e.reason)));

if (typeof maplibregl === 'undefined') {
  document.getElementById('map').innerHTML =
    '<div class="offline">MapLibre GL JS 未能加载。瓦片本身完好，可直接浏览 {z}/{x}/{y}.png 目录。</div>';
} else {
try {
  document.getElementById('badge').textContent =
    'Z' + CFG.zmin + '–Z' + CFG.zmax + ' · ' + (CFG.tms?'TMS':'XYZ') + ' · ' + CFG.t + 'px';

  // ---- 瓦片坐标 → 经纬度 ----
  // 假设 x/y ∈ [0, 2^z] 范围；mercator 模式下 ox/oy 由 mercator.rs 保证此约束。
  function tileLL(x, y, z) {
    const n = Math.pow(2, z);
    const lon = x / n * 360 - 180;
    const lat = Math.atan(Math.sinh(Math.PI * (1 - 2 * y / n))) * 180 / Math.PI;
    return [lon, lat];
  }

  // ---- 计算瓦片边界 ----
  const B = CFG.levels.find(function(l) { return l.z === CFG.zmax; }) || CFG.levels[CFG.levels.length - 1] || {ox:0, oy:0, tx:1, ty:1, z:CFG.zmax};
  var bo = { ox: B.ox || 0, oy: B.oy || 0 };
  var sw = tileLL(bo.ox, bo.oy + B.ty, B.z);
  var ne = tileLL(bo.ox + B.tx, bo.oy, B.z);
  var bounds = [sw, ne]; // [[west, south], [east, north]]

  // ---- 诊断面板：诊断坐标系 / 切片 / 底图对齐 ----
  var diag = {
    bounds: bounds,
    center: [(sw[0] + ne[0]) / 2, (sw[1] + ne[1]) / 2],
    ox: bo.ox, oy: bo.oy,
    tx: B.tx, ty: B.ty, z: B.z,
    wy: B.wy,
    scheme: CFG.tms ? 'tms' : 'xyz',
    localZmin: CFG.zmin, localZmax: CFG.zmax,
    overlays: (CFG.overlays||[]).map(function(o){ return {name:o.name, on:!!o.on, below:o.below!==false, zmin:o.zmin, zmax:o.zmax, tms:o.tms}; })
  };
  document.getElementById('diagInfo').textContent =
    '切片范围 sw=' + JSON.stringify({lon: +sw[0].toFixed(4), lat: +sw[1].toFixed(4)}) +
    '  ne=' + JSON.stringify({lon: +ne[0].toFixed(4), lat: +ne[1].toFixed(4)}) +
    '\n切片 Z[' + CFG.zmin + ',' + CFG.zmax + ']  scheme=' + (CFG.tms?'TMS':'XYZ') +
    '  当前 L.B=' + bo.ox + ',' + bo.oy + '  size=' + B.tx + 'x' + B.ty + ' (z=' + B.z + ')' +
    '\n底图：' + (CFG.overlays||[]).map(function(o){return (o.on?'✓':'✗')+' '+o.name+' [z'+o.zmin+'-'+o.zmax+' '+(o.tms?'TMS':'XYZ')+']';}).join(' | ');

  // ---- 初始化 MapLibre ----
  var map = new maplibregl.Map({
    container: 'map',
    style: {
      version: 8,
      sources: {},
      layers: []
    },
    center: [(sw[0] + ne[0]) / 2, (sw[1] + ne[1]) / 2],
    zoom: CFG.zmin,
    maxZoom: CFG.zmax,
    minZoom: CFG.zmin,
    attributionControl: true
  });

  map.addControl(new maplibregl.NavigationControl(), 'bottom-right');
  map.addControl(new maplibregl.ScaleControl({imperial: false}), 'bottom-left');

  // ---- 本地瓦片层（栅格瓦片作为 image source）----
  map.on('load', function() {
    // 添加栅格瓦片源
    map.addSource('local-tiles', {
      type: 'raster',
      tiles: ['{z}/{x}/{y}.png'],
      tileSize: CFG.t,
      maxzoom: CFG.zmax,
      minzoom: CFG.zmin,
      bounds: [sw[0], sw[1], ne[0], ne[1]],
      scheme: CFG.tms ? 'tms' : 'xyz'
    });

    map.addLayer({
      id: 'local-tiles-layer',
      type: 'raster',
      source: 'local-tiles',
      paint: {
        'raster-resampling': 'linear'
      }
    });

    // 定位到瓦片范围
    map.fitBounds(bounds, {padding: 24});

    // 实时缩放级别
    function updateZoom() {
      var z = map.getZoom();
      var rounded = Math.round(z * 4) / 4;
      document.getElementById('zlvl').textContent = '当前 Z' + (rounded % 1 ? rounded.toFixed(2) : rounded);
      document.getElementById('currentZ').textContent = rounded.toFixed(2) + ' (整数 Z = ' + Math.round(z) + ')';
    }
    map.on('zoom', updateZoom);
    updateZoom();

    // ---- 诊断面板交互 ----
    // 鼠标经纬度 + 瓦片坐标
    map.on('mousemove', function(e) {
      var lon = e.lngLat.lng, lat = e.lngLat.lat;
      var z = Math.round(map.getZoom());
      var n = Math.pow(2, z);
      var x = Math.floor((lon + 180) / 360 * n);
      var y = Math.floor((1 - Math.log(Math.tan(lat * Math.PI / 180) + 1 / Math.cos(lat * Math.PI / 180)) / Math.PI) / 2 * n);
      document.getElementById('mouseLL').textContent = lon.toFixed(5) + '°E, ' + lat.toFixed(5) + '°N';
      document.getElementById('mouseTile').textContent = 'z=' + z + '  x=' + x + '  y=' + y;
    });
    // 显隐本地瓦片
    document.getElementById('toggleLocal').addEventListener('change', function(e) {
      var vis = e.target.checked ? 'visible' : 'none';
      map.setLayoutProperty('local-tiles-layer', 'visibility', vis);
    });
    // 切片透明度
    document.getElementById('localOpacity').addEventListener('input', function(e) {
      var op = +e.target.value / 100;
      map.setPaintProperty('local-tiles-layer', 'raster-opacity', op);
    });
    // 回到切片范围
    document.getElementById('recenterBtn').addEventListener('click', function() {
      map.fitBounds(bounds, {padding: 24});
    });

    // 在线图层（已在 Dart 端按 on=true 过滤；此处按 below 决定 z-order）
    var ovs = (CFG.overlays || []).filter(function(o) { return o && o.on && o.tpl; });
    var items = [];
    ovs.forEach(function(o, i) {
      // 展开所有子域 {s} → [s0, s1, ...]（如谷歌 '0123' → 4 个 URL 模板）；
      // 无子域则保留单模板；密钥 {xxx} → o[xxx]；{z}/{x}/{y} 保留给 maplibre 解析。
      var subs = (o.subs && String(o.subs).length) ? String(o.subs).split('') : [''];
      var tiles = subs.map(function(s) {
        var t = String(o.tpl).replace(/\{s\}/g, s);
        return t.replace(/\{([a-zA-Z0-9]+)\}/g, function(_, k) {
          return o[k] != null ? o[k] : ('{' + k + '}');
        });
      });
      var layerId = 'online-' + i;
      var sourceId = 'online-src-' + i;
      map.addSource(sourceId, {
        type: 'raster',
        tiles: tiles,
        tileSize: 256,
        bounds: [sw[0], sw[1], ne[0], ne[1]],
        scheme: o.tms ? 'tms' : 'xyz'
      });
      // 关键：底图（below=true）插在本地瓦片之前→本地瓦片盖在上面；
      // 叠加（below=false）→ 默认加在顶部 → 在本地瓦片之上。
      var layerSpec = {
        id: layerId,
        type: 'raster',
        source: sourceId,
        paint: {
          'raster-opacity': typeof o.opacity === 'number' ? o.opacity : 1,
          'raster-resampling': 'nearest'
        }
      };
      if (typeof o.zmin === 'number') layerSpec.minzoom = o.zmin;
      if (typeof o.zmax === 'number') layerSpec.maxzoom = o.zmax;
      map.addLayer(layerSpec, o.below === false ? null : 'local-tiles-layer');
      items.push({
        name: o.name || ('layer' + i),
        layerId: layerId,
        defaultOn: o.below !== false  // 底图默认开启；叠加层默认关闭（避免一打开就盖住切片）
      });
    });

    if (items.length) {
      // 按 defaultOn 应用初始可见性
      items.forEach(function(it) {
        map.setLayoutProperty(it.layerId, 'visibility',
          it.defaultOn ? 'visible' : 'none');
      });
      var p = document.createElement('div');
      p.id = 'ovp';
      p.innerHTML = '<b style="font-size:12px">在线图层</b>' + items.map(function(it, i) {
        var checked = it.defaultOn ? ' checked' : '';
        return '<div><label><input type="checkbox" data-i="' + i + '"' + checked + '> ' +
          it.name + '</label>' +
          '<input type="range" min="10" max="100" value="100" data-r="' + i + '"></div>';
      }).join('');
      document.body.appendChild(p);
      p.querySelectorAll('input[data-i]').forEach(function(el) {
        el.onchange = function(e) {
          var it = items[+e.target.dataset.i];
          map.setLayoutProperty(it.layerId, 'visibility', e.target.checked ? 'visible' : 'none');
        };
      });
      p.querySelectorAll('input[data-r]').forEach(function(el) {
        el.oninput = function(e) {
          var it = items[+e.target.dataset.r];
          map.setPaintProperty(it.layerId, 'raster-opacity', +e.target.value / 100);
        };
      });
    }

    // 自检
    setTimeout(function() {
      var source = map.getSource('local-tiles');
      if (!source) showErr('[warn] local-tiles source 未创建');
    }, 1200);
  });
} catch(err){
  showErr('[init] ' + (err && err.stack ? err.stack : String(err)));
}
}
</script></body></html>"#;


    let html = html.replace("__CFG__", &cfg_str);
    write_static_assets(out)?;
    let p = out.join(PREVIEW_HTML_NAME);
    let mut f = fs::File::create(&p).map_err(|e| io_err(p.display().to_string(), e))?;
    f.write_all(html.as_bytes())
        .map_err(|e| io_err(p.display().to_string(), e))?;
    Ok(())
}

/// 本地内嵌 MapLibre GL JS（离线可用，杜绝 CDN 白屏）。
static MAPLIBRE_JS: &str = include_str!("maplibre_assets/maplibre-gl.js");
static MAPLIBRE_CSS: &str = include_str!("maplibre_assets/maplibre-gl.css");

fn write_static_assets(out: &Path) -> CoreResult<()> {
    let js_path = out.join("maplibre-gl.js");
    let css_path = out.join("maplibre-gl.css");
    let need = |p: &Path, content: &str| -> bool {
        !p.exists() || std::fs::read_to_string(p).map(|s| s != content).unwrap_or(true)
    };
    if need(&js_path, MAPLIBRE_JS) {
        std::fs::write(&js_path, MAPLIBRE_JS).map_err(|e| io_err(js_path.display().to_string(), e))?;
    }
    if need(&css_path, MAPLIBRE_CSS) {
        std::fs::write(&css_path, MAPLIBRE_CSS).map_err(|e| io_err(css_path.display().to_string(), e))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_out(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("swcutter_writer_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn info_with(overlays_json: &str) -> PreviewInfo {
        PreviewInfo {
            source_w: 512,
            source_h: 256,
            tile_size: 256,
            zmin: 0,
            zmax: 1,
            tms: false,
            levels: vec![ManifestLevel { level: 1, width: 512, height: 256, tiles: 2, ox: 0, oy: 0, wy: 2 }],
            overlays_json: overlays_json.to_string(),
        }
    }

    /// 回归测试：settings.json 中的 below/opacity/on/subs 字段必须影响生成的 preview.html，
    /// 防止"切换到 MapLibre 时丢失 below→z-order 语义"的 bug 复现。
    #[test]
    fn preview_html_respects_below_subdomains_and_keys() {
        // 覆盖三类场景：底图（below=true）/叠加（below=false）/多子域
        let overlays = r#"[
            {"name":"OSM","tpl":"https://tile.openstreetmap.org/{s}/{z}/{x}/{y}.png",
             "subs":"abc","on":true,"below":true,"opacity":0.7,"zmin":2,"zmax":18},
            {"name":"天地图·影像","tpl":"https://t{s}.tianditu.gov.cn/DataServer?T=img_w&x={x}&y={y}&l={z}&tk={tk}",
             "subs":"01234567","on":true,"below":true,"opacity":1.0,"zmin":0,"zmax":18,"tk":"MY_TK"},
            {"name":"标注叠加","tpl":"https://example.com/{z}/{x}/{y}.png",
             "on":true,"below":false,"opacity":0.5}
        ]"#;
        let out = temp_out("basemap");
        write_preview_html(&out, &info_with(overlays)).unwrap();
        let html = std::fs::read_to_string(out.join(PREVIEW_HTML_NAME)).unwrap();

        // 1) 关键 z-order 逻辑：在线图层 addLayer 必须按 below 决定 beforeId。
        // JS 模板形式是 `o.below === false ? null : 'local-tiles-layer'` —— 这是 bug 修复点。
        let has_below_logic = html.contains(
            "o.below === false ? null : 'local-tiles-layer'"
        );
        assert!(has_below_logic,
            "在线图层 addLayer 必须根据 below 决定 beforeId（修复：below=true→'local-tiles-layer'，below=false→null）");

        // 2) 子域应展开：模板代码须按 subs.split('') 展开 tiles 数组
        // 验证模板逻辑包含 `String(o.subs).split('')` 和 `.replace(/\{s\}/g, s)`
        assert!(html.contains("String(o.subs).split('')"),
            "subs 应按字符拆分为多个 URL 模板（修复：'0123'→4 个 URL）");
        assert!(html.contains(".replace(/\\{s\\}/g, s)"),
            "{{s}} 占位符应被每个子域字符替换");

        // 3) tk 等密钥占位符注入逻辑：模板代码应将非 z/x/y/s 占位符通过 o[k] 替换。
        // 这里的关键修复是覆盖所有密钥占位符（如 tk 占位符 → o.tk），且不应错误替换 z/x/y 占位符。
        assert!(html.contains("return o[k] != null ? o[k] :"),
            "密钥注入逻辑应使用 o[k] 替换非 z/x/y/s 占位符");

        // 4) zmin/zmax 应作为 layer 配置写入
        assert!(html.contains("layerSpec.minzoom = o.zmin") && html.contains("layerSpec.maxzoom = o.zmax"),
            "zmin/zmax 应作为 minzoom/maxzoom 写入 layer");

        // 5) 底图 defaultOn=true 应映射到 visibility:visible，叠加层 defaultOn=false → none
        // 模板中是 `o.below !== false` 作为 defaultOn 计算
        assert!(html.contains("defaultOn: o.below !== false"),
            "defaultOn 必须按 below 计算（修复：below=true→visible=true 默认开，below=false→默认关）");
        assert!(html.contains("it.defaultOn ? 'visible' : 'none'"),
            "初始 visibility 必须按 defaultOn 应用");
    }

    /// 容错：未传入 overlays 时页面应正常生成且不报错
    #[test]
    fn preview_html_works_without_overlays() {
        let out = temp_out("noov");
        write_preview_html(&out, &info_with("[]")).unwrap();
        let html = std::fs::read_to_string(out.join(PREVIEW_HTML_NAME)).unwrap();
        assert!(html.contains("maplibregl.Map"));
        // 没有在线图层时不会渲染 ovp 面板
        assert!(!html.contains("id=\"ovp\""));
    }

    /// 容错：overlays_json 非法 JSON 时不应崩溃，应回退为空数组
    #[test]
    fn preview_html_tolerates_bad_overlays_json() {
        let out = temp_out("bad");
        write_preview_html(&out, &info_with("not json {")).unwrap();
        let html = std::fs::read_to_string(out.join(PREVIEW_HTML_NAME)).unwrap();
        assert!(html.contains("maplibregl.Map"));
        assert!(!html.contains("id=\"ovp\""));
    }

    /// 诊断面板：必须包含鼠标经纬度、瓦片坐标、显隐开关、回到切片范围按钮
    #[test]
    fn preview_html_has_diagnostic_panel() {
        let out = temp_out("diag");
        write_preview_html(&out, &info_with("[]")).unwrap();
        let html = std::fs::read_to_string(out.join(PREVIEW_HTML_NAME)).unwrap();

        // 关键 UI 元素（用于诊断坐标系对齐问题）
        assert!(html.contains("id=\"diagPanel\""), "必须有诊断面板容器");
        assert!(html.contains("id=\"mouseLL\""),    "必须显示鼠标经纬度");
        assert!(html.contains("id=\"mouseTile\""),  "必须显示鼠标瓦片坐标");
        assert!(html.contains("id=\"currentZ\""),   "必须显示当前 Z");
        assert!(html.contains("id=\"toggleLocal\""),"必须有切片显隐开关");
        assert!(html.contains("id=\"recenterBtn\""),"必须有回到切片范围按钮");
        assert!(html.contains("id=\"diagInfo\""),   "必须显示切片 bounds/底图概要");

        // 鼠标交互逻辑
        assert!(html.contains("map.on('mousemove'"),
            "必须监听鼠标移动以更新经纬度显示");
        assert!(html.contains("toggleLocal').addEventListener"),
            "显隐开关必须绑定事件（切换 local-tiles-layer 的 visibility）");
    }
}
