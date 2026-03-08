# CLAUDE.md — scan-station

## Project Overview

Headless document scanning server for resource-constrained SBCs.
Cargo workspace with two crates:

- `crates/discover-scanner` — tiny sync binary (~1MB), USB scanner detection
- `crates/scan-station` — async binary with feature-gated upload backends

A physical button press on the scanner triggers scanbd, which calls
`scan-station scan` to produce a PDF and upload it to all configured backends.

## Build Commands

```sh
cargo build                                                   # debug, filesystem only
cargo build --release -p discover-scanner                     # release ~1MB
cargo build --release -p scan-station                         # release ~2MB (filesystem)
cargo build --release -p scan-station --features all-backends # release ~4-5MB
cargo test --workspace                                        # default features
cargo test -p scan-station --features all-backends            # test all backends
cargo clippy --workspace -- -D clippy::pedantic               # lint
cargo fmt --all                                               # format
```

## Feature Flags (scan-station crate)

| Flag | Backends | Extra deps |
|------|----------|------------|
| `filesystem` (default) | local file copy | none |
| `papra` | Papra API | reqwest |
| `paperless` | Paperless-NGX API | reqwest |
| `s3` | S3 / MinIO | reqwest, hmac, sha2, hex |
| `gdrive` | Google Drive | reqwest, jsonwebtoken, serde_json |
| `http-backends` | papra + paperless | |
| `cloud-backends` | s3 + gdrive | |
| `all-backends` | everything | |

## Key File Map

```
Cargo.toml                              # workspace root (version = 1.0.0)
crates/discover-scanner/src/main.rs     # standalone USB discovery binary
crates/scan-station/src/
  main.rs                               # CLI entry point
  cli.rs                                # clap subcommand definitions
  lib.rs                                # library root
  error.rs                              # ScanStationError enum
  config.rs                             # TOML + ${ENV_VAR} + SCAN_* env overrides
  scan.rs                               # shell out to sane-scan-pdf
  backends/
    mod.rs                              # Backend trait + Uploader orchestrator + factory
    filesystem.rs                       # std::fs::copy
    papra.rs                            # HTTP POST multipart (feature: papra)
    paperless.rs                        # HTTP POST multipart (feature: paperless)
    s3.rs                               # HTTP PUT + SigV4 (feature: s3)
    gdrive.rs                           # HTTP POST + JWT auth (feature: gdrive)
config.example.toml                     # annotated example config (root)
example/                                # copy-to-server Docker setup
  docker-compose.yml                    # all mounts, env vars documented
  config.toml                           # all backends commented, ready to enable
  .env                                  # secrets template
conf/scanbd.conf                        # scanbd daemon config
Dockerfile                              # multi-stage build, no baked config
.github/workflows/
  ci.yml                                # tests + lint on every PR
  test-image.yml                        # branch Docker image for device testing
  cleanup-test-image.yml                # delete test image on PR close
  release.yml                           # auto-semver, Docker, binary artifacts
```

## Adding a New Backend

See `docs/adding-backends.md` for the full guide with copy-paste templates.

Quick checklist:
1. Create `src/backends/my_backend.rs` with `#[cfg(feature = "my_backend")]`
2. Add feature + optional deps to `crates/scan-station/Cargo.toml`
3. Add config variant to `BackendConfig` enum in `config.rs`
4. Add match arm in factory function in `backends/mod.rs`
5. Add match arm in `extract_retry_configs` in `backends/mod.rs`
6. Add tests in the same file (`#[cfg(test)]` module)
7. Add example config section to `config.example.toml`
8. Document in `README.md` under "Backends"

## Key Patterns

- **No SDKs.** Every backend is a raw HTTP request. S3 uses hand-rolled SigV4.
- **Zero-config Docker:** Works with just env vars (SCAN_DPI, SCAN_VENDOR, etc.). TOML optional for multi-backend.
- **Config 3-tier:** TOML file -> ${ENV_VAR} substitution -> SCAN_* env overrides -> CLI flags.
- **Secrets:** `${ENV_VAR}` in TOML, resolved at config load time. Never hardcoded.
- **Errors:** `thiserror` for `ScanStationError`. Never `unwrap()` in library code.
- **Async:** All backend uploads dispatched in parallel via `tokio::task::JoinSet`.
- **Tests:** `wiremock` for HTTP backends, `tempfile` for filesystem.
- **Size:** `default-features = false` on all deps. Every KB matters on SBCs.
- **CI:** All GitHub Actions use only verified publishers (GitHub, Docker, Google). No third-party actions.

## Conventions

- Commits: conventional commits with emoji (`feat: ✨`, `fix: 🐛`, etc.)
- PRs: always draft mode
- Config: TOML with `[[backends]]` array-of-tables
- Feature gates: `#[cfg(feature = "...")]` on backend modules AND config enum variants
- No unnecessary dependencies — raw HTTP over SDKs
