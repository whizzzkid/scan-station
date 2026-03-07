# Stage 1: Build Rust binaries
FROM rust:1.88-slim-bookworm AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

RUN cargo build --release -p scan-station --features all-backends \
 && cargo build --release -p discover-scanner

# Stage 2: Minimal runtime image
FROM debian:bookworm-slim

WORKDIR /app

# Runtime deps for sane-scan-pdf + scan-station
#   bc            — empty page detection math
#   ca-certificates — TLS root certs for HTTPS backend uploads
#   ghostscript   — ps2pdf (PostScript -> PDF)
#   imagemagick   — convert (image processing, crop, contrast)
#   netpbm        — pnmtops (PNM -> PostScript)
#   poppler-utils — pdfunite (merge scanned pages)
#   sane-utils    — scanadf (scanner control)
#   scanbd        — button press daemon
#   tzdata        — timezone support for TZ env var
#   units         — DPI unit conversion in sane-scan-pdf
#   usbutils      — lsusb for discover-scanner
#   util-linux    — flock for scan synchronization
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
      bc \
      ca-certificates \
      ghostscript \
      git \
      imagemagick \
      netpbm \
      poppler-utils \
      sane-utils \
      scanbd \
      tzdata \
      units \
      usbutils \
      util-linux && \
    # Pinned to specific commit for reproducible builds
    git clone https://github.com/rocketraman/sane-scan-pdf.git /app/sane-scan-pdf && \
    cd /app/sane-scan-pdf && git checkout 019e95cb64bb31c60e6bf48c97b2f9f5728405ab && cd /app && \
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
RUN mkdir -p /etc/scanbd/scripts /scans /etc/scan-station && \
    printf '#!/bin/sh\nexec scan-station scan\n' \
      > /etc/scanbd/scripts/scan-station && \
    chmod +x /etc/scanbd/scripts/scan-station

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
