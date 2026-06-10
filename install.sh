#!/bin/sh
# Installs a prebuilt statusline binary from GitHub releases.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/ryanclark/statusline/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/ryanclark/statusline/main/install.sh | sh -s -- v0.1.4
#   INSTALL_DIR=/usr/local/bin ./install.sh

set -eu

REPO="ryanclark/statusline"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${1:-latest}"

err() {
  echo "error: $1" >&2
  exit 1
}

command -v curl >/dev/null 2>&1 || err "curl is required"
command -v tar >/dev/null 2>&1 || err "tar is required"

os="$(uname -s)"
arch="$(uname -m)"
case "$os $arch" in
  "Darwin arm64") target="aarch64-apple-darwin" ;;
  "Linux x86_64") target="x86_64-unknown-linux-gnu" ;;
  "Linux aarch64" | "Linux arm64") target="aarch64-unknown-linux-gnu" ;;
  *) err "unsupported platform: $os $arch (prebuilt binaries cover macOS arm64 and Linux x86_64/aarch64; see README.md for building from source)" ;;
esac

asset="statusline-$target.tar.gz"
if [ "$VERSION" = "latest" ]; then
  redirect="$(curl -fsSI --proto '=https' --tlsv1.2 -o /dev/null -w '%{redirect_url}' \
    "https://github.com/$REPO/releases/latest/download/$asset")" \
    || err "could not reach github.com to resolve the latest release"

  VERSION="$(printf '%s\n' "$redirect" | sed -n 's|.*/releases/download/\([^/]*\)/.*|\1|p')"
  [ -n "$VERSION" ] || err "could not determine the latest release tag"
fi

url="https://github.com/$REPO/releases/download/$VERSION/$asset"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Downloading $asset ($VERSION)..."
curl -fsSL --proto '=https' --tlsv1.2 -o "$tmp/$asset" "$url" \
  || err "download failed: $url (does release '$VERSION' include this platform?)"

tar -xzf "$tmp/$asset" -C "$tmp"
mkdir -p "$INSTALL_DIR"
install -m 755 "$tmp/statusline" "$INSTALL_DIR/statusline"

echo "Installed statusline $VERSION to $INSTALL_DIR/statusline"

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *) echo "note: $INSTALL_DIR is not on your PATH; add it with: export PATH=\"$INSTALL_DIR:\$PATH\"" ;;
esac

echo "Next: run 'statusline install' to wire it into Claude Code."
