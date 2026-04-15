# Dockerfile for oryx-bench QMK build environment
#
# Complete embedded development environment for building ZSA keyboard firmware:
# - QMK CLI (Python-based)
# - ARM Cortex-M cross-compiler (gcc-arm-none-eabi 13.2)
# - Zig 0.13.0 (for Tier 2 overlay code compilation)
# - ZSA qmk_firmware fork (pinned for reproducibility)
#
# Build with:
#   docker build -t oryx-bench-qmk:v0.1.0 .
#
# Run with:
#   docker run -v "$(pwd):/work" oryx-bench-qmk:v0.1.0 \
#     qmk compile -kb zsa/voyager -km oryx-bench
#
# Image size: ~4GB (includes full ARM toolchain + dependencies)
# Build time: ~5 minutes (first build), ~1 minute (cached)
# Final firmware size: ~80KB

FROM ubuntu:24.04

LABEL maintainer="Enrique Flores <enriquefft@404tf.com>"
LABEL description="Deterministic build environment for oryx-bench + ZSA keyboard layouts"

# Prevent interactive prompts during apt-get
ENV DEBIAN_FRONTEND=noninteractive

# Install system dependencies
# Split into multiple RUN commands for better caching and error visibility
RUN apt-get update -qq && \
    apt-get install -y --no-install-recommends \
    build-essential \
    git \
    curl \
    wget \
    ca-certificates \
    pkg-config \
    python3 \
    python3-pip \
    python3-venv \
    libusb-1.0-0-dev \
    dfu-util \
    && rm -rf /var/lib/apt/lists/*

# Install ARM embedded toolchain
# Individual packages for ARM Cortex-M cross-compilation
RUN apt-get update -qq && \
    apt-get install -y --no-install-recommends \
    binutils-arm-none-eabi \
    gcc-arm-none-eabi \
    libnewlib-arm-none-eabi \
    libstdc++-arm-none-eabi-newlib \
    && rm -rf /var/lib/apt/lists/*

# Install Zig (for Tier 2 overlay compilation)
# Zig 0.13.0 is compatible with the QMK build environment
RUN mkdir -p /opt/zig && \
    cd /opt/zig && \
    curl -fsSL --max-time 300 https://ziglang.org/download/0.13.0/zig-linux-x86_64-0.13.0.tar.xz | tar xJ && \
    ZIG_DIR=$(ls -d zig-* 2>/dev/null | head -1) && \
    test -n "$ZIG_DIR" || (echo "ERROR: No zig directory found after extraction" && exit 1) && \
    ln -sf /opt/zig/$ZIG_DIR/zig /usr/local/bin/zig && \
    /usr/local/bin/zig version

# Clone ZSA qmk_firmware fork at the pinned commit/branch
# pin.txt contains either a commit SHA or a branch name (e.g., "firmware24")
# Uses shallow clone (--depth=100) to reduce image size while maintaining safety
# For reproducible builds, pin.txt should contain a specific commit SHA (40-char hex)
COPY pin.txt /firmware_pin.txt

RUN set -e && \
    REF=$(cat /firmware_pin.txt | tr -d '\n' | tr -d '\r') && \
    echo "Cloning qmk_firmware at ${REF}" && \
    mkdir -p /firmware && \
    cd /firmware && \
    git clone --filter=blob:none https://github.com/zsa/qmk_firmware.git . && \
    git checkout "${REF}" && \
    git submodule update --init --recursive --depth=1 && \
    echo "Pinned to: $(git rev-parse HEAD)"

# Mark all directories as safe for any UID. The container runs with
# --user to match the host UID, but /firmware is owned by root.
# Without this, git warnings in QMK's Makefile $(shell) corrupt the
# Makefile parse — the colon in "fatal:" causes "multiple target
# patterns" errors. Wildcard is safe — this is an isolated build container.
RUN git config --system --add safe.directory '*'

# QMK's Makefile uses bash-isms (|| in recipes). Ubuntu 24.04
# defaults /bin/sh to dash which chokes on these.
RUN ln -sf /bin/bash /bin/sh

# Install QMK CLI and firmware Python dependencies.
# `qmk` (the CLI) is a separate package from the firmware repo's
# requirements.txt (which only lists library deps like milc, hjson, etc.).
# Use --break-system-packages because we're in a container environment.
RUN set -e && \
    cd /firmware && \
    python3 -m pip install --no-cache-dir --break-system-packages \
        qmk -r requirements.txt && \
    echo "✓ QMK CLI + dependencies installed"

# Set QMK home so the CLI finds the firmware tree.
ENV QMK_HOME=/firmware
ENV PATH="/usr/local/bin:${PATH}"

# Verify all tools are present and callable.
RUN set -e && \
    echo "Build environment ready:" && \
    echo "  QMK CLI: $(qmk --version)" && \
    echo "  Python 3: $(python3 --version)" && \
    echo "  ARM GCC: $(arm-none-eabi-gcc --version | head -1)" && \
    echo "  Zig: $(zig version)" && \
    test -f /usr/bin/make && echo "  Make: present" && \
    true

# Set working directory for bind-mounted project
WORKDIR /work

# Default command: open a shell (can be overridden for builds)
CMD ["/bin/bash"]
