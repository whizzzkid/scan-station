use crate::config::FilesystemConfig;
use crate::error::{Result, ScanStationError};
use async_trait::async_trait;
use std::path::{Path, PathBuf};

/// Backend that copies scanned files to a local directory.
pub struct FilesystemBackend {
    path: PathBuf,
}

impl FilesystemBackend {
    pub fn new(config: &FilesystemConfig) -> Self {
        Self {
            path: config.path.clone(),
        }
    }
}

#[async_trait]
impl super::Backend for FilesystemBackend {
    fn name(&self) -> &str {
        "filesystem"
    }

    async fn upload(&self, src: &Path) -> Result<()> {
        let filename = src
            .file_name()
            .ok_or_else(|| ScanStationError::Scan("source path has no filename".into()))?;

        let dest = self.path.join(filename);

        tracing::info!(
            src = %src.display(),
            dest = %dest.display(),
            "copying file"
        );

        tokio::fs::copy(src, &dest)
            .await
            .map_err(|e| ScanStationError::Upload {
                backend: "filesystem".into(),
                source: format!("copy to {}: {e}", dest.display()).into(),
            })?;

        Ok(())
    }

    async fn validate(&self) -> Result<()> {
        tokio::fs::create_dir_all(&self.path).await.map_err(|e| {
            ScanStationError::Config(format!(
                "filesystem backend: cannot create {}: {e}",
                self.path.display()
            ))
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::Backend;
    use crate::config::RetryConfig;

    #[tokio::test]
    async fn copies_file_to_target() {
        let tmp = tempfile::tempdir().unwrap();
        let src_dir = tempfile::tempdir().unwrap();

        // Create a source file
        let src = src_dir.path().join("test.pdf");
        tokio::fs::write(&src, b"fake pdf content").await.unwrap();

        let backend = FilesystemBackend::new(&FilesystemConfig {
            path: tmp.path().to_path_buf(),
            retry: RetryConfig::default(),
        });

        backend.upload(&src).await.unwrap();

        let dest = tmp.path().join("test.pdf");
        assert!(dest.exists());
        assert_eq!(
            tokio::fs::read_to_string(&dest).await.unwrap(),
            "fake pdf content"
        );
    }

    #[tokio::test]
    async fn validate_creates_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("subdir").join("deep");

        let backend = FilesystemBackend::new(&FilesystemConfig {
            path: target.clone(),
            retry: RetryConfig::default(),
        });

        assert!(!target.exists());
        backend.validate().await.unwrap();
        assert!(target.exists());
    }

    #[tokio::test]
    async fn upload_fails_if_dir_missing() {
        let backend = FilesystemBackend::new(&FilesystemConfig {
            path: PathBuf::from("/nonexistent/path/that/should/fail"),
            retry: RetryConfig::default(),
        });

        let src_dir = tempfile::tempdir().unwrap();
        let src = src_dir.path().join("test.pdf");
        tokio::fs::write(&src, b"data").await.unwrap();

        let result = backend.upload(&src).await;
        assert!(result.is_err());
    }
}
