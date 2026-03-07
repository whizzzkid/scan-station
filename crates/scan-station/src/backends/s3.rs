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

    /// Build the full URL for a given object key (already URI-encoded).
    fn object_url(&self, encoded_key: &str) -> String {
        match &self.endpoint {
            Some(ep) => format!("{}/{}/{}", ep.trim_end_matches('/'), self.bucket, encoded_key),
            None => format!(
                "https://{}.s3.{}.amazonaws.com/{}",
                self.bucket, self.region, encoded_key
            ),
        }
    }

    /// Build the host header value.
    fn host(&self) -> String {
        match &self.endpoint {
            Some(ep) => {
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
            Some(prefix) => format!("{}/{}", prefix.trim_end_matches('/'), filename),
            None => filename.to_owned(),
        }
    }

    /// Canonical URI for path-style validate (HEAD bucket).
    fn validate_canonical_uri(&self) -> String {
        match &self.endpoint {
            Some(_) => format!("/{}", self.bucket),
            None => "/".to_owned(),
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
        let encoded_key = uri_encode_path(&key);
        let url = self.object_url(&encoded_key);
        let host = self.host();

        tracing::debug!(url = %url, key = %key, "uploading to s3");

        let now = chrono::Utc::now();
        let date_stamp = now.format("%Y%m%d").to_string();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();

        let payload_hash = hex::encode(Sha256::digest(&file_bytes));
        let canonical_uri = format!("/{encoded_key}");

        let signed = sign_request(&SignInput {
            method: "PUT",
            canonical_uri: &canonical_uri,
            host: &host,
            amz_date: &amz_date,
            date_stamp: &date_stamp,
            payload_hash: &payload_hash,
            content_type: Some("application/pdf"),
            region: &self.region,
            access_key: &self.access_key,
            secret_key: &self.secret_key,
        });

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
        let canonical_uri = self.validate_canonical_uri();

        // HEAD requests have no content-type
        let signed = sign_request(&SignInput {
            method: "HEAD",
            canonical_uri: &canonical_uri,
            host: &host,
            amz_date: &amz_date,
            date_stamp: &date_stamp,
            payload_hash: &payload_hash,
            content_type: None,
            region: &self.region,
            access_key: &self.access_key,
            secret_key: &self.secret_key,
        });

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

struct SignInput<'a> {
    method: &'a str,
    canonical_uri: &'a str,
    host: &'a str,
    amz_date: &'a str,
    date_stamp: &'a str,
    payload_hash: &'a str,
    content_type: Option<&'a str>,
    region: &'a str,
    access_key: &'a str,
    secret_key: &'a str,
}

/// Sign an S3 request using AWS Signature Version 4.
fn sign_request(input: &SignInput<'_>) -> String {
    let scope = format!("{}/{}/s3/aws4_request", input.date_stamp, input.region);

    // Build canonical headers and signed headers based on what's actually present
    let (canonical_headers, signed_headers) = match input.content_type {
        Some(ct) => (
            format!(
                "content-type:{ct}\nhost:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n",
                input.host, input.payload_hash, input.amz_date
            ),
            "content-type;host;x-amz-content-sha256;x-amz-date".to_owned(),
        ),
        None => (
            format!(
                "host:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n",
                input.host, input.payload_hash, input.amz_date
            ),
            "host;x-amz-content-sha256;x-amz-date".to_owned(),
        ),
    };

    let canonical_request = format!(
        "{}\n{}\n\n{}\n{}\n{}",
        input.method, input.canonical_uri, canonical_headers, signed_headers, input.payload_hash
    );

    let canonical_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        input.amz_date, scope, canonical_hash
    );

    let signing_key = derive_signing_key(input.secret_key, input.date_stamp, input.region, "s3");
    let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));

    format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        input.access_key, scope, signed_headers, signature
    )
}

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

/// URI-encode a path (encode each segment, preserve `/` separators).
/// Per AWS SigV4 rules: encode all characters except unreserved (A-Z a-z 0-9 - _ . ~).
fn uri_encode_path(path: &str) -> String {
    path.split('/')
        .map(uri_encode_segment)
        .collect::<Vec<_>>()
        .join("/")
}

fn uri_encode_segment(s: &str) -> String {
    let mut encoded = String::with_capacity(s.len() * 3);
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    encoded
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
        assert!(auth.contains("SignedHeaders=content-type;"), "got: {auth}");
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
    fn uri_encodes_special_characters() {
        assert_eq!(uri_encode_path("scans/my file.pdf"), "scans/my%20file.pdf");
        assert_eq!(uri_encode_path("a+b/c#d"), "a%2Bb/c%23d");
        assert_eq!(uri_encode_path("simple.pdf"), "simple.pdf");
        assert_eq!(uri_encode_path("path/to/file"), "path/to/file");
    }

    #[test]
    fn validate_canonical_uri_path_style() {
        let config = test_config("http://minio.local:9000");
        let backend = S3Backend::new(&config).unwrap();
        assert_eq!(backend.validate_canonical_uri(), "/test-bucket");
    }

    #[test]
    fn validate_canonical_uri_virtual_hosted() {
        let mut config = test_config("http://localhost");
        config.endpoint = None;
        let backend = S3Backend::new(&config).unwrap();
        assert_eq!(backend.validate_canonical_uri(), "/");
    }

    #[test]
    fn sign_request_excludes_content_type_for_head() {
        let sig = sign_request(&SignInput {
            method: "HEAD",
            canonical_uri: "/test-bucket",
            host: "minio.local:9000",
            amz_date: "20250101T000000Z",
            date_stamp: "20250101",
            payload_hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            content_type: None,
            region: "us-east-1",
            access_key: "AKID",
            secret_key: "SECRET",
        });
        assert!(!sig.contains("content-type"), "HEAD sig should not include content-type: {sig}");
        assert!(sig.contains("SignedHeaders=host;x-amz-content-sha256;x-amz-date"), "got: {sig}");
    }

    #[test]
    fn sign_request_includes_content_type_for_put() {
        let sig = sign_request(&SignInput {
            method: "PUT",
            canonical_uri: "/scans/test.pdf",
            host: "bucket.s3.us-east-1.amazonaws.com",
            amz_date: "20250101T000000Z",
            date_stamp: "20250101",
            payload_hash: "abc123",
            content_type: Some("application/pdf"),
            region: "us-east-1",
            access_key: "AKID",
            secret_key: "SECRET",
        });
        assert!(sig.contains("content-type;"), "PUT sig should include content-type: {sig}");
    }

    #[test]
    fn sigv4_signing_is_deterministic() {
        let input = SignInput {
            method: "PUT",
            canonical_uri: "/test.pdf",
            host: "bucket.s3.us-east-1.amazonaws.com",
            amz_date: "20250101T000000Z",
            date_stamp: "20250101",
            payload_hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            content_type: Some("application/pdf"),
            region: "us-east-1",
            access_key: "AKID",
            secret_key: "SECRET",
        };
        let sig1 = sign_request(&input);
        let sig2 = sign_request(&input);
        assert_eq!(sig1, sig2);
    }
}
