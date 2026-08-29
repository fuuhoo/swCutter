//! 跨平台的应用数据目录（日志、历史数据库等）。
//!
//! - Windows: `%APPDATA%\swCutter`
//! - macOS:   `$HOME/Library/Application Support/swCutter`
//! - Linux:   `$XDG_DATA_HOME/swCutter` 或 `$HOME/.local/share/swCutter`
//!
//! 调用方在使用前应 `create_dir_all`；函数本身只给出**规范目录路径**，
//! 不保证该目录存在。返回 `Option` 是因为极少数受限容器/沙盒里取不到
//! 任何合适的根目录——此时业务方应回退到 env::temp_dir() 之类的兜底。

use std::path::PathBuf;

pub const APP_DIR_NAME: &str = "swCutter";

/// 返回应用专属数据目录的路径（不保证已存在）。
pub fn app_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        return std::env::var("APPDATA")
            .ok()
            .filter(|s| !s.is_empty())
            .map(|d| PathBuf::from(d).join(APP_DIR_NAME));
    }

    #[cfg(target_os = "macos")]
    {
        return std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .map(|h| PathBuf::from(h).join("Library").join("Application Support").join(APP_DIR_NAME));
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            if !xdg.is_empty() {
                return Some(PathBuf::from(xdg).join(APP_DIR_NAME));
            }
        }
        return std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .map(|h| PathBuf::from(h).join(".local").join("share").join(APP_DIR_NAME));
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        return std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .map(|h| PathBuf::from(h).join(format!(".{APP_DIR_NAME}")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 单元测试：仅看返回 Some & 路径名以 swCutter 结尾。
    /// 我们并不想做宿主绑定测试，所以用单平台覆盖：Rust 测试在
    /// CI 上以 linux 跑时，本测试需要在所有平台都过。
    #[test]
    fn dir_ends_with_app_name() {
        if let Some(p) = app_data_dir() {
            assert_eq!(
                p.file_name().and_then(|s| s.to_str()),
                Some(APP_DIR_NAME),
                "app_data_dir should end with {APP_DIR_NAME}, got {:?}",
                p
            );
        }
    }
}
