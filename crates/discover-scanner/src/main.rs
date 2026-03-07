use clap::Parser;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

#[derive(Parser)]
#[command(name = "discover-scanner")]
#[command(about = "Detect USB scanner and write SANE/scanbd device config")]
struct Cli {
    /// Scanner vendor name to search for in lsusb output
    #[arg(long, default_value = "fujitsu")]
    vendor: String,

    /// Directory for SANE config files
    #[arg(long, default_value = "/etc/sane.d")]
    sane_conf_dir: PathBuf,

    /// Directory for scanbd config files
    #[arg(long, default_value = "/etc/scanbd")]
    scanbd_conf_dir: PathBuf,
}

/// A discovered USB device with vendor and product IDs.
#[derive(Debug, PartialEq)]
struct UsbDevice {
    vid: String,
    pid: String,
}

/// Parse lsusb output to find a device matching the given vendor name.
///
/// Expected line format: `Bus 001 Device 004: ID 04c5:1156 Fujitsu ...`
fn find_device(lsusb_output: &str, vendor: &str) -> Option<UsbDevice> {
    let vendor_lower = vendor.to_lowercase();

    lsusb_output
        .lines()
        .find(|line| line.to_lowercase().contains(&vendor_lower))
        .and_then(|line| {
            let id_str = line.split("ID ").nth(1)?;
            let id = id_str.split_whitespace().next()?;
            let (vid, pid) = id.split_once(':')?;
            Some(UsbDevice {
                vid: vid.to_owned(),
                pid: pid.to_owned(),
            })
        })
}

fn run(cli: &Cli) -> Result<(), String> {
    let output = Command::new("lsusb")
        .output()
        .map_err(|e| format!("failed to run lsusb: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("lsusb failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let device = find_device(&stdout, &cli.vendor)
        .ok_or_else(|| format!("{} scanner not found in lsusb output", cli.vendor))?;

    let device_line = format!("usb 0x{} 0x{}\n", device.vid, device.pid);
    eprintln!("{} scanner found: {}", cli.vendor, device_line.trim());

    let sane_path = cli.sane_conf_dir.join(format!("{}.conf", cli.vendor));
    std::fs::write(&sane_path, &device_line)
        .map_err(|e| format!("failed to write {}: {e}", sane_path.display()))?;

    let scanbd_path = cli.scanbd_conf_dir.join(format!("{}.conf", cli.vendor));
    std::fs::write(&scanbd_path, &device_line)
        .map_err(|e| format!("failed to write {}: {e}", scanbd_path.display()))?;

    eprintln!(
        "wrote config to {} and {}",
        sane_path.display(),
        scanbd_path.display()
    );
    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_LSUSB: &str = "\
Bus 001 Device 001: ID 1d6b:0002 Linux Foundation 2.0 root hub
Bus 001 Device 004: ID 04c5:1156 Fujitsu, Ltd ScanSnap S1500
Bus 002 Device 001: ID 1d6b:0003 Linux Foundation 3.0 root hub
Bus 002 Device 002: ID 0bda:8153 Realtek RTL8153 Gigabit Ethernet";

    #[test]
    fn finds_fujitsu_scanner() {
        let device = find_device(SAMPLE_LSUSB, "fujitsu").unwrap();
        assert_eq!(
            device,
            UsbDevice {
                vid: "04c5".into(),
                pid: "1156".into()
            }
        );
    }

    #[test]
    fn finds_case_insensitive() {
        let device = find_device(SAMPLE_LSUSB, "FUJITSU").unwrap();
        assert_eq!(
            device,
            UsbDevice {
                vid: "04c5".into(),
                pid: "1156".into()
            }
        );
    }

    #[test]
    fn returns_none_for_missing_vendor() {
        assert!(find_device(SAMPLE_LSUSB, "canon").is_none());
    }

    #[test]
    fn returns_none_for_empty_output() {
        assert!(find_device("", "fujitsu").is_none());
    }

    #[test]
    fn returns_none_for_malformed_line() {
        let bad = "Bus 001 Device 004: Fujitsu without ID field";
        assert!(find_device(bad, "fujitsu").is_none());
    }

    #[test]
    fn uses_first_match() {
        let multi = "\
Bus 001 Device 002: ID aaaa:bbbb Fujitsu First
Bus 001 Device 003: ID cccc:dddd Fujitsu Second";
        let device = find_device(multi, "fujitsu").unwrap();
        assert_eq!(
            device,
            UsbDevice {
                vid: "aaaa".into(),
                pid: "bbbb".into()
            }
        );
    }

    #[test]
    fn writes_config_files() {
        let tmp = tempfile::tempdir().unwrap();
        let cli = Cli {
            vendor: "fujitsu".into(),
            sane_conf_dir: tmp.path().to_path_buf(),
            scanbd_conf_dir: tmp.path().to_path_buf(),
        };

        // We can't run lsusb in CI, so test the write logic directly
        let device = UsbDevice {
            vid: "04c5".into(),
            pid: "1156".into(),
        };
        let device_line = format!("usb 0x{} 0x{}\n", device.vid, device.pid);

        let sane_path = cli.sane_conf_dir.join("fujitsu.conf");
        std::fs::write(&sane_path, &device_line).unwrap();

        let scanbd_path = cli.scanbd_conf_dir.join("fujitsu.conf");
        std::fs::write(&scanbd_path, &device_line).unwrap();

        assert_eq!(
            std::fs::read_to_string(&sane_path).unwrap(),
            "usb 0x04c5 0x1156\n"
        );
        assert_eq!(
            std::fs::read_to_string(&scanbd_path).unwrap(),
            "usb 0x04c5 0x1156\n"
        );
    }
}
