#![cfg(feature = "s3")]

use crate::config::S3Config;
use crate::error::{Result, ScanStationError};
use async_trait::async_trait;
use hmac::{Hmac, Mac};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::path::Path;

type HmacSha256 = Hmac<Sha256>;

pub struct S3Backend {
    client: Client,
    bucket: String,
    region: String,
    prefix: Option<String>,
    endpoint: Option<String>,
    access_key: String,
    secret_key: String,
}

impl S3Backend {
    pub fn new(config: &S3Config) -> Result<Self> {
        let access_key = config
            .access_key_id
            .clone()
            .or_else(|| std::env::var("AWS_ACCESS_KEY_ID").ok())
            .ok_or_else(|| ScanStationError::Config("s3: access_key_id is required".into()))?;

        let secret_key = config
            .secret_access_key
            .clone()
            .or_else(|| std::env::var("AWS_SECRET_ACCESS_KEY").ok())
            .ok_or_else(|| ScanStationError::Config("s3: secret_access_key is required".into()))?;

        let client = Client::builder()
            .build()
            .map_err(|e| ScanStationError::Config(format!("s3: failed to build HTTP client: {e}")))?;

        Ok(Self {
            client,
            bucket: config.bucket.clone(),
            region: config.region.clone().unwrap_or_else(|| "us-east-1".into()),
            prefix: config.prefix.clone(),
            endpoint: config.endpoint.clone(),
            access_key,
            secret_key,
        })
    }

    /// Build the full URL for a given object key.
    fn object_url(&self, key: &str) -> String {
        match &self.endpoint {
            // Custom endpoint (MinIO): use path-style
            Some(ep) => format!("{}/{}/{}", ep.trim_end_matches('/'), self.bucket, key),
            // AWS S3: use virtual-hosted style
            None => format!(
                "https://{}.s3.{}.amazonaws.com/{}",
                self.bucket, self.region, key
            ),
        }
    }

    /// Build the host header value.
    fn host(&self) -> String {
        match &self.endpoint {
            Some(ep) => {
                // Extract host from endpoint URL
                ep.trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .trim_end_matches('/')
                    .to_owned()
            }
            None => format!("{}.s3.{}.amazonaws.com", self.bucket, self.region),
        }
    }

    /// Derive the S3 object key for a file.
    fn object_key(&self, filename: &str) -> String {
        match &self.prefix {
            Some(prefix) => format!("{}{}", prefix.trim_end_matches('/').to_owned() + "/", filename),
            None => filename.to_owned(),
        }
    }
}

#[async_trait]
impl super::Backend for S3Backend {
    fn name(&self) -> &str {
        "s3"
    }

    async fn upload(&self, path: &Path) -> Result<()> {
        let file_bytes = tokio::fs::read(path).await?;
        let filename = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let key = self.object_key(&filename);
        let url = self.object_url(&key);
        let host = self.host();

        tracing::debug!(url = %url, key = %key, "uploading to s3");

        let now = chrono::Utc::now();
        let date_stamp = now.format("%Y%m%d").to_string();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();

        let payload_hash = hex::encode(Sha256::digest(&file_bytes));

        // Build the signed request
        let signed = sign_request(
            "PUT",
            &key,
            &host,
            &amz_date,
            &date_stamp,
            &payload_hash,
            &self.region,
            &self.access_key,
            &self.secret_key,
        );

        let resp = self
            .client
            .put(&url)
            .header("Host", &host)
            .header("x-amz-date", &amz_date)
            .header("x-amz-content-sha256", &payload_hash)
            .header("Content-Type", "application/pdf")
            .header("Authorization", &signed)
            .body(file_bytes)
            .send()
            .await
            .map_err(|e| ScanStationError::Upload {
                backend: "s3".into(),
                source: Box::new(e),
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ScanStationError::Upload {
                backend: "s3".into(),
                source: format!("HTTP {status}: {body}").into(),
            });
        }

        Ok(())
    }

    async fn validate(&self) -> Result<()> {
        let url = match &self.endpoint {
            Some(ep) => format!("{}/{}", ep.trim_end_matches('/'), self.bucket),
            None => format!(
                "https://{}.s3.{}.amazonaws.com",
                self.bucket, self.region
            ),
        };
        let host = self.host();
        let now = chrono::Utc::now();
        let date_stamp = now.format("%Y%m%d").to_string();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let payload_hash = hex::encode(Sha256::digest(b""));

        let signed = sign_request(
            "HEAD",
            "",
            &host,
            &amz_date,
            &date_stamp,
            &payload_hash,
            &self.region,
            &self.access_key,
            &self.secret_key,
        );

        let resp = self
            .client
            .head(&url)
            .header("Host", &host)
            .header("x-amz-date", &amz_date)
            .header("x-amz-content-sha256", &payload_hash)
            .header("Authorization", &signed)
            .send()
            .await
            .map_err(|e| ScanStationError::Config(format!("s3: cannot reach bucket: {e}")))?;

        if !resp.status().is_success() {
            return Err(ScanStationError::Config(format!(
                "s3: bucket '{}' not accessible (HTTP {})",
                self.bucket,
                resp.status()
            )));
        }

        Ok(())
    }
}

