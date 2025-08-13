FROM oven/bun as build

WORKDIR /build

ENV NODE_ENV=production
COPY . .
RUN bun install --frozen-lockfile && \
    bun test --ci && \
    bun compile

FROM debian:bookworm-slim

WORKDIR /app

# Dependencies based on: https://github.com/rocketraman/sane-scan-pdf/wiki/Dependencies-Installation
RUN apt update &&         \
    apt upgrade -y &&     \
    apt install -y        \
      bc                  \
      ghostscript         \
      git                 \
      imagemagick         \
      netpbm              \
      parallel            \
      poppler-utils       \
      sane                \
      sane-utils          \
      scanbd              \
      time                \
      tzdata              \
      units               \
      usbutils            \
      util-linux

ENV TZ=${TZ-"America/Edmonton"}
ENV vendor=${vendor-"fujitsu"}

RUN git clone https://github.com/rocketraman/sane-scan-pdf.git --depth 1

COPY conf/scanbd.conf /etc/scanbd/scanbd.conf
COPY scripts/discover.sh /app/discover.sh
COPY --from=build /build/dist/scan /etc/scanbd/scripts/scan

RUN chmod +x /etc/scanbd/scripts/scan.sh && \
    chmod +x /app/discover.sh

RUN mkdir /scans
VOLUME /scans

CMD /app/discover.sh && scanbd -d1 -f
