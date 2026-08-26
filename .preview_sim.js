// 预览页逻辑仿真：用真实 mercator CFG 驱动模板 JS（Leaflet 桩），检查瓦片请求
const fs = require('fs');

// 模拟一份 tile_0 式输出：Z1..Z19，Z19 世界偏移 ox=268000, oy=340000
const levels = [];
const base = { z: 19, ox: 268000, oy: 340000, tx: 300, ty: 190, wy: 1 << 19 };
for (let z = 1; z <= 19; z++) {
  const f = Math.pow(2, 19 - z);
  levels.push({ z, ox: Math.floor(base.ox / f), oy: Math.floor(base.oy / f), tx: base.tx * f, ty: base.ty * f, wy: 1 << z });
}
const CFG = {
  w: 15250, h: 19300, t: 256, zmin: 1, zmax: 19, tms: false,
  overlays: [], levels,
};

// ---- Leaflet 桩 ----
const requested = [];
function makeLayer(tpl, opts) {
  return { options: opts || {}, tpl, _tiles: {},
    addTo: (m) => { m._layers = m._layers || []; m._layers.push(this); return this; },
    remove: () => {},
  };
}
const L = {
  map: (id, opts) => {
    const map = {
      _opts: opts, _layers: [],
      attributionControl: { setPrefix: () => {} },
      createPane: (n) => ({ style: {} }),
      fitBounds: (b, o) => {
        const z = Math.ceil(19);
        // 模拟 Leaflet：请求覆盖 bbox 的瓦片
        for (let y = Math.floor(b._sw[0] < 0 ? 0 : 0); y < 2; y++) {} // noop
        map._fit = { b, o };
      },
      removeLayer: () => {}, addLayer: () => {},
      on: () => {}, off: () => {},
    };
    return map;
  },
  tileLayer: (tpl, opts) => makeLayer(tpl, opts),
  latLngBounds: (sw, ne) => ({ _sw: sw, _ne: ne, isValid: () => true,
    getNorth: () => ne[0], getSouth: () => sw[0], getWest: () => sw[1], getEast: () => ne[1] }),
  control: { scale: () => ({ addTo: () => {} }) },
};

// ---- 最小 DOM 桩 ----
const elements = {};
const document = {
  getElementById: (id) => { if (!elements[id]) elements[id] = { style: {}, textContent: '', innerHTML: '', appendChild: () => {}, querySelectorAll: () => [], addEventListener: () => {} }; return elements[id]; },
  createElement: (tag) => ({ style: {}, textContent: '', innerHTML: '', appendChild: () => {}, querySelectorAll: () => [] }),
  body: { appendChild: () => {} },
};
const window = { addEventListener: () => {}, };
global.L = L; global.document = document; global.window = window;
global.setTimeout = (fn) => { try { fn(); } catch (e) { console.log('setTimeout err:', e.message); } };

// 载入模板 JS（替换 CFG）
const src = fs.readFileSync('H:/project_self/swCutter/rust/src/engine/writer.rs', 'utf8');
const i1 = src.indexOf('const CFG = __CFG__;');
const i2 = src.indexOf('</script>', i1);
let js = src.substring(i1, i2);
js = js.replace('__CFG__', JSON.stringify(CFG));
try {
  eval(js);
  console.log('OK: 无异常。CFG.levels=', levels.length, 'base z19 ox=', base.ox, 'tx=', base.tx);
} catch (e) {
  console.log('RUNTIME ERROR:', e.message);
  console.log(e.stack.split('\n').slice(0, 6).join('\n'));
}
