#!/bin/sh
# Install kontext into ~/.local/bin (override with KONTEXT_BIN_DIR, pin with KONTEXT_VERSION=v0.1.0).
# Downloads a prebuilt release for this platform, or builds from source with cargo.
#
#   curl -fsSL https://raw.githubusercontent.com/SemanS/kontext/main/scripts/install.sh | sh
set -eu
REPO="${KONTEXT_REPO:-SemanS/kontext}"
BIN_DIR="${KONTEXT_BIN_DIR:-$HOME/.local/bin}"
VERSION="${KONTEXT_VERSION:-latest}"
mkdir -p "$BIN_DIR"

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) target=aarch64-apple-darwin ;;
  Darwin-x86_64) target=x86_64-apple-darwin ;;
  Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
  Linux-aarch64 | Linux-arm64) target=aarch64-unknown-linux-gnu ;;
  *) target="" ;;
esac

if [ "$VERSION" = latest ]; then
  url="https://github.com/$REPO/releases/latest/download/kontext-$target.tar.gz"
else
  url="https://github.com/$REPO/releases/download/$VERSION/kontext-$target.tar.gz"
fi

installed=""
if [ -n "$target" ] && command -v curl >/dev/null 2>&1; then
  tmp=$(mktemp -d)
  if curl -fsSL "$url" -o "$tmp/kontext.tar.gz" 2>/dev/null; then
    tar -xzf "$tmp/kontext.tar.gz" -C "$BIN_DIR"
    chmod +x "$BIN_DIR/kontext"
    installed=release
  fi
  rm -rf "$tmp"
fi

if [ -z "$installed" ]; then
  if command -v cargo >/dev/null 2>&1; then
    echo "kontext: no prebuilt binary for this platform/version, building from source…" >&2
    if [ "$VERSION" = latest ]; then
      cargo install --locked --git "https://github.com/$REPO" --root "$(dirname "$BIN_DIR")" kontext
    else
      cargo install --locked --git "https://github.com/$REPO" --tag "$VERSION" --root "$(dirname "$BIN_DIR")" kontext
    fi
  else
    echo "kontext: no prebuilt binary for $(uname -s)-$(uname -m); install Rust (https://rustup.rs) and rerun" >&2
    exit 1
  fi
fi

echo "installed: $("$BIN_DIR/kontext" --version) → $BIN_DIR/kontext"
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) echo "note: add $BIN_DIR to your PATH" ;;
esac
