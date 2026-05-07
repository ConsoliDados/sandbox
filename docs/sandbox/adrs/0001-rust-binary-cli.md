# ADR-0001 — Rewrite as Rust binary CLI (vs evolving the shell script)

- **Status:** Accepted
- **Date:** 2026-05-06
- **Phase:** 1

## Context

The previous tool, `~/Dev/docker-sandbox/sandbox.sh`, is ~100 lines of zsh that wraps `docker compose run`. It works for the original scope (auto-detect language, mount, attach) but the new requirements push it past what shell can sensibly express:

- Subcommand UX (`run`, `down`, `nuke`, `ps`, `logs`, `exec`, `net`, `scan`, `lang`, `proxy`, `config`).
- Type-safe argument parsing with strict validation.
- Per-project state on disk (container hash, ports, volumes, scan cache).
- Pre-flight security scan with rule engine + heuristics + cache.
- Reverse proxy lifecycle and Traefik label generation.
- Docker network ops (create, connect, disconnect at runtime).
- Compose file validation against an allowlist.

Shell's lack of types, brittle error handling, and weak structuring would make the project unmaintainable at this scope.

## Decision

We will rewrite the tool as a Rust workspace producing a single binary called `sandbox`. Distribution via `cargo install --git` initially; pre-built releases when the project goes open source.

## Alternatives considered

- **(a) Evolve the shell script.** Rejected: error handling is brittle; flag parsing in zsh is painful; no type system to enforce security invariants (e.g. "this code path must never produce a `docker run` without `--cap-drop=ALL`").
- **(b) Python with click/typer.** Rejected: requires a Python runtime on the host; not meaningfully simpler than Rust at this scope; no static guarantees.
- **(c) Go.** Rejected: not the maintainer's preferred stack; no compelling advantage over Rust for this workload.

## Consequences

Positive:
- Type-safe CLI surface (clap derive).
- Lint-enforced invariants at workspace level (`forbid unsafe`, `warn unwrap`/`expect`/`panic`).
- Clean separation between "build a Plan" and "execute a Plan" → enables `--print-cmd` and `--dry-run` for free.
- Async via tokio for concurrent Docker calls (parallel volume creation, parallel network setup).
- Cargo's strong dependency model (lockfile committed) enables reproducible builds.

Negative:
- Build/distribution more complex than `chmod +x sandbox.sh` (mitigated: `cargo install --git` is one command).
- Compile time on first install (mitigated: small workspace, `release` profile uses `lto = "thin"`).
- Less directly readable than a shell script for casual auditors. Mitigated by: every Docker action is representable as a printable command line via `--print-cmd`; the binary's behavior is reproducible from source.

## References

- ADR-0002 (Docker integration approach: shell-out, preserves transparency)
- `../threat-model.md` (security posture this rewrite enables)
- `~/Dev/docker-sandbox/sandbox.sh` (legacy reference, frozen)
