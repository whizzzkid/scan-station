use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "scan-station")]
#[command(about = "Headless scanning server with multi-backend document upload")]
#[command(version)]
pub struct Cli {
    /// Path to configuration file
    #[arg(short, long, default_value = "/etc/scan-station/config.toml")]
    pub config: PathBuf,

    /// Increase log verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Scan a document and upload to all configured backends
    Scan {
        /// Override scanner vendor
        #[arg(long)]
        vendor: Option<String>,

        /// Override scan DPI
        #[arg(long)]
        dpi: Option<u32>,

        /// Override scan mode (Color, Gray, Lineart)
        #[arg(long)]
        mode: Option<String>,

        /// Skip upload after scanning (just produce the PDF)
        #[arg(long)]
        no_upload: bool,
    },

    /// Upload an existing file to all configured backends
    Upload {
        /// Path to the file to upload
        file: PathBuf,
    },

    /// Validate configuration and test backend connectivity
    Validate,
}
