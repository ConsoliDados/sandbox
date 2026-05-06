#!/bin/zsh
# Run unit + integration tests across the workspace.
# Pass --features docker-tests to include tests that require a live Docker daemon.
set -euo pipefail
cd "$(dirname "$0")/../.."

cargo test --workspace "$@"
