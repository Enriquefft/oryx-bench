#!/usr/bin/env sh
# install.sh — one-line installer for oryx-bench (Linux only)
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/enriquefft/oryx-bench/main/scripts/install.sh | sh
#
# Environment variables:
#   INSTALL_DIR   — installation directory (default: /usr/local/bin)
#   VERSION       — specific release tag to install (default: latest)

set -eu

REPO="enriquefft/oryx-bench"
BIN="oryx-bench"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

# ── Detect architecture ───────────────────────────────────────────────────────
case "$(uname -m)" in
  x86_64)          ARCH="x86_64" ;;
  aarch64 | arm64) ARCH="aarch64" ;;
  *)
    echo "error: unsupported architecture: $(uname -m)" >&2
    echo "       Supported: x86_64, aarch64" >&2
    exit 1
    ;;
esac

# ── Detect OS ─────────────────────────────────────────────────────────────────
case "$(uname -s)" in
  Linux) ;;
  *)
    echo "error: unsupported OS: $(uname -s)" >&2
    echo "       Only Linux is supported." >&2
    exit 1
    ;;
esac

TARGET="${ARCH}-unknown-linux-musl"

# ── Resolve version ───────────────────────────────────────────────────────────
if [ -z "${VERSION:-}" ]; then
  VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep '"tag_name"' \
    | head -1 \
    | cut -d'"' -f4)
  if [ -z "$VERSION" ]; then
    echo "error: could not determine latest release (GitHub API rate limit?)" >&2
    echo "       Set VERSION=v0.x.y to install a specific release." >&2
    exit 1
  fi
fi

# Normalise: strip leading 'v' for tarball name, keep with 'v' for URL path
TAG="${VERSION#v}"
TAG_WITH_V="v${TAG}"

ARCHIVE="${BIN}-${TAG_WITH_V}-${TARGET}.tar.gz"
URL="https://github.com/${REPO}/releases/download/${TAG_WITH_V}/${ARCHIVE}"

echo "Installing ${BIN} ${TAG_WITH_V} (${TARGET})..."

# ── Download and extract ──────────────────────────────────────────────────────
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

if ! curl -fsSL "$URL" -o "${TMP}/${ARCHIVE}"; then
  echo "error: download failed: ${URL}" >&2
  exit 1
fi

tar xzf "${TMP}/${ARCHIVE}" -C "$TMP"

# ── Install ───────────────────────────────────────────────────────────────────
if [ ! -d "$INSTALL_DIR" ]; then
  echo "error: install directory does not exist: ${INSTALL_DIR}" >&2
  echo "       Create it or set INSTALL_DIR to an existing directory." >&2
  exit 1
fi

DEST="${INSTALL_DIR}/${BIN}"

if [ -w "$INSTALL_DIR" ]; then
  mv "${TMP}/${BIN}" "$DEST"
else
  echo "note: ${INSTALL_DIR} is not writable, using sudo"
  sudo mv "${TMP}/${BIN}" "$DEST"
fi

chmod 755 "$DEST"

echo "Installed: ${DEST}"
"$DEST" --version
