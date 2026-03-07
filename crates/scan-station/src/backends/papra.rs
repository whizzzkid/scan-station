use crate::config::PapraConfig;
use crate::error::{Result, ScanStationError};
use async_trait::async_trait;
use reqwest::Client;
use std::path::Path;

pub struct PapraBackend {
    client: Client,
    api_url: String,
    api_key: String,
    organization_id: String,
    ocr_language: Option<String>,
}

impl PapraBackend {
    pub fn new(config: &PapraConfig) -> Result<Self> {
        let client = Client::builder().build().map_err(|e| {
            ScanStationError::Config(format!("papra: failed to build HTTP client: {e}"))
        })?;

        Ok(Self {
            client,
            api_url: config.api_url.trim_end_matches('/').to_owned(),
            api_key: config.api_key.clone(),
            organization_id: config.organization_id.clone(),
            ocr_language: config.ocr_language.clone(),
        })
    }

    fn endpoint(&self) -> String {
        format!(
            "{}/api/organizations/{}/documents",
            self.api_url, self.organization_id
        )
    }
}

#[async_trait]
impl super::Backend for PapraBackend {
    fn name(&self) -> &str {
        "papra"
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
                backend: "papra".into(),
                source: format!("mime error: {e}").into(),
            })?;

        let mut form = reqwest::multipart::Form::new().part("file", file_part);

        if let Some(ref lang) = self.ocr_language {
            form = form.text("ocrLanguages", lang.clone());
        }

        let url = self.endpoint();
        tracing::debug!(url = %url, "uploading to papra");

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| ScanStationError::Upload {
                backend: "papra".into(),
                source: Box::new(e),
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ScanStationError::Upload {
                backend: "papra".into(),
                source: format!("HTTP {status}: {body}").into(),
            });
        }

        Ok(())
    }

    async fn validate(&self) -> Result<()> {
        let url = self.endpoint();
        tracing::debug!(url = %url, "validating papra connectivity");

        let resp = self
            .client
            .head(&url)
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(|e| ScanStationError::Config(format!("papra: cannot reach {url}: {e}")))?;

        if !resp.status().is_success() {
            return Err(ScanStationError::Config(format!(
                "papra: validation failed (HTTP {})",
                resp.status()
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::Backend;
    use crate::config::RetryConfig;
    use wiremock::matchers::{header, method, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_config(url: &str) -> PapraConfig {
        PapraConfig {
            api_url: url.into(),
            api_key: "test-key-123".into(),
            organization_id: "org-456".into(),
            ocr_language: Some("eng".into()),
            retry: RetryConfig::default(),
        }
    }

    #[tokio::test]
    async fn uploads_successfully() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path_regex("/api/organizations/org-456/documents"))
            .and(header("Authorization", "Bearer test-key-123"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;

        let backend = PapraBackend::new(&test_config(&server.uri())).unwrap();

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"fake pdf").unwrap();

        backend.upload(tmp.path()).await.unwrap();
    }

    #[tokio::test]
    async fn returns_error_on_401() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let backend = PapraBackend::new(&test_config(&server.uri())).unwrap();

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"fake pdf").unwrap();

        let err = backend.upload(tmp.path()).await.unwrap_err();
        assert!(err.to_string().contains("401"), "got: {err}");
    }

    #[tokio::test]
    async fn returns_error_on_500() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
            .mount(&server)
            .await;

        let backend = PapraBackend::new(&test_config(&server.uri())).unwrap();

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"fake pdf").unwrap();

        let err = backend.upload(tmp.path()).await.unwrap_err();
        assert!(err.to_string().contains("500"), "got: {err}");
    }

    #[tokio::test]
    async fn validate_detects_auth_failure() {
        let server = MockServer::start().await;

        Mock::given(method("HEAD"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let backend = PapraBackend::new(&test_config(&server.uri())).unwrap();
        let err = backend.validate().await.unwrap_err();
        assert!(err.to_string().contains("401"), "got: {err}");
    }
}
