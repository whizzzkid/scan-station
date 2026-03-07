pub mod filesystem;

#[cfg(feature = "papra")]
pub mod papra;

#[cfg(feature = "paperless")]
pub mod paperless;

#[cfg(feature = "s3")]
pub mod s3;

#[cfg(feature = "gdrive")]
pub mod gdrive;

use crate::config::{BackendConfig, RetryConfig};
use crate::error::{Result, ScanStationError};
use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Result of a single backend upload attempt.
#[derive(Debug)]
pub struct UploadResult {
    pub backend_name: String,
    pub success: bool,
    pub message: String,
    pub duration: Duration,
}

/// All upload backends implement this trait.
#[async_trait]
pub trait Backend: Send + Sync {
    /// Human-readable name of this backend.
    fn name(&self) -> &str;

    /// Upload a file to this backend.
    async fn upload(&self, path: &Path) -> Result<()>;

    /// Validate configuration and connectivity.
    /// Called once at startup or by the `validate` subcommand.
    async fn validate(&self) -> Result<()> {
        Ok(())
    }
}

/// Orchestrates uploads to multiple backends in parallel.
pub struct Uploader {
    backends: Vec<Arc<dyn Backend>>,
}

impl Uploader {
    /// Construct an Uploader from a list of backend configurations.
    pub fn from_config(configs: &[BackendConfig]) -> Result<Self> {
        let mut backends: Vec<Arc<dyn Backend>> = Vec::with_capacity(configs.len());
        for config in configs {
            backends.push(create_backend(config)?);
        }
        Ok(Self { backends })
    }

    /// Access the underlying backends (for validation).
    pub fn backends(&self) -> &[Arc<dyn Backend>] {
        &self.backends
    }

    /// Upload a file to all configured backends in parallel.
    ///
    /// Returns a result for every backend. One failure does not block others.
    pub async fn upload_all(&self, path: &Path, retry_configs: &[RetryConfig]) -> Vec<UploadResult> {
        let mut set = tokio::task::JoinSet::new();

        for (i, backend) in self.backends.iter().enumerate() {
            let backend = Arc::clone(backend);
            let path = path.to_owned();
            let retry = retry_configs.get(i).cloned().unwrap_or_default();

            set.spawn(async move {
                let start = Instant::now();
                let result = retry_upload(&*backend, &path, &retry).await;
                let duration = start.elapsed();

                UploadResult {
                    backend_name: backend.name().to_owned(),
                    success: result.is_ok(),
                    message: match &result {
                        Ok(()) => "ok".into(),
                        Err(e) => e.to_string(),
                    },
                    duration,
                }
            });
        }

        let mut results = Vec::new();
        while let Some(join_result) = set.join_next().await {
            match join_result {
                Ok(upload_result) => results.push(upload_result),
                Err(e) => results.push(UploadResult {
                    backend_name: "unknown".into(),
                    success: false,
                    message: format!("task panicked: {e}"),
                    duration: Duration::ZERO,
                }),
            }
        }
        results
    }
}

/// Retry an upload with the given configuration.
async fn retry_upload(backend: &dyn Backend, path: &Path, retry: &RetryConfig) -> Result<()> {
    let mut last_err = None;
    let attempts = retry.max_retries + 1;

    for attempt in 1..=attempts {
        match backend.upload(path).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                tracing::warn!(
                    backend = backend.name(),
                    attempt,
                    total = attempts,
                    "upload failed: {e}"
                );
                last_err = Some(e);
                if attempt < attempts {
                    tokio::time::sleep(Duration::from_millis(retry.retry_delay_ms)).await;
                }
            }
        }
    }

    Err(last_err.unwrap_or_else(|| {
        ScanStationError::Upload {
            backend: backend.name().into(),
            source: "no attempts made".into(),
        }
    }))
}

/// Factory: instantiate a backend from its configuration.
fn create_backend(config: &BackendConfig) -> Result<Arc<dyn Backend>> {
    match config {
        BackendConfig::Filesystem(c) => Ok(Arc::new(filesystem::FilesystemBackend::new(c))),

        #[cfg(feature = "papra")]
        BackendConfig::Papra(c) => Ok(Arc::new(papra::PapraBackend::new(c)?)),

        #[cfg(feature = "paperless")]
        BackendConfig::Paperless(c) => Ok(Arc::new(paperless::PaperlessBackend::new(c)?)),

        #[cfg(feature = "s3")]
        BackendConfig::S3(c) => Ok(Arc::new(s3::S3Backend::new(c)?)),

        #[cfg(feature = "gdrive")]
        BackendConfig::GoogleDrive(c) => Ok(Arc::new(gdrive::GoogleDriveBackend::new(c)?)),
    }
}

