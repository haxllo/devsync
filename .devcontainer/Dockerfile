FROM mcr.microsoft.com/devcontainers/base:ubuntu-24.04

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        curl ca-certificates git build-essential python3 python3-pip \
    && rm -rf /var/lib/apt/lists/*

# Install Rust toolchain for Rust projects
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y \
    && ln -s /root/.cargo/bin/rustc /usr/local/bin/rustc \
    && ln -s /root/.cargo/bin/cargo /usr/local/bin/cargo

WORKDIR /workspace
