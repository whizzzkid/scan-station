mod cli;

use clap::Parser;
use cli::{Cli, Command};
use scan_station::backends::{self, Uploader};
use scan_station::config;
use scan_station::error::ScanStationError;
use std::process::ExitCode;

fn setup_tracing(verbosity: u8) {
    let level = match verbosity {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };

    let filter = format!("scan_station={level},discover_scanner={level}");

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .with_target(false)
        .init();
}

async fn run(cli: Cli) -> Result<(), ScanStationError> {
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        config = %cli.config.display(),
        "scan-station starting"
    );

    let mut app_config = config::load(&cli.config)?;

    match cli.command {
        Command::Scan {
            vendor,
            dpi,
            mode,
            no_upload,
        } => {
            // Apply CLI overrides
            if let Some(v) = vendor {
                app_config.scan.vendor = v;
            }
            if let Some(d) = dpi {
                app_config.scan.dpi = d;
            }
            if let Some(m) = mode {
                app_config.scan.mode = m;
            }

            let pdf_path = scan_station::scan::run(&app_config.scan).await?;
            let file_size = tokio::fs::metadata(&pdf_path)
                .await
                .map(|m| m.len())
                .unwrap_or(0);
            tracing::info!(
                path = %pdf_path.display(),
                size_bytes = file_size,
                "scan produced"
            );

            if no_upload {
                tracing::info!("--no-upload: skipping backend uploads");
                return Ok(());
            }

            let uploader = Uploader::from_config(&app_config.backends)?;
            tracing::info!(
                backends = uploader.backends().len(),
                names = %uploader.backends().iter().map(|b| b.name()).collect::<Vec<_>>().join(", "),
                "uploading to backends"
            );
            let retries = backends::extract_retry_configs(&app_config.backends);
            let results = uploader.upload_all(&pdf_path, &retries).await;

            // Clean up temp file after all backends have consumed it
            if let Err(e) = tokio::fs::remove_file(&pdf_path).await {
                tracing::warn!(path = %pdf_path.display(), "failed to remove temp file: {e}");
            }

            check_results(&results)?;
        }

        Command::Upload { file } => {
            if !file.exists() {
                return Err(ScanStationError::FileNotFound(file));
            }

            tracing::info!(path = %file.display(), "uploading existing file");
            let uploader = Uploader::from_config(&app_config.backends)?;
            tracing::info!(
                backends = uploader.backends().len(),
                names = %uploader.backends().iter().map(|b| b.name()).collect::<Vec<_>>().join(", "),
                "uploading to backends"
            );
            let retries = backends::extract_retry_configs(&app_config.backends);
            let results = uploader.upload_all(&file, &retries).await;
            check_results(&results)?;
        }

        Command::Validate => {
            let uploader = Uploader::from_config(&app_config.backends)?;
            let mut all_ok = true;

            for backend in uploader.backends() {
                match backend.validate().await {
                    Ok(()) => tracing::info!(backend = backend.name(), "ok"),
                    Err(e) => {
                        tracing::error!(backend = backend.name(), "failed: {e}");
                        all_ok = false;
                    }
                }
            }

            if !all_ok {
                return Err(ScanStationError::Config(
                    "one or more backends failed validation".into(),
                ));
            }
        }
    }

    Ok(())
}

/// Log upload results and return error if any backend failed.
fn check_results(results: &[backends::UploadResult]) -> std::result::Result<(), ScanStationError> {
    let mut succeeded = 0u32;
    let mut failed = 0u32;

    for r in results {
        if r.success {
            succeeded += 1;
            tracing::info!(
                backend = %r.backend_name,
                duration_ms = r.duration.as_millis(),
                "upload ok"
            );
        } else {
            failed += 1;
            tracing::error!(
                backend = %r.backend_name,
                duration_ms = r.duration.as_millis(),
                error = %r.message,
                "upload failed"
            );
        }
    }

    tracing::info!(succeeded, failed, total = results.len(), "upload complete");

    if failed > 0 {
        Err(ScanStationError::Upload {
            backend: "multiple".into(),
            source: format!("{failed} of {} backends failed", results.len()).into(),
        })
    } else {
        Ok(())
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    setup_tracing(cli.verbose);

    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!("{e}");
            ExitCode::FAILURE
        }
    }
}
