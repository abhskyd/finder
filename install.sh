#!/bin/sh
# finder installer — downloads the prebuilt binary for this platform.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/abhskyd/finder/main/install.sh | sh
#
# Optional environment overrides:
#   FINDER_VERSION=v0.1.1   (release tag)
#   FINDER_PREFIX=~/bin     (install directory)
set -e

REPO="abhskyd/finder"
VERSION="${FINDER_VERSION:-v0.1.1}"

# --- detect platform -------------------------------------------------------
os="$(uname -s)"
arch="$(uname -m)"
case "$os/$arch" in
  Darwin/arm64)  asset="finder-aarch64-apple-darwin.tar.gz" ;;
  Darwin/x86_64)
    echo "finder: no prebuilt binary for Intel macs — use Homebrew instead:"
    echo "  brew install abhskyd/tap/finder   (builds from source)"
    exit 1
    ;;
  *)
    echo "finder: no prebuilt binary for $os/$arch — use Homebrew instead:"
    echo "  brew install abhskyd/tap/finder   (builds from source)"
    exit 1
    ;;
esac

# --- pick an install prefix that's already on PATH -------------------------
if [ -n "${FINDER_PREFIX:-}" ]; then
  prefix="$FINDER_PREFIX"
elif [ -d /opt/homebrew/bin ] && [ -w /opt/homebrew/bin ]; then
  prefix=/opt/homebrew/bin
elif [ -d /usr/local/bin ] && [ -w /usr/local/bin ]; then
  prefix=/usr/local/bin
else
  prefix="$HOME/.local/bin"
fi
mkdir -p "$prefix"

# --- download + unpack -----------------------------------------------------
url="https://github.com/$REPO/releases/download/$VERSION/$asset"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Downloading finder $VERSION ($arch)…"
curl -fsSL "$url" | tar -xz -C "$tmp"

[ -f "$tmp/finder" ] || { echo "download failed"; exit 1; }
mv "$tmp/finder" "$prefix/finder"
chmod +x "$prefix/finder"

echo "Installed $( "$prefix/finder" --version ) → $prefix/finder"
case ":$PATH:" in
  *":$prefix:"*) ;;
  *) echo "NOTE: $prefix is not on your PATH — add it to your shell profile:" ;;
esac
