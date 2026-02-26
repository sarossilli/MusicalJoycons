FROM rust:1-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
    libudev-dev \
    libusb-1.0-0-dev \
    pkg-config \
    libdbus-1-dev \
    libbluetooth-dev \
    bluez \
    dbus \
    && rm -rf /var/lib/apt/lists/*

RUN rustup component add rustfmt clippy
