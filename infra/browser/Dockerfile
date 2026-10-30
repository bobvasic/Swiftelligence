# Optional isolated Servo runtime. Swiftel itself is a Rust binary.
# Downloaded engine: Servo v0.7.0, MPL-2.0. No Firecrawl code is bundled.
FROM ubuntu:24.04
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    ca-certificates curl libgstreamer-plugins-bad1.0-0 libgstreamer-plugins-base1.0-0 \
    libgstreamer1.0-0 libgudev-1.0-0 libgl1 libegl1 libx11-6 libwayland-client0 \
    libwayland-egl1 libwayland-cursor0 libfontconfig1 libunwind8 fonts-dejavu-core \
    && rm -rf /var/lib/apt/lists/*
RUN curl -fsSL --retry 3 -o /tmp/servo.tar.gz \
    https://github.com/servo/servo/releases/download/v0.7.0/servo-x86_64-linux-gnu.tar.gz \
    && printf '%s  %s\n' '728eba1be1cc1851e05dfaa90e18f98ab8644dd2355bedb0b8a5e89795ebe333' '/tmp/servo.tar.gz' | sha256sum -c - \
    && mkdir -p /opt && tar -xzf /tmp/servo.tar.gz -C /opt && rm /tmp/servo.tar.gz
RUN useradd --create-home --uid 10001 swiftel
USER swiftel
ENV HOME=/home/swiftel
WORKDIR /opt/servo
EXPOSE 7000
ENTRYPOINT ["/opt/servo/servoshell", "--headless", "--temporary-storage", "--config-dir=/tmp/servo-config", "--webdriver", "7000", "about:blank"]
