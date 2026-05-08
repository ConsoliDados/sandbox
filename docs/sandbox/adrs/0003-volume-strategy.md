# ADR-0003 — Volume strategy: source, package dirs, and lockfiles per profile

- **Status:** Draft
- **Date:** 2026-05-07
- **Phase:** 2

## Context

A project sandboxed under `sandbox run .` has three classes of files that need different handling:

1. **Source tree** (`src/`, `package.json`, etc.) — the user's actual code, possibly hostile.
2. **Package directories** (`node_modules/`, `target/`, `.venv/`, `.cargo/`, `dist/`, …) — populated by package managers; opaque, large, possibly hostile.
3. **Lockfiles** (`bun.lockb`, `package-lock.json`, `pnpm-lock.yaml`, `yarn.lock`, `Cargo.lock`, …) — declarative, must be commitable, normally edited by package managers from inside the container.

The defaults must serve the **paranoid** scenario (untrusted repo from a recruiter / consulting client) while still letting a **trusted** scenario (validated project) feel like a regular dev environment with `git status` showing lockfile changes.

## Decision

**We will mount these three classes differently per profile.**

| Class | `safe` (default) | `paranoid` | `unsafe` |
|---|---|---|---|
| Source tree (`/app`) | bind mount **read-only** | bind mount **read-only** | bind mount **read-write** |
| Package dirs | **named volume** per dir | **named volume** per dir | **bind mount RW** under host project |
| Lockfiles | **named volume** | **named volume** | **bind mount RW** |

Naming conventions (consistent with `sandbox-core::project::NamedVolume`):

```
sandbox-<hash[..12]>-<sanitized_relpath>
```

For lockfiles, the sanitisation collapses the path to a flat segment (`bun.lockb` → `bun_lockb`).

Detection lives in the language manifest — `package_dirs` and a new `lock_files` array (TOML). Both are merged from manifest + user config.

## Alternatives considered

- **(a) Always named volumes (no profile differentiation).** Rejected: the trusted/unsafe flow needs commitable lockfiles and IDE-visible `node_modules`. A consultant who validated a client repo must be able to `git status` and see the lockfile change.
- **(b) Always bind mount.** Rejected: this is the docker-sandbox v0 behaviour that the threat model exists to prevent (T1, T2). The 2026-05-06 incident would have written to the project tree under that model.
- **(c) Symlink in the host project pointing at the Docker volume mount.** Rejected: Docker volume contents live under `/var/lib/docker/volumes/<name>/_data`, which requires root to read on Linux. Editors, `git`, and tooling on the host would hit `EACCES` on every operation. The bind mount in `unsafe` delivers the same observable result without the permission friction.
- **(d) Whitelist specific files (lockfile only) for RW even in safe mode.** Considered: keeps lockfile commitable without losing source RO. Rejected for v0.1: malware could write through the whitelist (e.g. crafted `bun.lockb`-named payload). Revisit if a clean implementation appears.

## Consequences

Positive:

- Default mode preserves the threat model (T2): source is RO, package dirs are isolated. Postinstall scripts run inside the container without touching host files.
- Unsafe mode behaves like a normal dev container: `bun i` writes a real `node_modules` and a real lockfile that `git diff` shows.
- The same `LangManifest` schema drives all profiles — `lock_files` is one new field, not a separate config tree.

Negative / open:

- **Lockfile commits in safe/paranoid require leaving the named volume.** Today there is no path. This couples to **OQ-002** (git inside read-only `/app`): the resolution there must include either an explicit "promote to unsafe to commit lockfile" step, an `exec git` from inside the container with a writable `.git`, or a one-shot `sandbox sync-lockfiles` command. To be decided when OQ-002 closes.
- **Switching profiles on the same project** (run safe, then run unsafe) creates two states: the named volume still has a lockfile from safe runs, but unsafe binds the host's (possibly empty) lockfile. Mitigation: on profile change, log a warning + offer `sandbox nuke --keep-state` to clear named volumes. Detail to settle in Phase 2.
- **`unsafe` writes to the host source tree.** Documented and intentional: `unsafe` is the "I trust this project" switch, in line with ADR-0009 and the threat model.

## References

- `../threat-model.md` T1, T2, T5
- `../srs.md` § `run` (`--unsafe`, `--profile`)
- ADR-0009 (container reuse — package_dirs survival)
- ADR-0006 (language manifest schema — `lock_files` is added here)
- `../open-questions.md` OQ-002 (commits in `/app` RO), OQ-003 (trust persistence)
