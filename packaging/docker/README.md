# Docker Build Environment for oryx-bench

This directory contains the Docker image for building ZSA keyboard firmware with oryx-bench.

## What's included

The image bundles:
- **QMK CLI** — Python-based QMK build system
- **arm-none-eabi-gcc** — ARM toolchain for STM32F303 (Voyager) and other ZSA keyboards
- **Zig 0.13.0** — Compiler for Tier 2 overlay code (procedural behavior)
- **ZSA qmk_firmware fork** — Pinned at a specific commit/branch for reproducible builds

## Building the image

```bash
cd packaging/docker
docker build -t oryx-bench-qmk:v0.1.0 .
```

The image tag must match the oryx-bench version (reads from `Cargo.toml` automatically in CI).

## Using the image

The oryx-bench CLI automatically invokes this image when you run `oryx-bench build`:

```bash
# In your keyboard project directory:
oryx-bench build

# This internally runs something like:
# docker run -v "$(pwd):/work" ghcr.io/enriquefft/oryx-bench-qmk:v0.1.0 \
#   qmk compile -kb zsa/voyager -km oryx-bench
```

For manual builds or debugging:

```bash
docker run -it -v "$(pwd):/work" oryx-bench-qmk:v0.1.0
# Inside the container:
cd /work
qmk compile -kb zsa/voyager -km oryx-bench
```

## Pinning the firmware version

The `pin.txt` file controls which version of ZSA's qmk_firmware is used:

- **Format**: A git reference — either a branch name (e.g., `firmware24`) or a commit SHA
- **Branches**: `firmware24` (stable QMK 24.x), `main` (development), etc.
- **Commit SHAs**: 40-character hex strings (preferred for reproducible builds)

### For development: use a branch

```
firmware24
```

### For releases: use a commit SHA

```
abc123def456abc123def456abc123def456abc1
```

**Best practice**: Pin to a specific commit SHA after testing a new version. This ensures that every `oryx-bench build` across all machines produces byte-identical firmware.

## Updating the firmware version

When ZSA releases a new QMK version or fixes a critical bug:

1. Test against the target keyboard (e.g., Voyager) locally:
   ```bash
   # Update pin.txt to the new commit/branch
   echo "firmware24" > pin.txt
   
   # Rebuild the image
   docker build -t oryx-bench-qmk:v0.1.0 .
   
   # Test a compile
   oryx-bench build
   ```

2. Once verified, update `pin.txt` to use a specific commit SHA:
   ```bash
   # Inside the container, get the commit SHA:
   docker run oryx-bench-qmk:v0.1.0 git -C /firmware rev-parse HEAD
   
   # Then pin it:
   echo "abc123..." > pin.txt
   
   # Rebuild and commit
   docker build -t oryx-bench-qmk:v0.1.0 .
   git add pin.txt
   git commit -m "Pin qmk_firmware to <hash>"
   ```

3. Release the new oryx-bench version:
   - Update `Cargo.toml` version
   - Run the `docker.yml` GitHub workflow (publishes to GHCR)
   - Tag the release

## Technical details

### Zig integration

The Zig compiler (0.13.0) is installed so that Tier 2 overlay code can be compiled. The `oryx-bench` build generator writes Zig sources to `overlay/*.zig`, which are compiled to ARM object files and linked into the final firmware.

### Reproducibility

Build inputs are deterministic:
- QMK version is pinned in `pin.txt`
- Zig version is pinned in the Dockerfile (0.13.0)
- arm-none-eabi-gcc version is determined by Ubuntu 24.04's package repos
- Python dependencies are frozen by `qmk_firmware`'s `requirements.txt`

Two builds with the same `pin.txt` and `Dockerfile` will produce byte-identical firmware (`firmware.bin` SHA256 match).

### Network isolation

The Dockerfile requires network access at build time to clone the ZSA qmk_firmware repository. After the image is built, all build operations happen offline (no network calls during `oryx-bench build`).

## Debugging

If a build fails inside the container, you can inspect the build environment:

```bash
docker run -it -v "$(pwd):/work" oryx-bench-qmk:v0.1.0 /bin/bash
```

Useful commands inside the container:
```bash
qmk --version                          # Verify QMK is installed
arm-none-eabi-gcc --version            # Verify toolchain
zig version                            # Verify Zig
ls /firmware                           # View the pinned qmk_firmware
cd /work && qmk compile -kb zsa/voyager -km oryx-bench  # Manual build
```

## CI/CD

The GitHub workflow at `.github/workflows/docker.yml` automatically:
1. Builds the Docker image on every push
2. Publishes to GHCR (`ghcr.io/enriquefft/oryx-bench-qmk:vX.Y.Z`) on releases
3. Tags with the oryx-bench version from `Cargo.toml`

The CLI reads the version from `env!("CARGO_PKG_VERSION")` so the image tag always matches the binary.
