#!/bin/zsh
# Apply rustfmt across the workspace.
set -euo pipefail
cd "$(dirname "$0")/../.."

cargo fmt --all
