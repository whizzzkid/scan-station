use crate::error::{Result, ScanStationError};
use regex::Regex;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Top-level application configuration.
#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    #[serde(default)]
    pub scan: ScanConfig,

    #[serde(default)]
    pub backends: Vec<BackendConfig>,
}

/// Configuration for the scan operation.
#[derive(Debug, Deserialize, Clone)]
pub struct ScanConfig {
    #[serde(default = "default_vendor")]
    pub vendor: String,

    #[serde(default = "default_dpi")]
    pub dpi: u32,

    #[serde(default = "default_mode")]
    pub mode: String,

    #[serde(default = "default_file_prefix")]
    pub file_prefix: String,

    #[serde(default = "default_date_format")]
    pub date_format: String,

    #[serde(default = "default_scan_program")]
    pub scan_program: String,

    #[serde(default = "default_tmp_dir")]
    pub tmp_dir: PathBuf,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            vendor: default_vendor(),
            dpi: default_dpi(),
            mode: default_mode(),
            file_prefix: default_file_prefix(),
            date_format: default_date_format(),
            scan_program: default_scan_program(),
            tmp_dir: default_tmp_dir(),
        }
    }
}

fn default_vendor() -> String {
    "fujitsu".into()
}
fn default_dpi() -> u32 {
    300
}
fn default_mode() -> String {
    "Color".into()
}
fn default_file_prefix() -> String {
    "scan".into()
}
fn default_date_format() -> String {
    "%Y-%m-%d-%H%M%S-%3N-%Z".into()
}
fn default_scan_program() -> String {
    "/app/sane-scan-pdf/scan".into()
}
fn default_tmp_dir() -> PathBuf {
    PathBuf::from("/tmp/scan-station")
}

/// Per-backend retry configuration.
#[derive(Debug, Deserialize, Clone)]
pub struct RetryConfig {
    #[serde(default)]
    pub max_retries: u32,

    #[serde(default = "default_retry_delay_ms")]
    pub retry_delay_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 0,
            retry_delay_ms: 1000,
        }
    }
}

fn default_retry_delay_ms() -> u64 {
    1000
}

/// Discriminated union of backend configurations.
///
/// Each variant is gated by its compile-time feature flag.
/// The `type` field in TOML selects which variant to deserialize.
#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BackendConfig {
    Filesystem(FilesystemConfig),

    #[cfg(feature = "papra")]
    Papra(PapraConfig),

    #[cfg(feature = "paperless")]
    Paperless(PaperlessConfig),

    #[cfg(feature = "s3")]
    S3(S3Config),

    #[cfg(feature = "gdrive")]
    GoogleDrive(GoogleDriveConfig),
}

#[derive(Debug, Deserialize, Clone)]
pub struct FilesystemConfig {
    pub path: PathBuf,

    #[serde(default)]
    pub retry: RetryConfig,
}

#[cfg(feature = "papra")]
#[derive(Debug, Deserialize, Clone)]
pub struct PapraConfig {
    pub api_url: String,
    pub api_key: String,
    pub organization_id: String,
    pub ocr_language: Option<String>,

    #[serde(default)]
    pub retry: RetryConfig,
}

#[cfg(feature = "paperless")]
#[derive(Debug, Deserialize, Clone)]
pub struct PaperlessConfig {
    pub api_url: String,
    pub token: String,
    pub title: Option<String>,
    pub correspondent: Option<String>,
    pub document_type: Option<String>,
    pub tags: Option<Vec<String>>,

    #[serde(default)]
    pub retry: RetryConfig,
}

#[cfg(feature = "s3")]
#[derive(Debug, Deserialize, Clone)]
pub struct S3Config {
    pub bucket: String,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    pub prefix: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,

    #[serde(default)]
    pub retry: RetryConfig,
}

#[cfg(feature = "gdrive")]
#[derive(Debug, Deserialize, Clone)]
pub struct GoogleDriveConfig {
    pub credentials_file: PathBuf,
    pub folder_id: String,

    #[serde(default)]
    pub retry: RetryConfig,
}

/// Substitute `${VAR_NAME}` patterns in a string with environment variable values.
fn substitute_env_vars(input: &str) -> Result<String> {
    let re = Regex::new(r"\$\{([^}]+)\}").expect("valid regex");
    let mut result = input.to_owned();
    let mut errors = Vec::new();

    for cap in re.captures_iter(input) {
        let full_match = &cap[0];
        let var_name = &cap[1];
        match std::env::var(var_name) {
            Ok(value) => {
                result = result.replace(full_match, &value);
            }
            Err(_) => {
                errors.push(var_name.to_owned());
            }
        }
    }

    if !errors.is_empty() {
        return Err(ScanStationError::Config(format!(
            "missing environment variables: {}",
            errors.join(", ")
        )));
    }

    Ok(result)
}

