# Adding a New Backend

This guide walks through adding a new upload backend to scan-station.
Every backend is just an HTTP request — no SDKs needed.

## Step 1: Add Feature Flag to Cargo.toml

In `crates/scan-station/Cargo.toml`:

```toml
[features]
# Add your feature (pull in reqwest if you need HTTP)
my_backend = ["dep:reqwest"]

# Add to all-backends
all-backends = ["filesystem", "http-backends", "cloud-backends", "my_backend"]

[dependencies]
# If you need additional optional crates:
# my_crate = { version = "1", optional = true }
```

## Step 2: Add Config Struct

In `crates/scan-station/src/config.rs`:

```rust
// Add to the BackendConfig enum:
#[cfg(feature = "my_backend")]
MyBackend(MyBackendConfig),

// Add the config struct:
#[cfg(feature = "my_backend")]
#[derive(Debug, Deserialize, Clone)]
pub struct MyBackendConfig {
    pub api_url: String,
    pub api_key: String,
    // Add fields as needed. Use Option<T> for optional fields.

    #[serde(default)]
    pub retry: RetryConfig,
}
```

## Step 3: Create Backend Implementation

Create `crates/scan-station/src/backends/my_backend.rs`:

```rust
#![cfg(feature = "my_backend")]

use crate::config::MyBackendConfig;
use crate::error::{Result, ScanStationError};
use async_trait::async_trait;
use reqwest::Client;
use std::path::Path;

pub struct MyBackend {
    client: Client,
    api_url: String,
    api_key: String,
}

impl MyBackend {
    pub fn new(config: &MyBackendConfig) -> Result<Self> {
        let client = Client::builder()
            .build()
            .map_err(|e| ScanStationError::Config(
                format!("my_backend: failed to build HTTP client: {e}")
            ))?;

        Ok(Self {
            client,
            api_url: config.api_url.trim_end_matches('/').to_owned(),
            api_key: config.api_key.clone(),
        })
    }
}

#[async_trait]
impl super::Backend for MyBackend {
    fn name(&self) -> &str {
        "my_backend"
    }

    async fn upload(&self, path: &Path) -> Result<()> {
        let file_bytes = tokio::fs::read(path).await?;
        let file_name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let file_part = reqwest::multipart::Part::bytes(file_bytes)
            .file_name(file_name)
            .mime_str("application/pdf")
            .map_err(|e| ScanStationError::Upload {
                backend: "my_backend".into(),
                source: format!("mime error: {e}").into(),
            })?;

        let form = reqwest::multipart::Form::new()
            .part("file", file_part);

        let url = format!("{}/upload", self.api_url);

        let resp = self.client
            .post(&url)
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| ScanStationError::Upload {
                backend: "my_backend".into(),
                source: Box::new(e),
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ScanStationError::Upload {
                backend: "my_backend".into(),
                source: format!("HTTP {status}: {body}").into(),
            });
        }

        Ok(())
    }

    async fn validate(&self) -> Result<()> {
        // Optional: test connectivity at startup
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::Backend;
    use crate::config::RetryConfig;
    use wiremock::matchers::{header, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_config(url: &str) -> MyBackendConfig {
        MyBackendConfig {
            api_url: url.into(),
            api_key: "test-key".into(),
            retry: RetryConfig::default(),
        }
    }

    #[tokio::test]
    async fn uploads_successfully() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(header("Authorization", "Bearer test-key"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let backend = MyBackend::new(&test_config(&server.uri())).unwrap();

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"fake pdf").unwrap();

        backend.upload(tmp.path()).await.unwrap();
    }

    #[tokio::test]
    async fn returns_error_on_failure() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let backend = MyBackend::new(&test_config(&server.uri())).unwrap();

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"fake pdf").unwrap();

        let err = backend.upload(tmp.path()).await.unwrap_err();
        assert!(err.to_string().contains("500"));
    }
}
```

## Step 4: Register in Module and Factory

In `crates/scan-station/src/backends/mod.rs`:

```rust
// Add module declaration:
#[cfg(feature = "my_backend")]
pub mod my_backend;

// Add match arm in create_backend():
#[cfg(feature = "my_backend")]
BackendConfig::MyBackend(c) => Ok(Arc::new(my_backend::MyBackend::new(c)?)),

// Add match arm in extract_retry_configs():
#[cfg(feature = "my_backend")]
BackendConfig::MyBackend(c) => c.retry.clone(),
```

## Step 5: Add Example Config

In `config.example.toml`:

```toml
# [[backends]]
# type = "my_backend"
# api_url = "https://my-service.example.com"
# api_key = "${MY_BACKEND_API_KEY}"
# [backends.retry]
# max_retries = 3
# retry_delay_ms = 2000
```

## Step 6: Verify

```sh
# Build with your feature
cargo build -p scan-station --features my_backend

# Run tests
cargo test -p scan-station --features my_backend

# Verify all-backends still works
cargo test -p scan-station --features all-backends
```

## Checklist

- [ ] Feature flag added to `Cargo.toml`
- [ ] Feature added to `all-backends` aggregate
- [ ] Config struct added to `config.rs` with `#[cfg(feature)]`
- [ ] `BackendConfig` enum variant added
- [ ] Backend implementation file created with `#[cfg(feature)]`
- [ ] `Backend` trait implemented (name, upload, optionally validate)
- [ ] Factory match arm added in `backends/mod.rs`
- [ ] Retry config extraction match arm added
- [ ] Tests written (wiremock for HTTP, tempfile for filesystem)
- [ ] Example config added to `config.example.toml`
- [ ] README.md updated
