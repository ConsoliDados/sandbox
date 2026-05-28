#!/bin/sh
# install.sh — installer for `sandbox`
#   https://github.com/ConsoliDados/sandbox
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/ConsoliDados/sandbox/main/install.sh | sh
#
# Pin a specific version:
#   curl -fsSL https://.../install.sh | SANDBOX_VERSION=v0.1.1 sh
#
# Override install dir (default: ~/.local/bin):
#   curl -fsSL https://.../install.sh | SANDBOX_INSTALL_DIR=/opt/bin sh
#
# Prefers a prebuilt binary from a GitHub Release (sandbox-<target>.tar.gz
# + SHA256SUMS); falls back to `cargo install` (build from source) only
# when no matching binary exists or the platform isn't supported.
set -eu

REPO="ConsoliDados/sandbox"
CRATE="sandbox-cli"        # crates.io package; ships the `sandbox` binary
BIN="sandbox"
INSTALL_DIR="${SANDBOX_INSTALL_DIR:-$HOME/.local/bin}"

say() { printf '%s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
err() { printf 'error: %s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

# Hash a file as sha256, using whichever tool is available.
# Echoes the bare hex digest (no filename suffix).
sha256_of() {
  if have shasum; then
    shasum -a 256 "$1" | cut -d' ' -f1
  elif have sha256sum; then
    sha256sum "$1" | cut -d' ' -f1
  else
    return 1
  fi
}

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

# Resolve the release tag to fetch. SANDBOX_VERSION wins if set.
# Otherwise queries the GitHub API for `releases/latest`.
resolve_tag() {
  if [ -n "${SANDBOX_VERSION:-}" ]; then
    printf '%s' "$SANDBOX_VERSION"
    return 0
  fi
  curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null \
    | grep -o '"tag_name"[ ]*:[ ]*"[^"]*"' | head -1 | cut -d'"' -f4
}

# Try to fetch a prebuilt binary + verify against SHA256SUMS.
# Returns non-zero if no release, no asset for this platform, or hash mismatch.
install_prebuilt() {
  target="$1"
  have curl || return 1
  have tar || return 1

  tag="$(resolve_tag)"
  [ -n "$tag" ] || return 1

  asset="$BIN-$target.tar.gz"
  url="https://github.com/$REPO/releases/download/$tag/$asset"
  sums_url="https://github.com/$REPO/releases/download/$tag/SHA256SUMS"
  tmp="$(mktemp -d)"

  if ! curl -fsSL "$url" -o "$tmp/$asset" 2>/dev/null; then
    rm -rf "$tmp"
    return 1
  fi

  # Verify against the aggregate SHA256SUMS. Missing SHA256SUMS is treated
  # as a hard error since releases produced by the workflow always include it.
  if curl -fsSL "$sums_url" -o "$tmp/SHA256SUMS" 2>/dev/null; then
    expected="$(grep -E "[ *]${asset}\$" "$tmp/SHA256SUMS" | head -1 | cut -d' ' -f1)"
    actual="$(sha256_of "$tmp/$asset" 2>/dev/null || true)"
    if [ -z "$expected" ] || [ -z "$actual" ] || [ "$expected" != "$actual" ]; then
      rm -rf "$tmp"
      err "SHA256 mismatch on $asset (expected '$expected', got '$actual'). Aborting."
    fi
    say "sha256 verified: $actual"
  else
    warn "SHA256SUMS not found in release — skipping integrity check (older release?)"
  fi

  tar -xzf "$tmp/$asset" -C "$tmp" || { rm -rf "$tmp"; return 1; }
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
  say "no prebuilt binary available — building from source with cargo…"
  if [ -n "${SANDBOX_VERSION:-}" ]; then
    # Strip the leading `v` for crates.io version syntax.
    ver="${SANDBOX_VERSION#v}"
    cargo install "$CRATE" --version "$ver"
  else
    cargo install "$CRATE"
  fi
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
