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
 img.tile{position:absolute;image-rendering:auto;-webkit-user-drag:none;user-select:none;z-index:3}
 img.ov{position:absolute;pointer-events:none;-webkit-user-drag:none;z-index:6}
 img.ovb{z-index:1;background:#0d1117}
 #hint{position:absolute;right:12px;bottom:10px;font-size:11px;color:#7a8699}
 #panel{position:absolute;top:50px;right:10px;width:min(430px,92vw);max-height:70vh;overflow:auto;
        background:#171c26f2;border:1px solid #ffffff20;border-radius:12px;padding:12px;z-index:50;
        display:flex;flex-direction:column;gap:8px;font-size:12.5px}
 #panel[hidden]{display:none}
 .prow{display:flex;gap:6px;align-items:center;flex-wrap:wrap}
 .phint{color:#7a8699;font-size:11px;line-height:1.5}
 #panel input[type=text],#panel input:not([type]),#panel input[type=number]{
   background:#0f1219;border:1px solid #ffffff22;color:#dfe5ef;border-radius:7px;padding:5px 8px;font-size:12px}
 .chk{display:flex;align-items:center;gap:3px;color:#9fb0cc}
 .ovItem{border:1px solid #ffffff14;border-radius:9px;padding:7px 9px;display:flex;flex-direction:column;gap:5px;background:#10141d}
 .ovItem .row{display:flex;gap:7px;align-items:center}
 .ovItem input[type=range]{flex:1;accent-color:#4f8cff}
 .tag{font-size:10.5px;color:#7a8699}
</style></head>
<body>
<div id="bar">
 <b>🧩 swCutter 预览</b><span id="meta"></span><span class="sp"></span>
 <button id="out">−</button><span id="zoom"></span><button id="in">＋</button>
 <button id="fit">适应窗口</button><button id="one">1:1</button>
 <span id="badge"></span>
 <button id="layersBtn">图层</button>
</div>
<div id="panel" hidden>
 <div class="prow"><b>在线图层对照</b><span style="flex:1"></span><button id="addPresetTdt">天地图·矢量</button><button id="addPresetImg">天地图·影像</button><button id="addPresetOsm">OSM</button></div>
 <div class="phint">叠加层与本地金字塔同层级同编号对齐，用于核对切片网格/排列；天地图需填 tk 密钥。</div>
 <div class="prow"><input id="tkInput" placeholder="天地图 tk 密钥（保存到本机）"><button id="saveTk">保存密钥</button></div>
 <div id="ovList"></div>
 <div class="prow">
   <input id="ovName" placeholder="名称" style="width:90px">
   <input id="ovTpl" placeholder="URL 模板，含 {z} {x} {y} 可选 {s}" style="flex:1">
   <label class="chk"><input type="checkbox" id="ovTms"> TMS</label>
   <input id="ovSubs" placeholder="子域如 012" style="width:64px">
   <button id="ovAdd">添加</button>
 </div>
</div>
<div id="map"></div>
<div id="hint">拖动平移 · 滚轮缩放 · 双击放大</div>
<script>
const CFG = __CFG__;
const map = document.getElementById('map');
const zoomLabel = document.getElementById('zoom');
document.getElementById('meta').textContent =
  ` ${CFG.w}×${CFG.h}px · 瓦片 ${CFG.t}px`;
// 右上角信息徽章：层级范围 + 排列方式
document.getElementById('badge').textContent = `Z${CFG.zmin}–Z${CFG.zmax} · ${CFG.tms?'TMS':'XYZ'}`;
document.getElementById('badge').style.cssText='font-size:12px;font-weight:700;color:#8fb4ff;background:#4f8cff22;border:1px solid #4f8cff44;border-radius:999px;padding:4px 10px';

// 视图状态：源像素坐标 (cx,cy) 为视口中心，scale = 屏幕 px / 源 px
let scale = 1, cx = CFG.w/2, cy = CFG.h/2;
const cache = new Set();
const imgs = [];
// 前置声明：apply() 首帧（fit→apply）就会触达这些绑定，必须先于调用点初始化
let curL=null; let tsGuard=0; let ovImgs=[];

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
  tsGuard = ts;
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
  drawOverlays(L, left, top, vw, vh, k);
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

/* ---------------- 在线叠加图层 ---------------- */
const LS_KEY='swcutter_overlays', LS_TK='swcutter_tk';
// 全局设置(CFG.overlays)优先；本页 localStorage 修改仅作回退/临时覆盖
const cfgOv = Array.isArray(CFG.overlays) ? JSON.parse(JSON.stringify(CFG.overlays)) : [];
let overlays = cfgOv;
try{
  const parsed = JSON.parse(localStorage.getItem(LS_KEY)||'null');
  if(!(cfgOv.length) && Array.isArray(parsed)) overlays = parsed;
}catch(e){}
const tkInput=document.getElementById('tkInput');
tkInput.value = localStorage.getItem(LS_TK)||'';
document.getElementById('saveTk').onclick=()=>{ localStorage.setItem(LS_TK, tkInput.value.trim()); renderOvList(); };
document.getElementById('layersBtn').onclick=()=>{ const p=document.getElementById('panel'); p.hidden=!p.hidden; };

function saveOverlays(){ localStorage.setItem(LS_KEY, JSON.stringify(overlays)); renderOvList(); apply(); }
const TDT = t=>`https://t{s}.tianditu.gov.cn/DataServer?T=${t}&x={x}&y={y}&l={z}&tk={tk}`;
const PRESETS={
  tdtVec:{name:'天地图·矢量', tpl:TDT('vec_w'), subs:'01234567', tms:false},
  tdtImg:{name:'天地图·影像', tpl:TDT('img_w'), subs:'01234567', tms:false},
  osm:{name:'OpenStreetMap', tpl:'https://tile.openstreetmap.org/{z}/{x}/{y}.png', subs:'', tms:false},
};
document.getElementById('addPresetTdt').onclick=()=>addPreset('tdtVec');
document.getElementById('addPresetImg').onclick=()=>addPreset('tdtImg');
document.getElementById('addPresetOsm').onclick=()=>addPreset('osm');
function addPreset(k){
  if(overlays.some(o=>o.name===PRESETS[k].name)) return;
  overlays.push({...PRESETS[k], opacity:0.55, on:true, zmin:2, zmax:18});
  saveOverlays();
}
document.getElementById('ovAdd').onclick=()=>{
  const name=document.getElementById('ovName').value.trim()||'自定义';
  const tpl=document.getElementById('ovTpl').value.trim();
  if(!tpl.includes('{x}')||!tpl.includes('{y}')){ alert('模板需包含 {x} 与 {y}'); return; }
  overlays.push({name,tpl,subs:document.getElementById('ovSubs').value.trim(),
                 tms:document.getElementById('ovTms').checked,opacity:0.55,on:true,zmin:0,zmax:22});
  document.getElementById('ovName').value=''; document.getElementById('ovTpl').value='';
  saveOverlays();
};
function renderOvList(){
  const list=document.getElementById('ovList'); list.innerHTML='';
  overlays.forEach((o,i)=>{
    const div=document.createElement('div'); div.className='ovItem';
    div.innerHTML=`<div class="row">
      <label class="chk"><input type="checkbox" ${o.on?'checked':''} data-i="${i}" data-act="on"> ${o.name}</label>
      <span style="flex:1"></span>
      <span class="tag">${o.tms?'TMS':'XYZ'}${o.subs?` · s=${o.subs}`:''}</span>
      <button data-i="${i}" data-act="del">删除</button></div>
      <div class="row"><span class="tag">透明度</span>
      <input type="range" min="10" max="100" value="${Math.round(o.opacity*100)}" data-i="${i}" data-act="op">
      <button data-i="${i}" data-act="up">${o.on?'隐藏':'显示'}</button></div>`;
    list.appendChild(div);
  });
  list.querySelectorAll('input[type=checkbox]').forEach(el=>el.onchange=e=>{overlays[+e.target.dataset.i].on=e.target.checked;saveOverlays();});
  list.querySelectorAll('input[type=range]').forEach(el=>el.oninput=e=>{overlays[+e.target.dataset.i].opacity=+e.target.value/100;applySoft();});
  list.querySelectorAll('button[data-act=del]').forEach(el=>el.onclick=e=>{overlays.splice(+e.target.dataset.i,1);saveOverlays();});
  list.querySelectorAll('button[data-act=up]').forEach(el=>el.onclick=e=>{const o=overlays[+e.target.dataset.i];o.on=!o.on;saveOverlays();});
}
function applySoft(){ if(!curL) return;
  for(const el of ovImgs){ const o=overlays[el._ovi]; if(o) el.style.opacity=o.opacity; } }

function drawOverlays(L,left,top,vw,vh,k){
  for(const el of ovImgs) el.remove(); ovImgs.length=0;
  curL=L;
  if(tsGuard<4) return;
  const lx1e=Math.min(L.tx, Math.floor((left+vw/scale)/CFG.t)+1);
  const ly1e=Math.min(L.ty, Math.floor((top+vh/scale)/CFG.t)+1);
  const lx0=Math.max(0, Math.floor(left/CFG.t)), ly0=Math.max(0, Math.floor(top/CFG.t));
  overlays.forEach((o,oi)=>{
    if(!o.on) return;
    const tkv = localStorage.getItem(LS_TK) || o.tk || '';
    let tpl=o.tpl.replace(/\{tk\}/g, tkv);
    if(tpl.includes('{tk}')) return; // 无密钥不请求
    const oz=Math.max(o.zmin??0, Math.min(o.zmax??22, L.z));
    const flip=o.tms? (Math.pow(2,oz)-1):null;
    for(let ty=ly0; ty<ly1e; ty++){
      for(let tx=lx0; tx<lx1e; tx++){
        const oy=flip!=null? flip-ty : ty;
        let url=tpl.replace('{z}',oz).replace('{x}',tx).replace('{y}',oy);
        if(o.subs&&o.subs.length){ url=url.replace('{s}', o.subs[(tx+ty)%o.subs.length]); }
        const img=new Image();
        img.className = o.below ? 'ov ovb' : 'ov';
        img.style.left=(tx*CFG.t-left)*scale+'px';
        img.style.top=(ty*CFG.t-top)*scale+'px';
        img.style.width=(CFG.t*k+1)+'px'; img.style.height=(CFG.t*k+1)+'px';
        img.style.opacity=o.opacity;
        img.onerror=()=>img.remove();
        img.src=url;
        map.appendChild(img); ovImgs.push(img); img._ovi=oi;
      }
    }
  });
}
renderOvList();
</script></body></html>"#;

    let html = html.replace("__CFG__", &cfg_str);
    let p = out.join(PREVIEW_HTML_NAME);
    let mut f = fs::File::create(&p).map_err(|e| io_err(p.display().to_string(), e))?;
    f.write_all(html.as_bytes())
        .map_err(|e| io_err(p.display().to_string(), e))?;
    Ok(())
}
