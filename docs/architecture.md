# Scan-Station Architecture

## Overview

scan-station is a headless document scanning server designed to run on
resource-constrained single-board computers (Raspberry Pi, DietPi). It
detects a physical scanner button press, produces a PDF, and ships it to
one or more configured backends in parallel.

## System Flow

```
┌──────────────┐     button      ┌────────┐    trigger     ┌──────────────────┐
│  USB Scanner │ ──── press ───▶ │ scanbd │ ─────────────▶ │  scan-station    │
│  (ScanSnap)  │                 │ daemon │                │  scan subcommand │
└──────────────┘                 └────────┘                └────────┬─────────┘
                                                                    │
                                                        shell out to│sane-scan-pdf
                                                                    ▼
                                                            ┌──────────────┐
                                                            │  /tmp/*.pdf  │
                                                            └──────┬───────┘
                                                                   │
                                                      upload_all() │ (parallel)
                                          ┌────────────────────────┼────────────────────┐
                                          ▼                        ▼                    ▼
                                   ┌────────────┐          ┌─────────────┐      ┌─────────────┐
                                   │ Filesystem │          │   Papra     │      │     S3      │
                                   │  /scans/   │          │   API       │      │   bucket    │
                                   └────────────┘          └─────────────┘      └─────────────┘
                                                                  ...and any other configured backends
```

## Container Startup

```
┌─────────────────┐     writes config      ┌────────┐     monitors      ┌──────────────┐
│discover-scanner │ ──── to SANE ────────▶ │ scanbd │ ──── buttons ──▶ │  USB Scanner │
│  (runs once)    │     /etc/sane.d/       │ daemon │                  └──────────────┘
└─────────────────┘     /etc/scanbd/       │ (stays │
                                           │  alive)│
                                           └────────┘
```

1. `discover-scanner` runs once at container start
   - Calls `lsusb` to find the scanner's USB VID:PID
   - Writes device config to `/etc/sane.d/{vendor}.conf` and `/etc/scanbd/{vendor}.conf`
2. `scanbd` starts as a foreground daemon
   - Monitors the scanner for button presses
   - On button press: executes `scan-station scan`

## Binaries

The project produces two separate binaries from a Cargo workspace:

### `discover-scanner` (~1MB)

Standalone, fully synchronous. No async runtime, no HTTP, no TLS.

Dependencies: `clap` only.

```
discover-scanner [--vendor NAME] [--sane-conf-dir PATH] [--scanbd-conf-dir PATH]
```

### `scan-station` (~2-5MB depending on features)

Async (tokio). Feature-gated upload backends.

```
scan-station [--config PATH] [-v...] <COMMAND>
  scan       Scan document, then upload to all backends
  upload     Upload an existing file to all configured backends
  validate   Test config and backend connectivity
```

## Workspace Layout

```
scan-station/
  Cargo.toml                    # [workspace] root
  crates/
    discover-scanner/           # Standalone tiny binary
      Cargo.toml                #   deps: clap only
      src/main.rs
    scan-station/               # Main binary with feature-gated backends
      Cargo.toml                #   deps: core + optional per-backend
      src/
        main.rs                 # CLI entry point
        cli.rs                  # clap subcommand definitions
        lib.rs                  # Library root
        config.rs               # TOML config + ${ENV_VAR} substitution
        error.rs                # ScanStationError enum
        scan.rs                 # Shell out to sane-scan-pdf
        backends/
          mod.rs                # Backend trait + Uploader orchestrator + factory
          filesystem.rs         # std::fs::copy
          papra.rs              # HTTP POST multipart (feature: papra)
          paperless.rs          # HTTP POST multipart (feature: paperless)
          s3.rs                 # HTTP PUT + SigV4 (feature: s3)
          gdrive.rs             # HTTP POST + JWT auth (feature: gdrive)
```

## Backend Trait

Every upload backend implements the same trait:

```rust
#[async_trait]
pub trait Backend: Send + Sync {
    fn name(&self) -> &str;
    async fn upload(&self, path: &Path) -> Result<()>;
    async fn validate(&self) -> Result<()> { Ok(()) }
}
```

The `Uploader` orchestrator:
- Instantiates backends from config via a factory function
- Dispatches uploads to all backends in parallel via `tokio::task::JoinSet`
- Isolates errors: one backend failure does not block others
- Supports per-backend retry with configurable attempts and delay

## Feature Flags

Backends are compile-time feature flags on the `scan-station` crate:

| Feature | Backends | Extra deps |
|---------|----------|------------|
| `filesystem` (default) | Filesystem | none |
| `papra` | Papra | `reqwest` |
| `paperless` | Paperless-NGX | `reqwest` |
| `s3` | S3 / MinIO | `reqwest`, `hmac`, `sha2`, `hex` |
| `gdrive` | Google Drive | `reqwest`, `jsonwebtoken` |
| `http-backends` | papra + paperless | |
| `cloud-backends` | s3 + gdrive | |
| `all-backends` | everything | |

No SDKs. Every backend is an HTTP request. S3 uses hand-rolled SigV4
signing (~100 lines). Google Drive uses JWT service account auth.

## Configuration

TOML file with three-layer resolution:

1. **TOML file** — default `/etc/scan-station/config.toml`
2. **`${ENV_VAR}` substitution** — secrets injected from Docker environment
3. **CLI flag overrides** — `--vendor`, `--dpi`, `--mode`

```toml
[scan]
vendor = "fujitsu"
dpi = 300
mode = "Color"
file_prefix = "scan"

[[backends]]
type = "filesystem"
path = "/scans"

[[backends]]
type = "papra"
api_url = "https://papra.example.com"
api_key = "${PAPRA_API_KEY}"          # resolved from environment
organization_id = "org-123"
```

Backends use `[[backends]]` (TOML array of tables) so you can have
multiple instances of the same type (e.g., two S3 buckets).

## Docker Deployment

Zero-config (filesystem only, just env vars):

```yaml
services:
  scanner:
    image: ghcr.io/whizzzkid/scan-station:latest
    privileged: true
    environment:
      - TZ=America/Edmonton
      - SCAN_DPI=300
    volumes:
      - /ingest:/scans
      - /var/run/dbus:/var/run/dbus
```

Multi-backend (mount a TOML config):

```yaml
services:
  scanner:
    image: ghcr.io/whizzzkid/scan-station:latest
    privileged: true
    environment:
      - TZ=America/Edmonton
      - PAPRA_API_KEY=${PAPRA_API_KEY}
    volumes:
      - /ingest:/scans
      - ./config.toml:/etc/scan-station/config.toml:ro
      - /var/run/dbus:/var/run/dbus
```

See the [example/](../example/) folder for a complete ready-to-copy setup.

The Docker image works out of the box with no config file. Set `SCAN_*`
env vars for simple use, or mount a TOML config for multi-backend setups.

The image is a multi-stage build:
- **Builder stage**: `rust:slim-bookworm` compiles both binaries
- **Runtime stage**: `debian:bookworm-slim` with only scanning system deps

System dependencies (runtime only): bc, ca-certificates, ghostscript,
imagemagick, netpbm, poppler-utils, sane-utils, scanbd, tzdata, units,
usbutils, util-linux. See the Dockerfile for per-package justification.

## Adding a New Backend

See [docs/adding-backends.md](adding-backends.md) for the full guide.

Quick summary:
1. Create `src/backends/new_backend.rs` with `#[cfg(feature = "new_backend")]`
2. Add feature + optional dep to `Cargo.toml`
3. Add variant to `BackendConfig` enum in `config.rs`
4. Add match arm in factory function in `backends/mod.rs`
5. Add tests, example config, documentation
