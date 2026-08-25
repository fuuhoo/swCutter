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

#[derive(Debug, Clone, Serialize)]
pub struct ManifestLevel {
    pub level: u32,
    pub width: u32,
    pub height: u32,
    pub tiles: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Manifest {
    pub app: &'static str,
    pub version: &'static str,
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
}

/// 生成本地零依赖的瓦片浏览器查看页（vanilla JS，离线可用）。
pub fn write_preview_html(out: &Path, info: &PreviewInfo) -> CoreResult<()> {
    let cfg = serde_json::json!({
        "w": info.source_w,
        "h": info.source_h,
        "t": info.tile_size,
        "zmin": info.zmin,
        "zmax": info.zmax,
        "tms": info.tms,
        "levels": info.levels.iter().map(|l| serde_json::json!({
            "z": l.level, "w": l.width, "h": l.height,
            "tx": (l.width as f64 / info.tile_size as f64).ceil() as u64,
            "ty": (l.height as f64 / info.tile_size as f64).ceil() as u64,
        })).collect::<Vec<_>>(),
    });
    let cfg_str =
        serde_json::to_string(&cfg).map_err(|e| super::error::CoreError::Encoding(e.to_string()))?;

    let html = r#"<!DOCTYPE html>
<html lang="zh-CN"><head><meta charset="utf-8">
<title>swCutter 瓦片预览</title>
<style>
 :root{color-scheme:dark}
 html,body{margin:0;height:100%;background:#0f1219;color:#dfe5ef;font-family:"Microsoft YaHei UI",system-ui,sans-serif;overflow:hidden}
 #bar{display:flex;gap:10px;align-items:center;padding:10px 14px;background:#171c26;border-bottom:1px solid #ffffff14}
 #bar b{font-size:13px}
 #bar .sp{flex:1}
 button{background:#232a37;border:1px solid #ffffff18;color:#dfe5ef;border-radius:8px;padding:5px 12px;cursor:pointer;font-size:13px}
 button:hover{background:#2b3446}
 #zoom{min-width:64px;text-align:center;font-variant-numeric:tabular-nums;color:#9fb0cc;font-size:12px}
 #map{position:absolute;inset:46px 0 0 0;cursor:grab}
 #map.drag{cursor:grabbing}
 img.tile{position:absolute;image-rendering:auto;-webkit-user-drag:none;user-select:none}
 #hint{position:absolute;right:12px;bottom:10px;font-size:11px;color:#7a8699}
</style></head>
<body>
<div id="bar">
 <b>🧩 swCutter 预览</b><span id="meta"></span><span class="sp"></span>
 <button id="out">−</button><span id="zoom"></span><button id="in">＋</button>
 <button id="fit">适应窗口</button><button id="one">1:1</button>
</div>
<div id="map"></div>
<div id="hint">拖动平移 · 滚轮缩放 · 双击放大</div>
<script>
const CFG = __CFG__;
const map = document.getElementById('map');
const zoomLabel = document.getElementById('zoom');
document.getElementById('meta').textContent =
  ` ${CFG.w}×${CFG.h}px · 级别 ${CFG.zmin}–${CFG.zmax} · 瓦片 ${CFG.t}px`;

// 视图状态：源像素坐标 (cx,cy) 为视口中心，scale = 屏幕 px / 源 px
let scale = 1, cx = CFG.w/2, cy = CFG.h/2;
const cache = new Set();
const imgs = [];

function lvlMeta(){ 
  const want = Math.round(CFG.zmax - Math.log2(scale));
  let best = CFG.levels[0];
  for(const l of CFG.levels){ if(l.z === Math.min(CFG.zmax, Math.max(CFG.zmin, want))) best = l; }
  return best || CFG.levels[CFG.levels.length-1];
}
function apply(){
  // 清理上一帧
  for(const el of imgs) el.remove(); imgs.length = 0;
  const L = lvlMeta();
  const vw = map.clientWidth, vh = map.clientHeight;
  // 该级别像素→屏幕
  const k = scale * Math.pow(2, CFG.zmax - L.z);
  const ts = CFG.t * k;
  if(ts < 4){ zoomLabel.textContent = `${(scale*100)|0}%`; return; }
  const left = cx - vw/2/scale, top = cy - vh/2/scale;   // 源坐标视口左上
  const lx0 = Math.max(0, Math.floor(left / CFG.t)), ly0 = Math.max(0, Math.floor(top / CFG.t));
  const lx1 = Math.min(L.tx-1, Math.floor((left + vw/scale) / CFG.t));
  const ly1 = Math.min(L.ty-1, Math.floor((top + vh/scale) / CFG.t));
  for(let ty=ly0; ty<=ly1; ty++){
    for(let tx=lx0; tx<=lx1; tx++){
      const dy = CFG.tms ? (L.ty-1-ty) : ty;
      const key = `${L.z}/${tx}/${dy}`;
      const sx = (tx*CFG.t - left)*scale, sy = (ty*CFG.t - top)*scale;
      let img = new Image();
      img.className='tile';
      img.style.left = sx+'px'; img.style.top = sy+'px';
      img.style.width = (ts+1)+'px'; img.style.height=(ts+1)+'px';
      img.src = `${L.z}/${tx}/${dy}.png`;
      map.appendChild(img); imgs.push(img);
    }
  }
  zoomLabel.textContent = `${Math.round(scale*100)}% · L${L.z}`;
}
function clampCenter(){
  cx = Math.max(0, Math.min(CFG.w, cx));
  cy = Math.max(0, Math.min(CFG.h, cy));
}
function fit(){
  scale = Math.min(map.clientWidth/CFG.w, map.clientHeight/CFG.h);
  cx=CFG.w/2; cy=CFG.h/2; clampCenter(); apply();
}
function one(){ scale=1; apply(); }
function zoomAt(f, mx, my){
  const before = {x: cx + (mx - map.clientWidth/2)/scale,
                  y: cy + (my - map.clientHeight/2)/scale};
  scale = Math.min(64, Math.max(0.02, scale*f));
  cx = before.x - (mx - map.clientWidth/2)/scale;
  cy = before.y - (my - map.clientHeight/2)/scale;
  clampCenter(); apply();
}
map.addEventListener('wheel', e=>{ e.preventDefault();
  zoomAt(e.deltaY<0 ? 1.25 : 0.8, e.offsetX, e.offsetY);
},{passive:false});
let drag=null;
map.addEventListener('pointerdown',e=>{drag={x:e.clientX,y:e.clientY,cx,cy};map.classList.add('drag');map.setPointerCapture(e.pointerId);});
map.addEventListener('pointermove',e=>{ if(!drag)return;
  cx = drag.cx - (e.clientX-drag.x)/scale; cy = drag.cy - (e.clientY-drag.y)/scale;
  clampCenter(); apply();});
addEventListener('pointerup',()=>{drag=null;map.classList.remove('drag');});
map.addEventListener('dblclick',e=>zoomAt(2,e.offsetX,e.offsetY));
document.getElementById('in').onclick=()=>zoomAt(1.3,map.clientWidth/2,map.clientHeight/2);
document.getElementById('out').onclick=()=>zoomAt(0.77,map.clientWidth/2,map.clientHeight/2);
document.getElementById('fit').onclick=fit;
document.getElementById('one').onclick=one;
addEventListener('resize',apply);
fit();
</script></body></html>"#;

    let html = html.replace("__CFG__", &cfg_str);
    let p = out.join(PREVIEW_HTML_NAME);
    let mut f = fs::File::create(&p).map_err(|e| io_err(p.display().to_string(), e))?;
    f.write_all(html.as_bytes())
        .map_err(|e| io_err(p.display().to_string(), e))?;
    Ok(())
}
