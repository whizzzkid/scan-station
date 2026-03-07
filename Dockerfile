# Stage 1: Build Rust binaries
FROM rust:1.85-slim-bookworm AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

RUN cargo build --release -p scan-station --features all-backends \
 && cargo build --release -p discover-scanner

# Stage 2: Minimal runtime image
FROM debian:bookworm-slim

WORKDIR /app

# System dependencies for SANE scanning (no-install-recommends for minimal size)
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
      bc \
      ca-certificates \
      ghostscript \
      git \
      imagemagick \
      netpbm \
      parallel \
      poppler-utils \
      sane \
      sane-utils \
      scanbd \
      tzdata \
      usbutils \
      util-linux && \
    # Clone scan engine (only reason git is installed)
    git clone --depth 1 https://github.com/rocketraman/sane-scan-pdf.git /app/sane-scan-pdf && \
    # Remove git and clean up
    apt-get purge -y git && \
    apt-get autoremove -y && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/* /usr/share/doc /usr/share/man /usr/share/locale /tmp/*

# Copy compiled binaries from builder
COPY --from=builder /build/target/release/scan-station /usr/local/bin/scan-station
COPY --from=builder /build/target/release/discover-scanner /usr/local/bin/discover-scanner

# Copy scanbd config
COPY conf/scanbd.conf /etc/scanbd/scanbd.conf

# Create scanbd trigger script and directories
RUN printf '#!/bin/sh\nexec scan-station scan\n' \
      > /etc/scanbd/scripts/scan-station && \
    chmod +x /etc/scanbd/scripts/scan-station && \
    mkdir -p /scans /etc/scan-station

# Works out of the box with just env vars — no config file required.
# For multi-backend setups, mount a TOML config at /etc/scan-station/config.toml
#
# Scan settings via env vars:
#   SCAN_VENDOR       (default: fujitsu)
#   SCAN_DPI          (default: 300)
#   SCAN_MODE         (default: Color)
#   SCAN_FILE_PREFIX  (default: scan)
#   SCAN_OUTPUT_DIR   (default: /scans)
#
# For backend-specific secrets (used in TOML config):
#   PAPRA_API_KEY, PAPERLESS_TOKEN, AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY

ENV TZ=Etc/UTC
VOLUME ["/scans", "/etc/scan-station"]

# Discover scanner, then start scanbd daemon
CMD ["sh", "-c", "discover-scanner && scanbd -d1 -f"]
