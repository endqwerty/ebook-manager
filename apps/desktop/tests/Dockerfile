ARG UBUNTU=24.04
FROM ubuntu:${UBUNTU}
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y \
    build-essential curl ca-certificates pkg-config libgtk-4-dev libadwaita-1-dev \
    bubblewrap xvfb openbox dbus-x11 dosfstools python3-gi gir1.2-atspi-2.0 \
    at-spi2-core xdotool xauth imagemagick tesseract-ocr-eng && \
    useradd -m tester
USER tester
ENV PATH=/home/tester/.cargo/bin:$PATH CARGO_TARGET_DIR=/out/target
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal && \
    curl -fsSL https://get.nexte.st/0.9.152/linux | tar -xz -C /home/tester/.cargo/bin
WORKDIR /src
