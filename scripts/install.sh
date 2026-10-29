#!/bin/sh
# Install kontext into ~/.local/bin (override with KONTEXT_BIN_DIR, pin with KONTEXT_VERSION=v0.1.0).
# Downloads the prebuilt release for this platform and verifies it against the release's SHA256SUMS.
# Builds from source with cargo only when asked: --from-source (or KONTEXT_FROM_SOURCE=1).
#
#   curl -fsSL https://raw.githubusercontent.com/SemanS/kontext/main/scripts/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/SemanS/kontext/main/scripts/install.sh | sh -s -- --from-source
set -eu
REPO="${KONTEXT_REPO:-SemanS/kontext}"
BIN_DIR="${KONTEXT_BIN_DIR:-$HOME/.local/bin}"
VERSION="${KONTEXT_VERSION:-latest}"
FROM_SOURCE="${KONTEXT_FROM_SOURCE:-}"

for arg in "$@"; do
  case "$arg" in
    --from-source) FROM_SOURCE=1 ;;
    --version=*) VERSION="${arg#--version=}" ;;
    -h | --help)
      echo "usage: install.sh [--from-source] [--version=vX.Y.Z]  (env: KONTEXT_VERSION, KONTEXT_BIN_DIR, KONTEXT_FROM_SOURCE)"
      exit 0
      ;;
    *) echo "kontext install: unknown option $arg (use --from-source or --version=vX.Y.Z)" >&2; exit 2 ;;
  esac
done

fail() {
  echo "kontext install: $*" >&2
  exit 1
}

command -v curl >/dev/null 2>&1 || fail "curl is required"

# `latest` becomes the newest release tag, so a source build and the message name a real release.
if [ "$VERSION" = latest ]; then
  effective=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest") \
    || fail "could not reach github.com to find the latest release of $REPO (network or rate limit); retry, or set KONTEXT_VERSION=vX.Y.Z"
  VERSION="${effective##*/}"
  case "$VERSION" in
    v[0-9]*) ;;
    *) fail "could not resolve the latest release of $REPO (got '$effective'); set KONTEXT_VERSION=vX.Y.Z" ;;
  esac
fi

mkdir -p "$BIN_DIR"

if [ -n "$FROM_SOURCE" ]; then
  command -v cargo >/dev/null 2>&1 || fail "--from-source needs cargo; install Rust from https://rustup.rs"
  echo "kontext: building $VERSION from source…" >&2
  cargo install --locked --git "https://github.com/$REPO" --tag "$VERSION" --root "$(dirname "$BIN_DIR")" kontext
else
  case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) target=aarch64-apple-darwin ;;
    Darwin-x86_64) target=x86_64-apple-darwin ;;
    Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
    Linux-aarch64 | Linux-arm64) target=aarch64-unknown-linux-gnu ;;
    *) fail "no prebuilt binary for $(uname -s)-$(uname -m); rerun with --from-source (needs Rust, https://rustup.rs)" ;;
  esac

  base="https://github.com/$REPO/releases/download/$VERSION"
  asset="kontext-$target.tar.gz"
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT INT TERM

  # fetch URL FILE: 0 when downloaded, 4 when the release has no such file, else the network failed
  fetch() {
    code=$(curl -sSL -o "$2" -w '%{http_code}' "$1") || return 1
    case "$code" in
      200) return 0 ;;
      404) return 4 ;;
      *) return 1 ;;
    esac
  }

  status=0
  fetch "$base/$asset" "$tmp/$asset" || status=$?
  case "$status" in
    0) ;;
    4) fail "release $VERSION has no $asset; pick another KONTEXT_VERSION or rerun with --from-source" ;;
    *) fail "downloading $base/$asset failed (network or rate limit); nothing was installed, retry later" ;;
  esac

  if command -v sha256sum >/dev/null 2>&1; then
    sha() { sha256sum "$1" | cut -d' ' -f1; }
  elif command -v shasum >/dev/null 2>&1; then
    sha() { shasum -a 256 "$1" | cut -d' ' -f1; }
  else
    fail "neither sha256sum nor shasum is available to verify the download"
  fi

  # The archive is checked against SHA256SUMS; releases before SHA256SUMS existed carry a manifest
  # with the hash of the unpacked binary, which is checked after extraction instead.
  verified=""
  status=0
  fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS" || status=$?
  if [ "$status" = 0 ]; then
    want=$(awk -v f="$asset" '$2 == f || $2 == "*" f { print $1 }' "$tmp/SHA256SUMS")
    [ -n "$want" ] || fail "SHA256SUMS of $VERSION lists no $asset; not installing an unverified binary"
    got=$(sha "$tmp/$asset")
    [ "$got" = "$want" ] || fail "checksum mismatch for $asset ($got, SHA256SUMS says $want): the download is corrupt or was altered; nothing was installed"
    verified=archive
  elif [ "$status" != 4 ]; then
    fail "downloading $base/SHA256SUMS failed (network or rate limit); nothing was installed, retry later"
  fi

  mkdir -p "$tmp/x"
  tar -xzf "$tmp/$asset" -C "$tmp/x" || fail "could not unpack $asset; nothing was installed"
  [ -f "$tmp/x/kontext" ] || fail "$asset holds no kontext binary; nothing was installed"

  if [ -z "$verified" ]; then
    status=0
    fetch "$base/kontext-$target.manifest.json" "$tmp/manifest.json" || status=$?
    [ "$status" = 0 ] || fail "release $VERSION publishes neither SHA256SUMS nor a manifest to verify $asset against; not installing an unverified binary"
    want=$(sed -n 's/.*"sha256": *"\([0-9a-f]*\)".*/\1/p' "$tmp/manifest.json")
    [ -n "$want" ] || fail "the manifest of $VERSION has no sha256; not installing an unverified binary"
    got=$(sha "$tmp/x/kontext")
    [ "$got" = "$want" ] || fail "checksum mismatch for the kontext binary in $asset ($got, manifest says $want): the download is corrupt or was altered; nothing was installed"
  fi

  chmod +x "$tmp/x/kontext"
  # Run it before installing: a binary linked against a newer glibc than this system has fails here.
  if ! "$tmp/x/kontext" --version >/dev/null 2>"$tmp/run.err"; then
    cat "$tmp/run.err" >&2
    fail "the prebuilt $target binary of $VERSION cannot run on this system (often a glibc older than the one it was built against); nothing was installed. Rerun with --from-source"
  fi
  mv -f "$tmp/x/kontext" "$BIN_DIR/kontext"
fi

# Checked on its own line: a failing command inside echo's arguments would not stop `set -e`.
if ! installed=$("$BIN_DIR/kontext" --version 2>&1); then
  echo "$installed" >&2
  fail "$BIN_DIR/kontext was installed but cannot run on this system"
fi
echo "installed: $installed → $BIN_DIR/kontext"
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) echo "note: add $BIN_DIR to your PATH" ;;
esac
