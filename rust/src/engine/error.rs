use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("TIFF 解码失败: {0}")]
    TiffDecode(#[from] ::tiff::TiffError),

    #[error("IO 错误 ({path}): {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("不支持的 TIFF 格式: {0}")]
    Unsupported(String),

    #[error("无效参数: {0}")]
    InvalidInput(String),

    #[error("编码错误: {0}")]
    Encoding(String),

    #[error("任务已取消")]
    Cancelled,
}

pub type CoreResult<T> = Result<T, CoreError>;

pub fn io_err(path: impl Into<String>, e: std::io::Error) -> CoreError {
    CoreError::Io { path: path.into(), source: e }
}
