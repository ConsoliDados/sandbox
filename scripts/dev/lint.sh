#!/bin/zsh
# Run formatter check + clippy across the workspace.
set -euo pipefail
cd "$(dirname "$0")/../.."

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