/// Extract retry configs from backend configs (in the same order).
pub fn extract_retry_configs(configs: &[BackendConfig]) -> Vec<RetryConfig> {
    configs
        .iter()
        .map(|c| match c {
            BackendConfig::Filesystem(fc) => fc.retry.clone(),
            #[cfg(feature = "papra")]
            BackendConfig::Papra(pc) => pc.retry.clone(),
            #[cfg(feature = "paperless")]
            BackendConfig::Paperless(pc) => pc.retry.clone(),
            #[cfg(feature = "s3")]
            BackendConfig::S3(sc) => sc.retry.clone(),
            #[cfg(feature = "gdrive")]
            BackendConfig::GoogleDrive(gc) => gc.retry.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct MockBackend {
        name: String,
        should_fail: bool,
        call_count: Arc<AtomicU32>,
    }

    #[async_trait]
    impl Backend for MockBackend {
        fn name(&self) -> &str {
            &self.name
        }

        async fn upload(&self, _path: &Path) -> Result<()> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            if self.should_fail {
                Err(ScanStationError::Upload {
                    backend: self.name.clone(),
                    source: "mock failure".into(),
                })
            } else {
                Ok(())
            }
        }
    }

    #[tokio::test]
    async fn uploads_to_all_backends() {
        let count_a = Arc::new(AtomicU32::new(0));
        let count_b = Arc::new(AtomicU32::new(0));

        let uploader = Uploader {
            backends: vec![
                Arc::new(MockBackend {
                    name: "a".into(),
                    should_fail: false,
                    call_count: Arc::clone(&count_a),
                }),
                Arc::new(MockBackend {
                    name: "b".into(),
                    should_fail: false,
                    call_count: Arc::clone(&count_b),
                }),
            ],
        };

        let retries = vec![RetryConfig::default(); 2];
        let results = uploader.upload_all(Path::new("/fake.pdf"), &retries).await;

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.success));
        assert_eq!(count_a.load(Ordering::SeqCst), 1);
        assert_eq!(count_b.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn one_failure_doesnt_block_others() {
        let count_ok = Arc::new(AtomicU32::new(0));
        let count_fail = Arc::new(AtomicU32::new(0));

        let uploader = Uploader {
            backends: vec![
                Arc::new(MockBackend {
                    name: "ok".into(),
                    should_fail: false,
                    call_count: Arc::clone(&count_ok),
                }),
                Arc::new(MockBackend {
                    name: "fail".into(),
                    should_fail: true,
                    call_count: Arc::clone(&count_fail),
                }),
            ],
        };

        let retries = vec![RetryConfig::default(); 2];
        let results = uploader.upload_all(Path::new("/fake.pdf"), &retries).await;

        assert_eq!(results.len(), 2);
        let ok_result = results.iter().find(|r| r.backend_name == "ok").unwrap();
        let fail_result = results.iter().find(|r| r.backend_name == "fail").unwrap();
        assert!(ok_result.success);
        assert!(!fail_result.success);
    }

    #[tokio::test]
    async fn retries_on_failure() {
        let count = Arc::new(AtomicU32::new(0));

        let uploader = Uploader {
            backends: vec![Arc::new(MockBackend {
                name: "retry_test".into(),
                should_fail: true,
                call_count: Arc::clone(&count),
            })],
        };

        let retries = vec![RetryConfig {
            max_retries: 2,
            retry_delay_ms: 1, // 1ms for fast tests
        }];
        let results = uploader.upload_all(Path::new("/fake.pdf"), &retries).await;

        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        // 1 initial + 2 retries = 3 total calls
        assert_eq!(count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn empty_backends_returns_empty() {
        let uploader = Uploader {
            backends: vec![],
        };
        let results = uploader.upload_all(Path::new("/fake.pdf"), &[]).await;
        assert!(results.is_empty());
    }
}
