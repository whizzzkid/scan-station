#![cfg(feature = "gdrive")]

use crate::config::GoogleDriveConfig;
use crate::error::{Result, ScanStationError};
use async_trait::async_trait;
use std::path::Path;

/// Google Drive upload backend (stub — full implementation in Phase 4b).
pub struct GoogleDriveBackend {
    _folder_id: String,
}

impl GoogleDriveBackend {
    pub fn new(config: &GoogleDriveConfig) -> Result<Self> {
        Ok(Self {
            _folder_id: config.folder_id.clone(),
        })
    }
}

#[async_trait]
impl super::Backend for GoogleDriveBackend {
    fn name(&self) -> &str {
        "google_drive"
    }

    async fn upload(&self, _path: &Path) -> Result<()> {
        Err(ScanStationError::Upload {
            backend: "google_drive".into(),
            source: "not yet implemented".into(),
        })
    }
}
