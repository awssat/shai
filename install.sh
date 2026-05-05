#!/bin/sh
set -e

REPO="awssat/shai"
BASE_URL="https://github.com/$REPO/releases/latest/download"

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
  linux)
    case "$ARCH" in
      x86_64)  ARTIFACT="shai-linux-x86_64.tar.gz" ;;
      aarch64) ARTIFACT="shai-linux-aarch64.tar.gz" ;;
      *) echo "Unsupported arch: $ARCH"; exit 1 ;;
    esac
    ;;
  darwin)
    case "$ARCH" in
      x86_64)  ARTIFACT="shai-macos-x86_64.tar.gz" ;;
      arm64)   ARTIFACT="shai-macos-aarch64.tar.gz" ;;
      *) echo "Unsupported arch: $ARCH"; exit 1 ;;
    esac
    ;;
  *) echo "Unsupported OS: $OS"; exit 1 ;;
esac

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

echo "Downloading $ARTIFACT..."
curl -fsSL "$BASE_URL/$ARTIFACT" | tar xz -C "$TMP"

INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

if [ -w "$INSTALL_DIR" ]; then
  mv "$TMP/shai" "$INSTALL_DIR/shai"
else
  sudo mv "$TMP/shai" "$INSTALL_DIR/shai"
fi

echo "shai installed to $INSTALL_DIR/shai"
shai --version
