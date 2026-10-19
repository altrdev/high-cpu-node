#!/bin/sh
# Installs the torchio binary of a release from GitHub: the one for this system, with its checksum
# verified.
#
#   curl -fsSL https://raw.githubusercontent.com/altrdev/torchio/main/install.sh | sh
#
# TORCHIO_VERSION      a tag, like v4.0.0 (default: the latest release)
# TORCHIO_INSTALL_DIR  where to put it (default: /usr/local/bin, or ~/.local/bin without root or sudo)
# TORCHIO_BASE_URL     where the release files are (default: the GitHub release), for mirrors and tests
set -eu

REPO="altrdev/torchio"

say() { printf '%s\n' "$*"; }
fail() { printf 'torchio install: %s\n' "$*" >&2; exit 1; }

case "${1:-}" in
  -h | --help)
    sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'
    exit 0
    ;;
esac

case "$(uname -s)" in
  Linux) os=linux ;;
  Darwin) os=macos ;;
  *) fail "unsupported system '$(uname -s)': torchio is for Linux and macOS" ;;
esac

case "$(uname -m)" in
  x86_64 | amd64) arch=amd64 ;;
  aarch64 | arm64) arch=arm64 ;;
  *) fail "unsupported architecture '$(uname -m)': torchio is for amd64 and arm64" ;;
esac

asset="torchio-$os-$arch"

if [ -n "${TORCHIO_BASE_URL:-}" ]; then
  base="$TORCHIO_BASE_URL"
elif [ -n "${TORCHIO_VERSION:-}" ]; then
  base="https://github.com/$REPO/releases/download/$TORCHIO_VERSION"
else
  base="https://github.com/$REPO/releases/latest/download"
fi

if command -v curl > /dev/null 2>&1; then
  fetch() { curl -fsSL "$1" -o "$2"; }
elif command -v wget > /dev/null 2>&1; then
  fetch() { wget -qO "$2" "$1"; }
else
  fail "curl or wget is needed to download the release"
fi

if command -v sha256sum > /dev/null 2>&1; then
  hash_of() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum > /dev/null 2>&1; then
  hash_of() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
  fail "sha256sum or shasum is needed to check the download"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT INT TERM

say "Downloading $asset from $base"
fetch "$base/$asset" "$tmp/$asset" || fail "cannot download $base/$asset"
fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS" || fail "cannot download $base/SHA256SUMS"

expected="$(grep " $asset\$" "$tmp/SHA256SUMS" | cut -d' ' -f1)"
[ -n "$expected" ] || fail "$asset is not in SHA256SUMS"
actual="$(hash_of "$tmp/$asset")"
[ "$expected" = "$actual" ] || fail "checksum mismatch for $asset (expected $expected, got $actual)"
chmod +x "$tmp/$asset"

# the system folder when we can write to it, else with sudo if there is one, else the home folder
dir="${TORCHIO_INSTALL_DIR:-}"
use_sudo=
if [ -z "$dir" ]; then
  dir=/usr/local/bin
  if [ ! -w "$dir" ] 2> /dev/null; then
    if [ "$(id -u)" != 0 ] && command -v sudo > /dev/null 2>&1; then
      use_sudo=sudo
    else
      dir="$HOME/.local/bin"
    fi
  fi
fi

run() { if [ -n "$use_sudo" ]; then sudo "$@"; else "$@"; fi; }
run mkdir -p "$dir" || fail "cannot create $dir"
run install -m 0755 "$tmp/$asset" "$dir/torchio" || fail "cannot write $dir/torchio: set TORCHIO_INSTALL_DIR to a folder you can write to"

say "Installed $dir/torchio"
case ":$PATH:" in
  *":$dir:"*) ;;
  *) say "$dir is not in your PATH: add it, or run $dir/torchio" ;;
esac