/// Load configuration from a TOML file with `${ENV_VAR}` substitution.
pub fn load(path: &Path) -> Result<AppConfig> {
    let raw = std::fs::read_to_string(path).map_err(|e| {
        ScanStationError::Config(format!("failed to read {}: {e}", path.display()))
    })?;

    let substituted = substitute_env_vars(&raw)?;

    let config: AppConfig = toml::from_str(&substituted).map_err(|e| {
        ScanStationError::Config(format!("failed to parse {}: {e}", path.display()))
    })?;

    Ok(config)
}

/// Load configuration from a TOML string (useful for tests).
pub fn load_from_str(toml_str: &str) -> Result<AppConfig> {
    let substituted = substitute_env_vars(toml_str)?;
    let config: AppConfig = toml::from_str(&substituted)
        .map_err(|e| ScanStationError::Config(format!("failed to parse config: {e}")))?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_config() {
        let config = load_from_str("").unwrap();
        assert_eq!(config.scan.vendor, "fujitsu");
        assert_eq!(config.scan.dpi, 300);
        assert_eq!(config.scan.mode, "Color");
        assert!(config.backends.is_empty());
    }

    #[test]
    fn parses_scan_config() {
        let toml = r#"
[scan]
vendor = "canon"
dpi = 600
mode = "Gray"
file_prefix = "doc"
"#;
        let config = load_from_str(toml).unwrap();
        assert_eq!(config.scan.vendor, "canon");
        assert_eq!(config.scan.dpi, 600);
        assert_eq!(config.scan.mode, "Gray");
        assert_eq!(config.scan.file_prefix, "doc");
    }

    #[test]
    fn parses_filesystem_backend() {
        let toml = r#"
[[backends]]
type = "filesystem"
path = "/scans"
"#;
        let config = load_from_str(toml).unwrap();
        assert_eq!(config.backends.len(), 1);
        match &config.backends[0] {
            BackendConfig::Filesystem(fs) => {
                assert_eq!(fs.path, PathBuf::from("/scans"));
                assert_eq!(fs.retry.max_retries, 0);
            }
            #[allow(unreachable_patterns)]
            _ => panic!("expected filesystem backend"),
        }
    }

    #[test]
    fn parses_filesystem_with_retry() {
        let toml = r#"
[[backends]]
type = "filesystem"
path = "/scans"
[backends.retry]
max_retries = 3
retry_delay_ms = 2000
"#;
        let config = load_from_str(toml).unwrap();
        match &config.backends[0] {
            BackendConfig::Filesystem(fs) => {
                assert_eq!(fs.retry.max_retries, 3);
                assert_eq!(fs.retry.retry_delay_ms, 2000);
            }
            #[allow(unreachable_patterns)]
            _ => panic!("expected filesystem backend"),
        }
    }

    #[test]
    fn multiple_filesystem_backends() {
        let toml = r#"
[[backends]]
type = "filesystem"
path = "/scans"

[[backends]]
type = "filesystem"
path = "/backup"
"#;
        let config = load_from_str(toml).unwrap();
        assert_eq!(config.backends.len(), 2);
    }

    #[test]
    fn env_substitution_works() {
        unsafe { std::env::set_var("TEST_SCAN_STATION_KEY", "secret123") };
        let toml = r#"
[[backends]]
type = "filesystem"
path = "${TEST_SCAN_STATION_KEY}/scans"
"#;
        let config = load_from_str(toml).unwrap();
        match &config.backends[0] {
            BackendConfig::Filesystem(fs) => {
                assert_eq!(fs.path, PathBuf::from("secret123/scans"));
            }
            #[allow(unreachable_patterns)]
            _ => panic!("expected filesystem backend"),
        }
        unsafe { std::env::remove_var("TEST_SCAN_STATION_KEY") };
    }

    #[test]
    fn missing_env_var_returns_error() {
        let toml = r#"
[[backends]]
type = "filesystem"
path = "${DEFINITELY_DOES_NOT_EXIST_XYZ}/scans"
"#;
        let err = load_from_str(toml).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("DEFINITELY_DOES_NOT_EXIST_XYZ"), "got: {msg}");
    }

    #[test]
    fn invalid_toml_returns_error() {
        let err = load_from_str("[[invalid").unwrap_err();
        assert!(err.to_string().contains("failed to parse"), "got: {err}");
    }

    #[test]
    fn unknown_backend_type_returns_error() {
        let toml = r#"
[[backends]]
type = "nonexistent"
path = "/scans"
"#;
        let err = load_from_str(toml).unwrap_err();
        assert!(err.to_string().contains("failed to parse"), "got: {err}");
    }
}
