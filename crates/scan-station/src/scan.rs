use crate::config::ScanConfig;
use crate::error::{Result, ScanStationError};
use std::path::PathBuf;
use tokio::process::Command;

/// Generate a timestamped filename for a scan.
pub fn generate_filename(config: &ScanConfig) -> String {
    let now = chrono::Local::now().format(&config.date_format);
    format!("{}-{}.pdf", config.file_prefix, now)
}

/// Run a scan using sane-scan-pdf and return the path to the produced PDF.
///
/// The PDF is saved to `config.tmp_dir/{filename}`. It is the caller's
/// responsibility to upload/move the file via configured backends.
pub async fn run(config: &ScanConfig) -> Result<PathBuf> {
    let filename = generate_filename(config);

    tokio::fs::create_dir_all(&config.tmp_dir).await?;
    let tmp_file = config.tmp_dir.join(&filename);

    tracing::info!(
        program = %config.scan_program,
        vendor = %config.vendor,
        dpi = config.dpi,
        mode = %config.mode,
        output = %tmp_file.display(),
        "starting scan"
    );

    let output = Command::new(&config.scan_program)
        .args([
            "-v",
            "-d",
            "-x",
            &config.vendor,
            "-r",
            &config.dpi.to_string(),
            "--mode",
            &config.mode,
            "--skip-empty-pages",
            "--crop",
            "-o",
            &tmp_file.to_string_lossy(),
        ])
        .output()
        .await
        .map_err(|e| {
            ScanStationError::Scan(format!("failed to execute {}: {e}", config.scan_program))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ScanStationError::Command {
            code: output.status.code(),
            stderr: stderr.into_owned(),
        });
    }

    tracing::info!(output = %tmp_file.display(), "scan completed");
    Ok(tmp_file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_has_prefix_and_extension() {
        let config = ScanConfig {
            file_prefix: "doc".into(),
            ..ScanConfig::default()
        };
        let name = generate_filename(&config);
        assert!(name.starts_with("doc-"), "got: {name}");
        assert!(name.ends_with(".pdf"), "got: {name}");
    }

    #[test]
    fn filename_changes_with_prefix() {
        let config_a = ScanConfig {
            file_prefix: "invoice".into(),
            ..ScanConfig::default()
        };
        let config_b = ScanConfig {
            file_prefix: "receipt".into(),
            ..ScanConfig::default()
        };
        let name_a = generate_filename(&config_a);
        let name_b = generate_filename(&config_b);
        assert!(name_a.starts_with("invoice-"));
        assert!(name_b.starts_with("receipt-"));
    }

    #[test]
    fn default_config_produces_valid_filename() {
        let config = ScanConfig::default();
        let name = generate_filename(&config);
        assert!(name.starts_with("scan-"));
        assert!(name.ends_with(".pdf"));
        assert!(name.len() > "scan-.pdf".len());
    }
}