// --- AWS SigV4 Signing ---

/// Sign an S3 request using AWS Signature Version 4.
fn sign_request(
    method: &str,
    key: &str,
    host: &str,
    amz_date: &str,
    date_stamp: &str,
    payload_hash: &str,
    region: &str,
    access_key: &str,
    secret_key: &str,
) -> String {
    let scope = format!("{date_stamp}/{region}/s3/aws4_request");

    // Canonical URI: must start with /
    let canonical_uri = if key.is_empty() {
        "/".to_owned()
    } else {
        format!("/{key}")
    };

    // Canonical headers (sorted by name)
    let canonical_headers = format!(
        "content-type:application/pdf\nhost:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n"
    );
    let signed_headers = "content-type;host;x-amz-content-sha256;x-amz-date";

    // Canonical request
    let canonical_request = format!(
        "{method}\n{canonical_uri}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
    );

    // String to sign
    let canonical_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));
    let string_to_sign = format!("AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{canonical_hash}");

    // Signing key
    let signing_key = derive_signing_key(secret_key, date_stamp, region, "s3");

    // Signature
    let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));

    format!(
        "AWS4-HMAC-SHA256 Credential={access_key}/{scope}, SignedHeaders={signed_headers}, Signature={signature}"
    )
}

/// Derive the AWS SigV4 signing key via HMAC chain.
fn derive_signing_key(secret: &str, date: &str, region: &str, service: &str) -> Vec<u8> {
    let k_date = hmac_sha256(format!("AWS4{secret}").as_bytes(), date.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    hmac_sha256(&k_service, b"aws4_request")
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::Backend;
    use crate::config::RetryConfig;
    use wiremock::matchers::{header_exists, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_config(endpoint: &str) -> S3Config {
        S3Config {
            bucket: "test-bucket".into(),
            region: Some("us-east-1".into()),
            endpoint: Some(endpoint.into()),
            prefix: Some("scans/".into()),
            access_key_id: Some("AKIAIOSFODNN7EXAMPLE".into()),
            secret_access_key: Some("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into()),
            retry: RetryConfig::default(),
        }
    }

    #[tokio::test]
    async fn uploads_with_sigv4_auth() {
        let server = MockServer::start().await;

        Mock::given(method("PUT"))
            .and(header_exists("Authorization"))
            .and(header_exists("x-amz-date"))
            .and(header_exists("x-amz-content-sha256"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let backend = S3Backend::new(&test_config(&server.uri())).unwrap();

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"fake pdf").unwrap();

        backend.upload(tmp.path()).await.unwrap();
    }

    #[tokio::test]
    async fn auth_header_has_correct_format() {
        let server = MockServer::start().await;

        Mock::given(method("PUT"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let backend = S3Backend::new(&test_config(&server.uri())).unwrap();

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"test").unwrap();

        backend.upload(tmp.path()).await.unwrap();

        let received = server.received_requests().await.unwrap();
        assert_eq!(received.len(), 1);

        let auth = received[0]
            .headers
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(auth.starts_with("AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/"), "got: {auth}");
        assert!(auth.contains("SignedHeaders="), "got: {auth}");
        assert!(auth.contains("Signature="), "got: {auth}");
    }

    #[tokio::test]
    async fn object_key_includes_prefix() {
        let config = test_config("http://localhost");
        let backend = S3Backend::new(&config).unwrap();
        assert_eq!(backend.object_key("test.pdf"), "scans/test.pdf");
    }

    #[tokio::test]
    async fn object_key_without_prefix() {
        let mut config = test_config("http://localhost");
        config.prefix = None;
        let backend = S3Backend::new(&config).unwrap();
        assert_eq!(backend.object_key("test.pdf"), "test.pdf");
    }

    #[tokio::test]
    async fn returns_error_on_403() {
        let server = MockServer::start().await;

        Mock::given(method("PUT"))
            .respond_with(ResponseTemplate::new(403).set_body_string("access denied"))
            .mount(&server)
            .await;

        let backend = S3Backend::new(&test_config(&server.uri())).unwrap();

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"data").unwrap();

        let err = backend.upload(tmp.path()).await.unwrap_err();
        assert!(err.to_string().contains("403"), "got: {err}");
    }

    #[test]
    fn sigv4_signing_is_deterministic() {
        let sig1 = sign_request(
            "PUT", "test.pdf", "bucket.s3.us-east-1.amazonaws.com",
            "20250101T000000Z", "20250101",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "us-east-1", "AKID", "SECRET",
        );
        let sig2 = sign_request(
            "PUT", "test.pdf", "bucket.s3.us-east-1.amazonaws.com",
            "20250101T000000Z", "20250101",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "us-east-1", "AKID", "SECRET",
        );
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn sigv4_changes_with_key() {
        let sig1 = sign_request(
            "PUT", "test.pdf", "host",
            "20250101T000000Z", "20250101", "hash",
            "us-east-1", "KEY1", "SECRET",
        );
        let sig2 = sign_request(
            "PUT", "test.pdf", "host",
            "20250101T000000Z", "20250101", "hash",
            "us-east-1", "KEY2", "SECRET",
        );
        assert_ne!(sig1, sig2);
    }
}
