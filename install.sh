#!/bin/sh
# install.sh — installer for `sandbox`
#   https://github.com/JohnnyCarreiro/sandbox
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/JohnnyCarreiro/sandbox/main/install.sh | sh
#
# WIP: prefers a prebuilt binary from the latest GitHub Release; falls back to
# `cargo install` (build from source) until release binaries are published.
# Override the install dir with SANDBOX_INSTALL_DIR (default: ~/.local/bin).
set -eu

REPO="JohnnyCarreiro/sandbox"
CRATE="sandbox-cli"        # crates.io package; ships the `sandbox` binary
BIN="sandbox"
INSTALL_DIR="${SANDBOX_INSTALL_DIR:-$HOME/.local/bin}"

say() { printf '%s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
err() { printf 'error: %s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

# Map `uname` to a Rust target triple. Empty output = unsupported platform.
detect_target() {
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os" in
    Linux) os_part="unknown-linux-gnu" ;;
    Darwin) os_part="apple-darwin" ;;
    *) return 1 ;;
  esac
  case "$arch" in
    x86_64 | amd64) arch_part="x86_64" ;;
    aarch64 | arm64) arch_part="aarch64" ;;
    *) return 1 ;;
  esac
  printf '%s-%s' "$arch_part" "$os_part"
}

# Try to fetch a prebuilt binary from the latest GitHub Release.
# Release assets must be named: sandbox-<target>.tar.gz  (containing `sandbox`).
# Returns non-zero if there's no release or no asset for this platform.
install_prebuilt() {
  target="$1"
  have curl || return 1
  have tar || return 1
  tag="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null \
    | grep -o '"tag_name"[ ]*:[ ]*"[^"]*"' | head -1 | cut -d'"' -f4)"
  [ -n "$tag" ] || return 1
  url="https://github.com/$REPO/releases/download/$tag/$BIN-$target.tar.gz"
  tmp="$(mktemp -d)"
  if ! curl -fsSL "$url" -o "$tmp/pkg.tar.gz" 2>/dev/null; then
    rm -rf "$tmp"
    return 1
  fi
  tar -xzf "$tmp/pkg.tar.gz" -C "$tmp" || { rm -rf "$tmp"; return 1; }
  mkdir -p "$INSTALL_DIR"
  cp "$tmp/$BIN" "$INSTALL_DIR/$BIN"
  chmod 0755 "$INSTALL_DIR/$BIN"
  rm -rf "$tmp"
  say "installed $BIN $tag (prebuilt) → $INSTALL_DIR/$BIN"
  PATH_HINT_DIR="$INSTALL_DIR"
}

# Fallback: build + install from source. Installs to ~/.cargo/bin.
install_cargo() {
  have cargo || err "no prebuilt binary for your platform and cargo not found.
  Install Rust (https://rustup.rs) and re-run, or grab a binary from
  https://github.com/$REPO/releases"
  say "no prebuilt binary available yet — building from source with cargo…"
  cargo install "$CRATE"
  PATH_HINT_DIR="$HOME/.cargo/bin"
}

main() {
  have docker || warn "Docker not found on PATH — sandbox needs Docker (daemon running) + 'docker compose' v2 to actually run anything."

  PATH_HINT_DIR=""
  target="$(detect_target 2>/dev/null || true)"
  if [ -n "${target:-}" ] && install_prebuilt "$target"; then
    :
  else
    install_cargo
  fi

  case ":$PATH:" in
    *":$PATH_HINT_DIR:"*) ;;
    *) say "note: $PATH_HINT_DIR is not on your PATH. Add:
  export PATH=\"$PATH_HINT_DIR:\$PATH\"" ;;
  esac
  say "done — run: $BIN --help"
}

main "$@"
