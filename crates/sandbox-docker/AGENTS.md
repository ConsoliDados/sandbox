# AGENTS.md — sandbox-docker

## Responsibility

Adapter that drives the local Docker daemon by **shelling out** to `docker` and `docker compose` (see ADR-0002). Everything that touches Docker lives here.

## Boundaries

- **Owns:**
  - `Plan` (pure data describing a `docker run` invocation)
  - `Compose` (pure data describing `docker compose up` of project deps)
  - `run`, `start`, `stop`, `exec`, `rm` operations
  - Network create / connect / disconnect
  - Named volume create / inspect / rm
  - Compose lifecycle (parse, validate-via-`sandbox-scan`, up, down)
- **Does not own:** decisions about whether to apply hardening (that's `sandbox-core::Profile`), or about whether scan blocks the run (that's `sandbox-cli`).
- **Depends on:** `sandbox-core`, `tokio`, `tracing`. **Not on `sandbox-scan` or `sandbox-proxy`** — those are sibling adapters consumed by the CLI.

## Layout (target shape — Phase 1+)

```
src/
├── lib.rs                  re-exports public API
├── error.rs                Error enum
├── plan.rs                 Plan struct (mounts, env, caps, network, etc.)
├── run.rs                  docker run / start / exec / stop / rm
├── network.rs              network create/connect/disconnect
├── volume.rs               named volume ops
├── compose/
│   ├── mod.rs              Compose struct + lifecycle
│   ├── parse.rs            compose file parser (subset of spec we care about)
│   └── validate.rs         security validator (calls into sandbox-scan)
└── cmd.rs                  Command builder helpers, --print-cmd formatter
```

Today (Phase 0): `lib.rs` only.

## Conventions

- **Build commands programmatically.** `Command::new("docker").args([...])`. Never `bash -c "..."`.
- **Every operation produces a `Plan` first**, then executes. The `Plan` is `Debug`-printable for `--print-cmd`.
- **Async everywhere** (this crate uses tokio). Use `tokio::process::Command`.
- **Capture stdout/stderr** for `tracing::debug!`. Don't pipe to inherit unless we're attaching the user's terminal (e.g. `docker exec -it`).
- **Detect daemon-down errors and surface them as `Error::DaemonUnreachable`.** Don't propagate raw IO errors with bad messages.
- **No `unwrap` on Command output.** Always check `status.success()`.
- **Tests need real Docker.** Mark them `#[cfg(feature = "docker-tests")]`. CI provides a daemon.

## Commands

```sh
cargo test -p sandbox-docker
cargo test -p sandbox-docker --features docker-tests   # requires docker daemon
```

## Points of attention

- `docker compose` v2 is the target. `docker-compose` v1 (Python) is unsupported. Detect via `docker compose version`.
- `docker run --network=internal-net` requires the network to exist. Always `ensure_network` before `run`.
- When connecting/disconnecting a running container's network, `docker network` works on running containers, but the *first* network must be specified at `docker run` time. Plan accordingly.
- `--user $(id -u):$(id -g)` requires the host UID/GID to exist inside the image OR `--user` accepts numeric. Use numeric.
