use std::path::PathBuf;

/// All errors produced by the scan-station library.
#[derive(thiserror::Error, Debug)]
pub enum ScanStationError {
    #[error("config: {0}")]
    Config(String),

    #[error("scan failed: {0}")]
    Scan(String),

    #[error("upload to '{backend}': {source}")]
    Upload {
        backend: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("file not found: {0}")]
    FileNotFound(PathBuf),

    #[error("{0}")]
    Io(#[from] std::io::Error),

    #[error("command failed (exit {code:?}): {stderr}")]
    Command {
        code: Option<i32>,
        stderr: String,
    },
}

pub type Result<T> = std::result::Result<T, ScanStationError>;
