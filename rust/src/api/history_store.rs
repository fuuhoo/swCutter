//! 任务历史 SQLite 存储：%APPDATA%\swCutter\history.db
//! 每次保存为「全量重写」事务（列表小，简单可靠）；首次运行自动迁移旧 history.json。

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use rusqlite::Connection;

use crate::engine::alpha::AlphaMode;
use crate::engine::planner::{Resample, Scheme};

/// 一条任务历史记录（与 TaskEntry/TaskDto 字段对应）。
#[derive(Clone, Debug)]
pub struct Rec {
    pub id: u64,
    pub source: String,
    pub output: String,
    pub tile_size: u32,
    pub scheme: Scheme,
    pub alpha: AlphaMode,
    pub resample: Resample,
    pub zmin: Option<u32>,
    pub zmax: Option<u32>,
    pub status: String,
    pub level: u32,
    pub tiles_done: u64,
    pub total_tiles: u64,
    pub bytes_written: u64,
    pub elapsed_ms: u64,
    pub error: Option<String>,
    pub started_ms: u64,
    pub finished_ms: u64,
}

fn db_path() -> Option<PathBuf> {
    std::env::var("APPDATA")
        .ok()
        .map(|d| PathBuf::from(d).join("swCutter").join("history.db"))
}

fn conn() -> &'static Mutex<Connection> {
    static CONN: OnceLock<Mutex<Connection>> = OnceLock::new();
    CONN.get_or_init(|| {
        let path = db_path().unwrap_or_else(|| PathBuf::from("swcutter_history.db"));
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let c = Connection::open(&path).expect("open sqlite history");
        c.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS tasks (
                id INTEGER PRIMARY KEY,
                source TEXT NOT NULL,
                output TEXT NOT NULL,
                tile_size INTEGER NOT NULL,
                scheme TEXT NOT NULL,
                alpha TEXT NOT NULL,
                resample TEXT NOT NULL,
                zmin INTEGER,
                zmax INTEGER,
                status TEXT NOT NULL,
                level INTEGER NOT NULL DEFAULT 0,
                tiles_done INTEGER NOT NULL DEFAULT 0,
                total_tiles INTEGER NOT NULL DEFAULT 0,
                bytes_written INTEGER NOT NULL DEFAULT 0,
                elapsed_ms INTEGER NOT NULL DEFAULT 0,
                error TEXT,
                started_ms INTEGER NOT NULL DEFAULT 0,
                finished_ms INTEGER NOT NULL DEFAULT 0
             );",
        )
        .expect("init history schema");
        Mutex::new(c)
    })
}

/// 初始化：迁移旧 JSON（若存在）。
pub fn init() {
    import_legacy_json();
}

fn ser<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_default()
}
fn de<T: serde::de::DeserializeOwned>(s: &str) -> Option<T> {
    serde_json::from_str(s).ok()
}

/// 读取全部记录（按 id 升序）。
pub fn load() -> Vec<Rec> {
    let c = conn().lock().unwrap();
    let mut stmt = match c.prepare(
        "SELECT id, source, output, tile_size, scheme, alpha, resample, zmin, zmax,
                status, level, tiles_done, total_tiles, bytes_written, elapsed_ms,
                error, started_ms, finished_ms
         FROM tasks ORDER BY id ASC",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let map = |r: &rusqlite::Row| -> rusqlite::Result<Rec> {
        let scheme_s: String = r.get(4)?;
        let alpha_s: String = r.get(5)?;
        let resample_s: String = r.get(6)?;
        Ok(Rec {
            id: r.get::<_, i64>(0)? as u64,
            source: r.get(1)?,
            output: r.get(2)?,
            tile_size: r.get::<_, i64>(3)? as u32,
            scheme: de(&scheme_s).unwrap_or(Scheme::Xyz),
            alpha: de(&alpha_s).unwrap_or(AlphaMode::Keep),
            resample: de(&resample_s).unwrap_or(Resample::Bilinear),
            zmin: r.get::<_, Option<i64>>(7)?.map(|v| v as u32),
            zmax: r.get::<_, Option<i64>>(8)?.map(|v| v as u32),
            status: r.get(9)?,
            level: r.get::<_, i64>(10)? as u32,
            tiles_done: r.get::<_, i64>(11)? as u64,
            total_tiles: r.get::<_, i64>(12)? as u64,
            bytes_written: r.get::<_, i64>(13)? as u64,
            elapsed_ms: r.get::<_, i64>(14)? as u64,
            error: r.get(15)?,
            started_ms: r.get::<_, i64>(16)? as u64,
            finished_ms: r.get::<_, i64>(17)? as u64,
        })
    };
    let rows = stmt.query_map([], map);
    match rows {
        Ok(it) => it.filter_map(Result::ok).collect(),
        Err(_) => Vec::new(),
    }
}

