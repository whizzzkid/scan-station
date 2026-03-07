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
            tracing::info!(path = %pdf_path.display(), "scan produced");

            if no_upload {
                tracing::info!("--no-upload: skipping backend uploads");
                return Ok(());
            }

            let uploader = Uploader::from_config(&app_config.backends)?;
            let retries = backends::extract_retry_configs(&app_config.backends);
            let results = uploader.upload_all(&pdf_path, &retries).await;
            check_results(&results)?;
        }

        Command::Upload { file } => {
            if !file.exists() {
                return Err(ScanStationError::FileNotFound(file));
            }

            let uploader = Uploader::from_config(&app_config.backends)?;
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
    let mut any_failed = false;
    for r in results {
        if r.success {
            tracing::info!(
                backend = %r.backend_name,
                duration_ms = r.duration.as_millis(),
                "upload ok"
            );
        } else {
            tracing::error!(
                backend = %r.backend_name,
                duration_ms = r.duration.as_millis(),
                error = %r.message,
                "upload failed"
            );
            any_failed = true;
        }
    }
    if any_failed {
        Err(ScanStationError::Upload {
            backend: "multiple".into(),
            source: "one or more backends failed".into(),
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
