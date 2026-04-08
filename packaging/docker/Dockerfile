# Dockerfile for oryx-bench QMK build environment
#
# Bundles QMK CLI, arm-none-eabi-gcc, Zig, and ZSA's qmk_firmware fork
# at a pinned commit (see pin.txt) to provide a reproducible build sandbox.
#
# The firmware is mounted at /firmware and assumed to be at the commit
# hash specified in pin.txt; this is validated by the build pipeline.
#
# Build with:
#   docker build -t oryx-bench-qmk:v0.1.0 .
#
# Run with (mounts current directory as /work, outputs firmware.bin):
#   docker run -v "$(pwd):/work" oryx-bench-qmk:v0.1.0 qmk compile -kb zsa/voyager -km oryx-bench

FROM ubuntu:24.04

LABEL maintainer="Enrique Flores <enrique@zsa.io>"
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
    ls -la && \
    ZIG_DIR=$(ls -d zig-* 2>/dev/null | head -1) && \
    test -n "$ZIG_DIR" || (echo "ERROR: No zig directory found" && ls -la && exit 1) && \
    ln -sf /opt/zig/$ZIG_DIR/zig /usr/local/bin/zig && \
    /usr/local/bin/zig version && \
    echo "✓ Zig 0.13.0 installed"

# Clone ZSA qmk_firmware fork at the pinned commit/branch
# pin.txt contains either a commit SHA or a branch name (e.g., "firmware24")
# If a branch name is used, git checkout will resolve it to the current HEAD of that branch
# For reproducible builds, pin.txt should contain a specific commit SHA (40-char hex)
COPY pin.txt /firmware_pin.txt

RUN set -e && \
    REF=$(cat /firmware_pin.txt | tr -d '\n' | tr -d '\r') && \
    echo "Cloning ZSA qmk_firmware from '${REF}'" && \
    mkdir -p /firmware && \
    cd /firmware && \
    git clone --filter=blob:none https://github.com/zsa/qmk_firmware.git . && \
    git checkout "${REF}" && \
    COMMIT_SHA=$(git rev-parse HEAD) && \
    echo "✓ Firmware checkout complete at: ${COMMIT_SHA}" && \
    git log --oneline -1

# Install QMK Python dependencies
# Use --break-system-packages because we're in a container environment
# Don't upgrade system pip/setuptools; QMK's requirements will work with Debian versions
RUN set -e && \
    cd /firmware && \
    python3 -m pip install --no-cache-dir --break-system-packages -r requirements.txt && \
    echo "✓ QMK dependencies installed"

# Set QMK home and enable QMK CLI
ENV QMK_HOME=/firmware
ENV PATH="/firmware/bin:/usr/local/bin:${PATH}"

# Verify the build environment
RUN set -e && \
    echo "==== Build environment verification ====" && \
    echo "✓ Python 3:" && python3 --version && \
    echo "✓ ARM GCC:" && arm-none-eabi-gcc --version && \
    echo "✓ Zig:" && zig version && \
    echo "✓ Firmware repo:" && test -d /firmware && echo "Cloned" && \
    echo "✓ All tools installed and functional" && \
    echo "====================================="

# Set working directory for bind-mounted project
WORKDIR /work

# Default command: open a shell (can be overridden for builds)
CMD ["/bin/bash"]