/// 全量保存（单事务）。
pub fn save(recs: &[Rec]) {
    let c = conn().lock().unwrap();
    if c.execute_batch("BEGIN").is_err() {
        return;
    }
    let ok = (|| -> Result<(), rusqlite::Error> {
        c.execute("DELETE FROM tasks", [])?;
        {
            let mut ins = c.prepare_cached(
                "INSERT INTO tasks (id, source, output, tile_size, scheme, alpha, resample,
                                    zmin, zmax, status, level, tiles_done, total_tiles,
                                    bytes_written, elapsed_ms, error, started_ms, finished_ms)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
            )?;
            for r in recs {
                ins.execute(rusqlite::params![
                    r.id as i64,
                    r.source,
                    r.output,
                    r.tile_size as i64,
                    ser(&r.scheme),
                    ser(&r.alpha),
                    ser(&r.resample),
                    r.zmin.map(|v| v as i64),
                    r.zmax.map(|v| v as i64),
                    r.status,
                    r.level as i64,
                    r.tiles_done as i64,
                    r.total_tiles as i64,
                    r.bytes_written as i64,
                    r.elapsed_ms as i64,
                    r.error,
                    r.started_ms as i64,
                    r.finished_ms as i64,
                ])?;
            }
        }
        Ok(())
    })();
    let _ = if ok.is_ok() { c.execute_batch("COMMIT") } else { c.execute_batch("ROLLBACK") };
}

/// 旧 history.json → SQLite 一次性迁移；成功后改名 .bak。
fn import_legacy_json() {
    let Some(base) = db_path().and_then(|p| p.parent().map(|d| d.to_path_buf())) else { return };
    let json = base.join("history.json");
    let Ok(raw) = std::fs::read_to_string(&json) else { return };
    #[derive(serde::Deserialize)]
    struct Old {
        id: u64,
        source: String,
        output: String,
        tile_size: u32,
        scheme: Scheme,
        alpha: AlphaMode,
        resample: Resample,
        zmin: Option<u32>,
        zmax: Option<u32>,
        status: String,
        #[serde(default)]
        level: u32,
        #[serde(default)]
        tiles_done: u64,
        #[serde(default)]
        total_tiles: u64,
        #[serde(default)]
        bytes_written: u64,
        #[serde(default)]
        elapsed_ms: u64,
        #[serde(default)]
        error: Option<String>,
        #[serde(default)]
        started_ms: u64,
        #[serde(default)]
        finished_ms: u64,
    }
    let Ok(old) = serde_json::from_str::<Vec<Old>>(&raw) else { return };
    let recs: Vec<Rec> = old
        .into_iter()
        .map(|o| Rec {
            id: o.id,
            source: o.source,
            output: o.output,
            tile_size: o.tile_size,
            scheme: o.scheme,
            alpha: o.alpha,
            resample: o.resample,
            zmin: o.zmin,
            zmax: o.zmax,
            status: o.status,
            level: o.level,
            tiles_done: o.tiles_done,
            total_tiles: o.total_tiles,
            bytes_written: o.bytes_written,
            elapsed_ms: o.elapsed_ms,
            error: o.error,
            started_ms: o.started_ms,
            finished_ms: o.finished_ms,
        })
        .collect();
    save(&recs);
    let _ = std::fs::rename(&json, base.join("history.json.bak"));
}
