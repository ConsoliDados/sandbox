# ADR-0009 — Container reuse semantics for `run` / `down` / `nuke`

- **Status:** Accepted
- **Date:** 2026-05-06
- **Phase:** 1

## Context

When a user runs `sandbox run .` twice on the same project, what should happen? Two extremes:

1. **Always create a new container.** Cleanest state, slowest iteration (image rebuild, volume reinit, package reinstall).
2. **Reuse a project-scoped container.** Fast iteration, but stale state risks (e.g. user changed manifest, base image rotated).

The user explicitly asked for reuse: "se dermos um sandbox run novamente ele reutilza container, assim não recriamos o container toda hora para o mesmo projeto."

We also need to decide: what scopes "the same project"? Path? Hash? Both?

## Decision

**Container identity is a function of project hash.**

```
container_name = "sandbox-" + sha256(project_hash_inputs)[..12]
```

Where `project_hash_inputs` is the canonical sha256 of `git ls-files` (sorted) of the project, with a fallback to walkdir excluding `package_dirs` if the project is not a git repo.

**Lifecycle semantics:**

| Command | Container running | Container stopped | Container missing |
|---|---|---|---|
| `sandbox run` | `docker exec` shell | `docker start` then exec | create new |
| `sandbox down` | stop | no-op | error (nothing to stop) |
| `sandbox nuke` | stop + rm + remove volumes + remove state | rm + remove volumes + remove state | error unless `--all` |

**`--rebuild`** flag forces image rebuild and container recreation, preserving named volumes by default.

## Alternatives considered

- **(a) Always recreate on `run`.** Rejected: user explicitly preferred reuse; package installs are slow.
- **(b) Hash by absolute path instead of source content.** Rejected: same project at two paths (e.g. `~/dev/proj` and `~/projects/proj`) would become two containers, which is surprising.
- **(c) User-supplied container name (`sandbox run --name foo`).** Acceptable as a future override flag; not the default.
- **(d) Hash includes manifest content.** Considered: would auto-rotate the container when the language manifest changes. Rejected for v0.1: manifest changes are rare and `--rebuild` covers them. Reconsider in Phase 7.

## Consequences

Positive:
- `cargo install` cache survives across runs (named volume on `target/`).
- pnpm store, `node_modules`, `.venv` survive — fast iteration.
- Per-project state (`$XDG_DATA_HOME/sandbox/containers/<hash>/`) has a stable key.

Negative:
- **Hash collision** at 48 bits is theoretically possible (~16M projects → 50% chance by birthday paradox). In practice negligible for personal use; surfaced as a clean error if it ever happens (`docker run` will fail with name conflict). For OSS release, consider 16-char prefix (64 bits).
- **Stale base image** if the upstream `node:24.10.0` is rotated by Docker Hub (mutable tag). Mitigated: the language manifest pins a specific tag; `--rebuild` regenerates with the latest pull.
- **Container starts as immutable** for hardening flags (caps, network mode); changing them requires `nuke` + `run`. Acceptable: those are infrequent operations.

## References

- `docs/srs.md` `run` / `down` / `nuke` sections
- `docs/open-questions.md` OQ-001 (rebuild policy on Dockerfile change)
- `crates/sandbox-core/AGENTS.md` (Project, ProjectHash)
