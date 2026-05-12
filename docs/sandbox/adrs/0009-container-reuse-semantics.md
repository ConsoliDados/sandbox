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

**Container identity is a function of the project's canonical filesystem path.**

```
container_name = "sandbox-" + sha256(canonical_path_bytes).hex()[..12]
```

`canonical_path` is the absolute, symlink-resolved path of the project directory (`std::fs::canonicalize`).

**Why path and not file content?** Two reasons:

1. **Stability across normal development.** Adding, removing, or renaming files during a dev session is constant. A content-based hash would force a fresh container every time the user creates a file, losing `node_modules` and other named volume state. The container should track *the workspace*, not *the current set of files*.
2. **Separation of concerns.** "Same workspace" (path) and "same source content" (hash of files) are different questions. The scanner (`sandbox-scan`) uses a separate **content hash** to invalidate its cache when sources change — that's the right place for content sensitivity. The container reuse decision is about workspace identity.

**Lifecycle semantics:**

| Command | Container running | Container stopped | Container missing |
|---|---|---|---|
| `sandbox run` | `docker exec` shell | `docker start` then exec | create new |
| `sandbox down` | stop | no-op | error (nothing to stop) |
| `sandbox nuke` | stop + rm + remove volumes + remove state | rm + remove volumes + remove state | error unless `--all` |

**`--rebuild`** flag forces image rebuild and container recreation, preserving named volumes by default.

## Alternatives considered

- **(a) Always recreate on `run`.** Rejected: user explicitly preferred reuse; package installs are slow.
- **(b) Hash by `git ls-files` (file list).** Rejected: file list changes constantly during normal dev (new files, renames). Container would rebuild on every file add and the user would lose state. The original draft of this ADR had this; correcting before implementation.
- **(c) Hash by file list + content.** Rejected for the same reason as (b), worse: any file save would change the hash. Content-sensitive hashing belongs in the scan cache, not container identity.
- **(d) User-supplied container name (`sandbox run --name foo`).** Acceptable as a future override flag; not the default.
- **(e) Hash includes manifest content.** Considered: would auto-rotate the container when the language manifest changes. Rejected for v0.1: manifest changes are rare and `--rebuild` covers them. Reconsider in Phase 7.

## Consequences

Positive:
- `cargo install` cache survives across runs (named volume on `target/`).
- pnpm store, `node_modules`, `.venv` survive — fast iteration.
- Per-project state (`$XDG_DATA_HOME/sandbox/containers/<hash>/`) has a stable key.

Negative:
- **Same repo cloned to two paths = two containers.** Acceptable behavior: each working tree has its own `target/`, `node_modules/`, etc. on disk; sharing a container would force them to share named volumes, which would be a bug, not a feature.
- **Renaming the project directory rotates the container.** The user can `sandbox nuke` the old one or migrate state manually. Surfaced via a clear `sandbox ps --all` view.
- **Hash collision** at 48 bits is theoretically possible (~16M projects → 50% chance by birthday paradox). In practice negligible for personal use; surfaced as a clean error if it ever happens (`docker run` will fail with name conflict). For OSS release, consider 16-char prefix (64 bits).
- **Stale base image** if the upstream `node:24.10.0` is rotated by Docker Hub (mutable tag). Mitigated: the language manifest pins a specific tag; `--rebuild` regenerates with the latest pull.
- **Container starts as immutable** for hardening flags (caps, network mode); changing them requires `nuke` + `run`. Acceptable: those are infrequent operations.

## References

- `../srs.md` `run` / `down` / `nuke` sections
- `../open-questions.md` OQ-001 (rebuild policy on Dockerfile change)
- `crates/sandbox-core/AGENTS.md` (Project, ProjectHash)
