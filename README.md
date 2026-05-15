# sandbox

Isolated, **secure-by-default** development environments in Docker for untrusted code (job interview challenges, OSS contributions, AI-generated code, etc.).

> **Status:** 🟢 Phases 1–5 shipped on `dev` — full lifecycle, observability, three-motor scan pipeline (YARA + heuristics + compose + ClamAV), and a Traefik reverse proxy with `<slug>.sandbox.local:<PORT>` routing. See [`docs/sandbox/roadmap.md`](docs/sandbox/roadmap.md).

## Why

Born after a [Contagious Interview / DPRK Lazarus](docs/sandbox/threat-model.md#real-world-incident) incident where the previous shell-script `sandbox` (volume mount + no other isolation) almost let a payload persist on the host via the project directory.

The premise: **paranoid by default**. Unsafe behavior is opt-in, not opt-out.

## Quickstart

```sh
# Auto-detect lang, secure mode (RO source, no internet, pre-flight scan first)
sandbox run .

# Audit-only — no container, full scan report
sandbox scan . --explain

# Add the AV motor (requires `sandbox scan --update-db` once first)
sandbox scan . --with-clamav

# Trust the project — full read/write, full network, scan skipped
sandbox run . --unsafe

# Trust just the network for this run
sandbox run . --network
```

## Subcommands

| Command | Purpose |
|---|---|
| `sandbox run [PATH]` | Start (or resume) sandbox for a project (pre-flight scan in safe/paranoid) |
| `sandbox down [PROJECT]` | Stop sandbox container; keep state |
| `sandbox nuke [PROJECT]` | Remove container + named volumes + state (`-y` skips prompt) |
| `sandbox ps [--all] [--format json\|table]` | List sandboxes |
| `sandbox logs PROJECT [-f] [--tail N] [--since DUR]` | Tail sandbox container logs |
| `sandbox exec PROJECT [--user U] [--workdir P] -- CMD` | Run a command inside the running sandbox |
| `sandbox scan [PATH] [--with-clamav] [--explain] [--format json\|table]` | Run security scan without launching |
| `sandbox scan --update-db` | Refresh ClamAV signature DB |
| `sandbox net on\|off PROJECT` | Toggle internet egress at runtime (Phase 6) |
| `sandbox lang list\|show NAME\|add FILE` | Manage language manifests (Phase 7) |
| `sandbox proxy start\|stop\|status\|logs` | Control the Traefik reverse proxy sidecar |
| `sandbox run --expose PORT...` | Override port detection (proxy entryPoints) |
| `sandbox config edit\|show\|path` | Edit/show config (Phase 7) |

Full surface and semantics in [`docs/sandbox/srs.md`](docs/sandbox/srs.md). Exit codes are documented there too (notably 30 for scan-blocked, 40 for container-not-found, 20 for ClamAV DB missing).

## Repository shape

```
sandbox/
├── crates/
│   ├── sandbox-cli/      bin (clap, subcommand dispatch)
│   ├── sandbox-core/     domain types, project hash, lifecycle, profiles
│   ├── sandbox-docker/   docker shell-out, compose lifecycle, network ops, scanner
│   ├── sandbox-scan/     YARA + heuristics + compose validator + ClamAV parser + cache
│   └── sandbox-proxy/    Traefik labels, sidecar lifecycle (Phase 5)
├── docs/sandbox/                 architecture, playbook, threat model, ADRs, smoke tests
├── languages/            TOML manifests (node, bun, rust, ...)
└── scripts/dev/          lint, test, fmt helpers
```

Each crate has its own `AGENTS.md` describing responsibility and conventions.

## Supported platforms

- **Linux** — primary target. Native Docker engine. Tested here.
- **macOS** — next target after the MVP. Unix-like; expected to work with Docker Desktop after small adjustments (UID mapping, path normalisation).
- **WSL2** — conditional future support, only with Docker installed inside the WSL distribution (Docker Desktop's WSL backend has different mount semantics).
- **Windows (native)** — out of scope.

## Development

### Build, test, lint

```sh
cargo build --workspace                                       # build all crates
cargo test  --workspace                                       # 172 passing on Phase 5
cargo fmt   --check                                           # silent = clean
cargo clippy --workspace --all-targets -- -D warnings         # silent = clean
bash scripts/dev/lint.sh                                      # combines fmt + clippy
```

Tests that need a live local Docker daemon are gated behind a feature flag:

```sh
cargo test -p sandbox-cli --features docker-tests             # requires docker
```

### Smoke tests

The full recipe collection — every shipped feature, every "vulnerable" fixture, every expected output — lives in [`docs/sandbox/smoke-tests.md`](docs/sandbox/smoke-tests.md). Start there when you want to verify a feature end-to-end without reading the implementation.

A few quick ones to get the feel:

```sh
SB=$(pwd)/target/debug/sandbox

# Phase 1 — print-cmd on a fresh node project (no Docker needed):
mkdir -p /tmp/sb-node && echo '{"name":"x"}' > /tmp/sb-node/package.json
$SB --print-cmd run /tmp/sb-node

# Phase 4 — synthetic Lazarus-shape detection (no Docker needed):
mkdir -p /tmp/sb-evil && cat > /tmp/sb-evil/server.js <<'EOF'
const _ = new (Function.constructor)('require','m','...');
const c2 = 'Y2hhaW5saW5rLWFwaS12My5saXY=';
const endpoint = '/api/service/token/abc';
EOF
$SB scan /tmp/sb-evil --explain        # → 3 findings (critical+high+high), exit 30

# Phase 4b — EICAR validates the ClamAV motor (needs Docker + `--update-db` once):
$SB scan --update-db
mkdir -p /tmp/sb-eicar
printf 'X5O!P%%@AP[4\\PZX54(P^)7CC)7}$EICAR-STANDARD-ANTIVIRUS-TEST-FILE!$H+H*' > /tmp/sb-eicar/test.com
$SB scan /tmp/sb-eicar --with-clamav --explain    # → clamav/Win.Test.EICAR_HDB-1 (critical)
```

See [`smoke-tests.md`](docs/sandbox/smoke-tests.md) for full coverage (lifecycle, ps/logs/exec, exit codes, vscode autorun, package.json supply-chain shapes, compose audits, suppression syntax, Traefik proxy + `<slug>.sandbox.local:<PORT>` routing).

## Documentation

Priority reading order (see [`AGENTS.md`](AGENTS.md) for the canonical chain):

1. [`docs/sandbox/threat-model.md`](docs/sandbox/threat-model.md) — what we defend against (and don't)
2. [`docs/sandbox/srs.md`](docs/sandbox/srs.md) — CLI surface and semantics
3. [`docs/sandbox/sad.md`](docs/sandbox/sad.md) — architecture and crate boundaries
4. [`docs/sandbox/playbook.md`](docs/sandbox/playbook.md) — code conventions
5. [`docs/sandbox/roadmap.md`](docs/sandbox/roadmap.md) — phases and current status
6. [`docs/sandbox/smoke-tests.md`](docs/sandbox/smoke-tests.md) — recipes for verifying every feature
7. [`docs/sandbox/usage-flow.md`](docs/sandbox/usage-flow.md) — the trust-dial as a user story
8. [`docs/sandbox/adrs/`](docs/sandbox/adrs/) — architectural decisions

## Author

Johnny Carreiro — São Paulo, Brazil. Tooling for personal use; will be open-sourced once stable.
