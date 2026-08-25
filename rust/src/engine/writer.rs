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
