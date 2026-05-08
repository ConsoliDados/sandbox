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

## Layout

```
src/
├── lib.rs                  re-exports public API
├── error.rs                Error enum
├── cmd.rs                  Command builder helpers; daemon-down detection
├── plan.rs                 Plan struct + UserSpec, Mount, NetworkSpec, etc.
├── volume.rs               named volume ops (ensure / exists / remove)
├── network.rs              --internal network create + connect/disconnect
├── lifecycle.rs            container ops: exists / is_running / run / start / stop / exec / rm
└── compose/                (Phase 6 — not yet present)
    ├── mod.rs              Compose struct + lifecycle
    ├── parse.rs            compose file parser (subset)
    └── validate.rs         security validator (calls into sandbox-scan)
```

Phase 1 ships `Plan`, `lifecycle`, `volume`, `network`. Compose support lands
in Phase 6 (ADR-0010).

## Conventions

- **Build commands programmatically.** `Command::new("docker").args([...])`. Never `bash -c "..."`.
- **Every operation produces a `Plan` first**, then executes. The `Plan` is `Debug`-printable for `--print-cmd`.
- **Async everywhere** (this crate uses tokio). Use `tokio::process::Command`.
- **Capture stdout/stderr** for `tracing::debug!`. Don't pipe to inherit unless we're attaching the user's terminal (e.g. `docker exec -it`).
- **Detect daemon-down errors and surface them as `Error::DaemonUnreachable`.** Don't propagate raw IO errors with bad messages.
- **No `unwrap` on Command output.** Always check `status.success()`.
- **Tests need real Docker.** The `Plan` itself has unit tests (no daemon
  required). Tests that drive `docker` for real are in `sandbox-cli/tests/` and
  gated behind the `docker-tests` feature there. CI provides the daemon.
- **No `expect`/`unwrap` even in tests.** Use the `?`-returning test pattern
  with `Result<(), Box<dyn Error>>`; `assert!`, `assert_eq!`, `assert_ne!`,
  `assert_matches!` are the only acceptable panic forms (per playbook § 6.5).

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
