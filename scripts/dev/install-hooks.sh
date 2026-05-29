#!/bin/sh
# Install lefthook **project-locally** and wire git hooks to use it.
#
# Why project-local: pinned version per repo (no system/global drift),
# zero new system deps (no `cargo install lefthook` user-wide, no Node).
# The binary is downloaded to `./bin/lefthook` (gitignored) and the git
# hooks (`.githooks/`) are tiny shims that exec it.
#
# Re-run safely: idempotent. Pins LEFTHOOK_VERSION below.
set -eu

LEFTHOOK_VERSION="2.1.8"

repo_root="$(git rev-parse --show-toplevel 2>/dev/null)" || {
  echo "error: not inside a git repo" >&2
  exit 1
}
cd "$repo_root"

uname_s="$(uname -s)"
uname_m="$(uname -m)"

case "$uname_s" in
  Linux)  os="Linux"  ;;
  Darwin) os="MacOS"  ;;
  *)      echo "error: unsupported OS: $uname_s" >&2; exit 1 ;;
esac

case "$uname_m" in
  x86_64|amd64)   arch="x86_64" ;;
  aarch64|arm64)
    # Lefthook ships both `Linux_aarch64` and `Linux_arm64`; the asset
    # names align with the kernel arch string.
    if [ "$os" = "Linux" ]; then arch="aarch64"; else arch="arm64"; fi
    ;;
  *) echo "error: unsupported arch: $uname_m" >&2; exit 1 ;;
esac

asset="lefthook_${LEFTHOOK_VERSION}_${os}_${arch}"
url="https://github.com/evilmartians/lefthook/releases/download/v${LEFTHOOK_VERSION}/${asset}"

mkdir -p bin
target="bin/lefthook"

if [ -x "$target" ] && "$target" version 2>/dev/null | grep -q "$LEFTHOOK_VERSION"; then
  echo "lefthook v${LEFTHOOK_VERSION} already present at $target"
else
  echo "downloading $asset ..."
  curl -fsSL -o "$target.tmp" "$url"
  chmod +x "$target.tmp"
  mv "$target.tmp" "$target"
  echo "installed lefthook v${LEFTHOOK_VERSION} -> $target"
fi

# Point git at the in-repo hooks dir. The shims there call ./bin/lefthook.
git config --local core.hooksPath .githooks

# Make sure shims are executable (git won't run non-x files).
chmod +x .githooks/* 2>/dev/null || true

echo
echo "✓ hooks wired (core.hooksPath = .githooks)"
echo "  pre-commit: cargo fmt --check"
echo "  pre-push  : scripts/dev/lint.sh + scripts/dev/test.sh"
echo
echo "Skip a single push: LEFTHOOK=0 git push"
echo "Uninstall:          git config --unset core.hooksPath  &&  rm -rf bin/"
