#![cfg(feature = "paperless")]

use crate::config::PaperlessConfig;
use crate::error::{Result, ScanStationError};
use async_trait::async_trait;
use reqwest::Client;
use std::path::Path;

pub struct PaperlessBackend {
    client: Client,
    api_url: String,
    token: String,
    title: Option<String>,
    correspondent: Option<String>,
    document_type: Option<String>,
    tags: Option<Vec<String>>,
}

impl PaperlessBackend {
    pub fn new(config: &PaperlessConfig) -> Result<Self> {
        let client = Client::builder()
            .build()
            .map_err(|e| ScanStationError::Config(format!("paperless: failed to build HTTP client: {e}")))?;

        Ok(Self {
            client,
            api_url: config.api_url.trim_end_matches('/').to_owned(),
            token: config.token.clone(),
            title: config.title.clone(),
            correspondent: config.correspondent.clone(),
            document_type: config.document_type.clone(),
            tags: config.tags.clone(),
        })
    }

    fn endpoint(&self) -> String {
        format!("{}/api/documents/post_document/", self.api_url)
    }
}

#[async_trait]
impl super::Backend for PaperlessBackend {
    fn name(&self) -> &str {
        "paperless"
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
                backend: "paperless".into(),
                source: format!("mime error: {e}").into(),
            })?;

        let mut form = reqwest::multipart::Form::new().part("document", file_part);

        if let Some(ref title) = self.title {
            form = form.text("title", title.clone());
        }
        if let Some(ref correspondent) = self.correspondent {
            form = form.text("correspondent", correspondent.clone());
        }
        if let Some(ref doc_type) = self.document_type {
            form = form.text("document_type", doc_type.clone());
        }
        if let Some(ref tags) = self.tags {
            for tag in tags {
                form = form.text("tags", tag.clone());
            }
        }

        let url = self.endpoint();
        tracing::debug!(url = %url, "uploading to paperless-ngx");

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Token {}", self.token))
            .multipart(form)
            .send()
            .await
            .map_err(|e| ScanStationError::Upload {
                backend: "paperless".into(),
                source: Box::new(e),
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ScanStationError::Upload {
                backend: "paperless".into(),
                source: format!("HTTP {status}: {body}").into(),
            });
        }

        Ok(())
    }

    async fn validate(&self) -> Result<()> {
        let url = format!("{}/api/", self.api_url);
        tracing::debug!(url = %url, "validating paperless-ngx connectivity");

        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Token {}", self.token))
            .send()
            .await
            .map_err(|e| ScanStationError::Config(format!("paperless: cannot reach {url}: {e}")))?;

        if resp.status().as_u16() == 401 || resp.status().as_u16() == 403 {
            return Err(ScanStationError::Config(
                format!("paperless: authentication failed ({})", resp.status()),
            ));
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

    fn test_config(url: &str) -> PaperlessConfig {
        PaperlessConfig {
            api_url: url.into(),
            token: "test-token-abc".into(),
            title: Some("Test Scan".into()),
            correspondent: None,
            document_type: None,
            tags: Some(vec!["scan".into(), "test".into()]),
            retry: RetryConfig::default(),
        }
    }

    #[tokio::test]
    async fn uploads_successfully() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path_regex("/api/documents/post_document"))
            .and(header("Authorization", "Token test-token-abc"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;

        let backend = PaperlessBackend::new(&test_config(&server.uri())).unwrap();

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"fake pdf").unwrap();

        backend.upload(tmp.path()).await.unwrap();
    }

    #[tokio::test]
    async fn returns_error_on_403() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(403).set_body_string("forbidden"))
            .mount(&server)
            .await;

        let backend = PaperlessBackend::new(&test_config(&server.uri())).unwrap();

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"fake pdf").unwrap();

        let err = backend.upload(tmp.path()).await.unwrap_err();
        assert!(err.to_string().contains("403"), "got: {err}");
    }

    #[tokio::test]
    async fn validate_detects_auth_failure() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path_regex("/api/"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let backend = PaperlessBackend::new(&test_config(&server.uri())).unwrap();
        let err = backend.validate().await.unwrap_err();
        assert!(err.to_string().contains("401"), "got: {err}");
    }

    #[tokio::test]
    async fn uses_token_auth_not_bearer() {
        let server = MockServer::start().await;

        // Only match if the exact "Token" prefix is used (not "Bearer")
        Mock::given(method("POST"))
            .and(header("Authorization", "Token test-token-abc"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let backend = PaperlessBackend::new(&test_config(&server.uri())).unwrap();

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"fake pdf").unwrap();

        // This will succeed only if the auth header is correct
        backend.upload(tmp.path()).await.unwrap();
    }
}
