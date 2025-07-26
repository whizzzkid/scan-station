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
RUN git clone https://github.com/rocketraman/sane-scan-pdf.git --depth 1

COPY scanbd.conf /etc/scanbd/scanbd.conf
COPY scan.sh /etc/scanbd/scripts/scan.sh
COPY discover.sh /app/discover.sh

RUN chmod +x /etc/scanbd/scripts/scan.sh && \
    chmod +x /app/discover.sh

RUN mkdir /scans
VOLUME /scans

CMD /app/discover.sh && scanbd -d1 -f
