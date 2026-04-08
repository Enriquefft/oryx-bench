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
RUN apt-get update && apt-get install -y --no-install-recommends \
    # Build essentials
    build-essential \
    git \
    curl \
    wget \
    ca-certificates \
    pkg-config \
    # QMK dependencies
    python3 \
    python3-pip \
    python3-venv \
    # ARM toolchain
    arm-none-eabi-gcc \
    arm-none-eabi-binutils \
    arm-none-eabi-newlib \
    libnewlib-arm-none-eabi \
    # For dfu and flashing (though oryx-bench doesn't use dfu-util directly)
    libusb-1.0-0-dev \
    # Cleanup
    && rm -rf /var/lib/apt/lists/*

# Install Zig (for Tier 2 overlay compilation)
# Zig 0.13.0 is compatible with the QMK build environment
RUN mkdir -p /opt/zig && \
    cd /opt/zig && \
    curl -L https://ziglang.org/download/0.13.0/zig-linux-x86_64-0.13.0.tar.xz | tar xJ && \
    ln -s /opt/zig/zig-0.13.0/zig /usr/local/bin/zig && \
    zig version

# Clone ZSA qmk_firmware fork at the pinned commit/branch
# pin.txt contains either a commit SHA or a branch name (e.g., "firmware24")
# If a branch name is used, git checkout will resolve it to the current HEAD of that branch
# For reproducible builds, pin.txt should contain a specific commit SHA (40-char hex)
COPY pin.txt /firmware_pin.txt

RUN REF=$(cat /firmware_pin.txt | tr -d '\n' | tr -d '\r') && \
    echo "Cloning ZSA qmk_firmware from ${REF}" && \
    mkdir -p /firmware && \
    cd /firmware && \
    git clone --depth=1 --branch "${REF}" https://github.com/zsa/qmk_firmware.git . || \
    (git clone https://github.com/zsa/qmk_firmware.git . && git checkout "${REF}") && \
    COMMIT_SHA=$(git rev-parse HEAD) && \
    echo "✓ Firmware checkout complete: ${COMMIT_SHA}" && \
    git log --oneline -1

# Install QMK Python dependencies
RUN cd /firmware && \
    python3 -m pip install --no-cache-dir -r requirements.txt

# Set QMK home and enable QMK CLI
ENV QMK_HOME=/firmware
ENV PATH="/firmware:${PATH}"

# Verify the build environment
RUN qmk --version && \
    arm-none-eabi-gcc --version && \
    zig version

# Set working directory for bind-mounted project
WORKDIR /work

# Default command: open a shell (can be overridden for builds)
CMD ["/bin/bash"]
