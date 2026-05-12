# sandbox

Isolated, **secure-by-default** development environments in Docker for untrusted code (job interview challenges, OSS contributions, AI-generated code, etc.).

> **Status:** 🟡 Phase 1 — lifecycle MVP. `sandbox run`/`down`/`nuke` are wired against a real Docker daemon; everything else is staged in later phases. See [`docs/sandbox/roadmap.md`](docs/sandbox/roadmap.md).

## Why

Born after a [Contagious Interview / DPRK Lazarus](docs/sandbox/threat-model.md#real-world-incident) incident where the previous shell-script `sandbox` (volume mount + no other isolation) almost let a payload persist on the host via the project directory.

The premise: **paranoid by default**. Unsafe behavior is opt-in, not opt-out.

## Quickstart

```sh
# Auto-detect lang, secure mode (RO source, no internet, scan first)
sandbox run .

# Trust this project — full read/write, full network
sandbox run . --unsafe

# Trust just the network for this run
sandbox run . --network

# Scan only (no run) — Phase 4
sandbox scan .
```

## Subcommands

| Command | Purpose |
|---|---|
| `sandbox run [PATH]` | Start (or resume) sandbox for a project |
| `sandbox down [PROJECT]` | Stop sandbox container; keep state |
| `sandbox nuke [PROJECT]` | Remove container + named volumes + state |
| `sandbox ps` | List active sandboxes and their deps |
| `sandbox logs PROJECT` | Tail sandbox container logs |
| `sandbox exec PROJECT -- CMD` | Run a one-shot command inside sandbox |
| `sandbox net on\|off PROJECT` | Toggle internet egress at runtime |
| `sandbox scan [PATH]` | Run security scan without launching |
| `sandbox lang list\|show NAME\|add FILE` | Manage language manifests |
| `sandbox proxy start\|stop\|status` | Control reverse proxy sidecar |
| `sandbox config edit\|show\|path` | Edit/show config |

Full surface and semantics in [`docs/sandbox/srs.md`](docs/sandbox/srs.md).

## Repository shape

```
sandbox/
├── crates/
│   ├── sandbox-cli/      bin (clap, subcommand dispatch)
│   ├── sandbox-core/     domain types, project hash, lifecycle, profiles
│   ├── sandbox-docker/   docker shell-out, compose lifecycle, network ops
│   ├── sandbox-scan/     YARA + heuristics, scan cache
│   └── sandbox-proxy/    Traefik labels, sidecar lifecycle
├── docs/sandbox/                 architecture, playbook, threat model, ADRs
├── languages/            TOML manifests (node, bun, rust, ...)
└── scripts/dev/          lint, test, fmt helpers
```

Each crate has its own `AGENTS.md` describing responsibility and conventions.

## Supported platforms

- **Linux** — primary target. Native Docker engine. The MVP runs and is tested here.
- **macOS** — next target after the MVP. Unix-like; expected to work with Docker Desktop after small adjustments (UID mapping, path normalisation).
- **WSL2** — conditional future support, only with Docker installed inside the WSL distribution (Docker Desktop's WSL backend has different mount semantics).
- **Windows (native)** — out of scope.

## Development

### Build, test, lint

```sh
cargo build --workspace                                       # build all crates
cargo test  --workspace                                       # 49 passing on Phase 1
cargo fmt   --check                                           # silent = clean
cargo clippy --workspace --all-targets -- -D warnings         # silent = clean
bash scripts/dev/lint.sh                                      # combines fmt + clippy
```

Tests that need a live local Docker daemon are gated behind a feature flag:

```sh
cargo test -p sandbox-cli --features docker-tests             # requires docker
```

### Smoke test — `--print-cmd` (no Docker needed)

`--print-cmd` short-circuits before any Docker shell-out and prints the
exact `docker run …` invocation the orchestrator would issue. Useful for
inspecting the rendered Plan without launching anything.

**Node project:**

```sh
mkdir -p /tmp/sb-node && echo '{"name":"itest"}' > /tmp/sb-node/package.json
cargo run -p sandbox-cli -- --print-cmd run /tmp/sb-node
cargo run -p sandbox-cli -- --print-cmd run /tmp/sb-node --unsafe
cargo run -p sandbox-cli -- --print-cmd run /tmp/sb-node --network
```

**Bun project** (auto-detected over node when `bun.lock` is present, per `priority` in the manifest):

```sh
mkdir -p /tmp/sb-bun && echo '{"name":"itest"}' > /tmp/sb-bun/package.json && touch /tmp/sb-bun/bun.lock
cargo run -p sandbox-cli -- --print-cmd run /tmp/sb-bun
```

You should see:
- `--volume /tmp/sb-…:/app:ro` (source RO unless `--unsafe`)
- one `--volume sandbox-<hash>-<dir>` per `package_dirs` entry from the manifest
- `--tmpfs /home/sandbox` + `~/.zshrc` and `~/.config/starship.toml` bound RO under `/home/sandbox/`
- `--network sandbox-internal` (no internet) unless `--unsafe`/`--network`
- `--cap-drop ALL --security-opt no-new-privileges --user $(id -u):$(id -g)`
- `--cpus 2 --memory 4096m` (from the `default` profile)
- ends with `<image> /bin/zsh`

### Live test — full lifecycle (needs Docker)

Drops you into a real container shell. First run pulls the language image
(`node:24.10.0`, `oven/bun:1.3.6`, …) which can take a minute.

```sh
cargo run -p sandbox-cli -- run /tmp/sb-node          # creates + attaches zsh
# inside the sandbox shell:
ls -la /app                                            # see your package.json
bun i --frozen-lockfile                                # works in safe; lockfile is RO
exit                                                   # leaves container running

cargo run -p sandbox-cli -- down /tmp/sb-node         # stop, keep volumes/state
cargo run -p sandbox-cli -- nuke /tmp/sb-node         # remove container + volumes + state
```

Per-project state lives at `~/.local/share/sandbox/containers/<hash>/meta.toml`.

## Documentation

Priority reading order (see [`AGENTS.md`](AGENTS.md) for the canonical chain):

1. [`docs/sandbox/threat-model.md`](docs/sandbox/threat-model.md) — what we defend against (and don't)
2. [`docs/sandbox/srs.md`](docs/sandbox/srs.md) — CLI surface and semantics
3. [`docs/sandbox/sad.md`](docs/sandbox/sad.md) — architecture and crate boundaries
4. [`docs/sandbox/playbook.md`](docs/sandbox/playbook.md) — code conventions
5. [`docs/sandbox/roadmap.md`](docs/sandbox/roadmap.md) — phases and current status
6. [`docs/sandbox/adrs/`](docs/sandbox/adrs/) — architectural decisions (drafted, not all filled)

## Author

Johnny Carreiro — São Paulo, Brazil. Tooling for personal use; will be open-sourced once stable.
