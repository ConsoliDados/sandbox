# sandbox

Isolated, **secure-by-default** development environments in Docker for untrusted code (job interview challenges, OSS contributions, AI-generated code, etc.).

> **Status:** 🟡 Phase 0 — workspace skeleton. Not functional yet. See [`docs/sandbox/roadmap.md`](docs/sandbox/roadmap.md).

## Why

Born after a [Contagious Interview / DPRK Lazarus](docs/sandbox/threat-model.md#real-world-incident) incident where the previous shell-script `sandbox` (volume mount + no other isolation) almost let a payload persist on the host via the project directory.

The premise: **paranoid by default**. Unsafe behavior is opt-in, not opt-out.

## Quickstart (will work after Phase 1)

```sh
# Auto-detect lang, secure mode (RO source, no internet, scan first)
sandbox run .

# Trust this project — full read/write, full network
sandbox run . --unsafe

# Trust just the network for this run
sandbox run . --network

# Scan only (no run)
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
