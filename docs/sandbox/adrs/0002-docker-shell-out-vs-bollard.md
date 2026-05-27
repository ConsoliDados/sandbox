# ADR-0002 — Docker integration via shell-out instead of bollard

- **Status:** Accepted
- **Date:** 2026-05-06
- **Phase:** 1

## Context

The `sandbox-docker` crate must drive a local Docker daemon to create networks, run containers, exec into them, manage named volumes, and orchestrate `docker compose` for project deps. Two integration approaches are viable:

1. **Shell out** to the `docker` and `docker compose` CLIs via `tokio::process::Command`.
2. **Use bollard**, a Rust async client that talks to the Docker engine API over a unix socket.

Constraints we care about:
- Transparency: every action must be representable as a command line for `--print-cmd`.
- Debuggability: when something fails, the user (or maintainer) should be able to reproduce by typing the same command.
- Compose support: project compose lifecycle (up, down, validate) is part of the scope.
- Dependency footprint: small is better.

## Decision

We will shell out to `docker` and `docker compose` via `tokio::process::Command`. No bollard, no direct socket calls.

## Alternatives considered

- **(a) bollard.** Rejected:
  - Pulls a large async-HTTP dependency tree (hyper, http, etc.).
  - API stability is tied to specific Docker engine versions; engine upgrades can break us.
  - `docker compose` is **not** part of the engine API — it's a CLI plugin. Using bollard would mean reimplementing compose semantics in Rust, which is out of scope and a maintenance hazard.
  - Less debuggable: when bollard returns an error, the maintainer cannot easily reproduce by running a CLI command.
- **(b) Direct unix socket HTTP calls.** Rejected: same problems as bollard plus we'd be reimplementing bollard.

## Consequences

Positive:
- Every `docker` invocation is a literal `Command::new("docker").args([...])`. The same args feed `--print-cmd`.
- `docker compose up`, `docker compose validate` (via parsing), `docker compose down` are first-class — no reimplementation.
- Minimal dep footprint: tokio is the only async dep we need for this crate.
- Failures surface as standard process errors; capturing stdout/stderr for `tracing::debug!` is straightforward.

Negative:
- Process spawn overhead per call. Acceptable: per-`sandbox run` we spawn ~5–10 docker processes total, not thousands.
- Parsing structured output requires `--format json` everywhere (e.g. `docker inspect --format '{{json .}}'`). Pure-string parsers are forbidden.
- Calling code is async-friendly but not type-checked at compile time the way bollard's API would be. Mitigated by an integration test suite gated behind `--features docker-tests`.

## Migration path

If we ever need bollard (e.g. a hypothetical `sandbox watch` that streams container events with low latency), we can add it as a parallel adapter behind the same `sandbox-docker` public API. The `Plan` data model is independent of the executor.

## References

- `crates/sandbox-docker/AGENTS.md`
- ADR-0001 (Rust binary CLI)
