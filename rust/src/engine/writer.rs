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
<link rel="stylesheet" href="https://unpkg.com/leaflet@1.9.4/dist/leaflet.css">
<script src="https://unpkg.com/leaflet@1.9.4/dist/leaflet.js"></script>
<style>
 html,body{margin:0;height:100%;background:#0d1117}
 #map{position:absolute;inset:0}
 #bar{position:absolute;left:0;right:0;top:0;z-index:1000;display:flex;gap:10px;align-items:center;
      padding:8px 14px;background:#171c26e6;border-bottom:1px solid #ffffff14}
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
 <b>🧩 swCutter 预览（Leaflet）</b>
 <span class="sp"></span>
 <span id="badge"></span>
 <span id="hint">拖动 / 滚轮缩放 · 双击放大</span>
</div>
<div id="map"></div>
<script>
const CFG = __CFG__;

// ---- 运行诊断：任何异常都显示在页面上，避免白屏无法排查 ----
function showErr(msg){
  let el = document.getElementById('errBox');
  if(!el){ el = document.createElement('div'); el.id='errBox';
    el.style.cssText='position:absolute;left:10px;bottom:10px;right:10px;z-index:2000;background:#3a1010f2;color:#ffb4b4;border:1px solid #ff555555;border-radius:10px;padding:10px;font:12px/1.6 monospace;white-space:pre-wrap;max-height:40vh;overflow:auto';
    document.body.appendChild(el); }
  el.textContent = (el.textContent? el.textContent+'\n':'') + msg;
}
window.addEventListener('error', e => showErr('[error] '+e.message+' @'+(e.filename||'')+':'+(e.lineno||'?')));
window.addEventListener('unhandledrejection', e => showErr('[promise] '+String(e.reason)));

// ---- 世界网格 → 经纬度（EPSG:3857 XYZ/TMS 惯例）----
function nn(z){ return Math.pow(2, z); }
function tileLL(x, y, z){
  const n = nn(z);
  const lon = x / n * 360 - 180;
  const lat = Math.atan(Math.sinh(Math.PI * (1 - 2 * y / n))) * 180 / Math.PI;
  return [lat, lon];
}

if (typeof L === 'undefined') {
  document.getElementById('map').innerHTML =
    '<div class="offline">Leaflet 未能从 CDN 加载（离线？）。瓦片本身完好，可稍后重试或直接浏览 {z}/{x}/{y}.png 目录。</div>';
} else {
try {
  document.getElementById('badge').textContent =
    `Z${CFG.zmin}–Z${CFG.zmax} · ${CFG.tms?'TMS':'XYZ'} · ${CFG.t}px`;

  // ---- 本地瓦片层 ----
  const B = CFG.levels.find(l => l.z === CFG.zmax) || CFG.levels[CFG.levels.length - 1] || {ox:0, oy:0, tx:1, ty:1, z:CFG.zmax};
  const bo = { ox: B.ox || 0, oy: B.oy || 0 };
  // 瓦片包围盒的地理角点：SW=(左下) NE=(右上)；TMS 的 oy 即 XYZ 行号起点
  const swCorner = tileLL(bo.ox,        bo.oy + B.ty, B.z);
  const neCorner = tileLL(bo.ox + B.tx, bo.oy,        B.z);
  let bbox = L.latLngBounds(swCorner, neCorner);
  if(!bbox.isValid() || Math.abs(bbox.getNorth()-bbox.getSouth()) < 1e-9){
    // 边界无效（异常数据）→ 退回全球范围
    bbox = L.latLngBounds([-85, -180], [85, 180]);
    showErr('[warn] 瓦片包围盒无效，已退回全球范围: '+JSON.stringify({sw:swCorner,ne:neCorner}));
  }

  const map = L.map('map', { zoomSnap: 0.25, zoomDelta: 0.5 });
  map.attributionControl.setPrefix('Leaflet');

  const localOpts = {
    tms: !!CFG.tms,
    tileSize: CFG.t,
    minZoom: CFG.zmin,
    maxZoom: CFG.zmax,
    minNativeZoom: CFG.zmin,
    maxNativeZoom: CFG.zmax,
    noWrap: true,
    bounds: bbox,
    attribution: 'swCutter'
  };
  const local = L.tileLayer('{z}/{x}/{y}.png', localOpts).addTo(map);

  // ---- 在线图层（仅当设置启用了条目才会出现在 CFG.overlays）----
  const ovs = (CFG.overlays || []).filter(o => o && o.on && o.tpl);
  const items = [];
  if (ovs.length) {
    map.createPane('under').style.zIndex = 250;
    map.createPane('over').style.zIndex  = 450;
    ovs.forEach((o, i) => {
      const tpl = String(o.tpl).replace(/\{tk\}/g, o.tk || '');
      const subs = (o.subs && String(o.subs).length) ? String(o.subs).split('') : ['a'];
      const layer = L.tileLayer(tpl, {
        tms: !!o.tms,
        opacity: (typeof o.opacity === 'number' ? o.opacity : 1),
        minZoom: (o.zmin ?? 0),
        maxZoom: (o.zmax ?? 22),
        pane: o.below ? 'under' : 'over',
        noWrap: true,
        bounds: bbox,
        subdomains: subs
      }).addTo(map);
      items.push({ name: o.name || ('layer' + i), layer });
    });

    const p = document.createElement('div');
    p.id = 'ovp';
    p.innerHTML = '<b style="font-size:12px">在线图层</b>' + items.map((it, i) =>
      `<div><label><input type="checkbox" checked data-i="${i}"> ${it.name}</label>` +
      `<input type="range" min="10" max="100" value="${Math.round((it.layer.options.opacity ?? 1) * 100)}" data-r="${i}"></div>`
    ).join('');
    document.body.appendChild(p);
    p.querySelectorAll('input[data-i]').forEach(el => el.onchange = e => {
      const it = items[+e.target.dataset.i];
      if (e.target.checked) it.layer.addTo(map); else map.removeLayer(it.layer);
    });
    p.querySelectorAll('input[data-r]').forEach(el => el.oninput = e => {
      items[+e.target.dataset.r].layer.setOpacity(+e.target.value / 100);
    });
  }

  L.control.scale({ imperial: false }).addTo(map);
  // ---- 主动定位到瓦片包围盒 ----
  map.fitBounds(bbox, { padding: [24, 24] });
  // ---- 自检：首帧后若本地瓦片 0 张 → 提示（排查白屏）----
  setTimeout(() => {
    const n = Object.keys(local._tiles || {}).length;
    if(!n) showErr('[warn] 首帧没有任何本地瓦片被请求。检查：输出目录是否含 {z}/{x}/{y}.png、CFG.levels 是否为空、URL 是否为 http(s) 访问。');
  }, 1200);
} catch(err){
  showErr('[init] ' + (err && err.stack ? err.stack : String(err)));
}
}
</script></body></html>"#;


    let html = html.replace("__CFG__", &cfg_str);
    let p = out.join(PREVIEW_HTML_NAME);
    let mut f = fs::File::create(&p).map_err(|e| io_err(p.display().to_string(), e))?;
    f.write_all(html.as_bytes())
        .map_err(|e| io_err(p.display().to_string(), e))?;
    Ok(())
}
