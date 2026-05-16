# ADR-0003 — Volume strategy: source, package dirs, and lockfiles per profile

- **Status:** Accepted
- **Date:** 2026-05-08
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
| Lockfiles | **state-dir bind RW (file)** | **state-dir bind RW (file)** | covered by source bind RW |

Naming conventions for package-dir named volumes (consistent with `sandbox-core::project::NamedVolume`):

```
sandbox-<hash[..12]>-<sanitized_relpath>
```

Detection lives in the language manifest — `package_dirs` and `lock_files` (TOML arrays). Both are merged from manifest + user config.

### Lockfile mount mechanics (safe / paranoid)

Named Docker volumes only mount at directory paths, so mounting one over a single file (e.g. `/app/bun.lock`) does not work without an init container or `docker cp` seed step. Instead, lockfiles in `safe`/`paranoid` are bind-mounted as **regular files** from the per-project state dir:

```
$XDG_DATA_HOME/sandbox/containers/<hash>/lockfiles/<name>   ←→   /app/<name>   (RW)
```

- A manifest-declared lockfile is bound when it already exists on the host source **or** has been seeded in the state dir on a previous run. Lockfiles absent from both are skipped (Docker cannot `mkdirat` a missing `/app/<name>` mountpoint inside `:ro`, so an unbound stub would abort the run).
- **Primary-lock fallback** (added Phase 5b smoke): when the selection above is empty, the manifest's `primary_lock_file` is bound anyway — and `seed_lockfiles` touches an empty stub on the host so Docker can mount over it. The real lockfile lives in the state-dir bind; the host file stays empty until `sandbox sync-lock` (Phase 6+) copies it back. Without this, a fresh `npm install` / `bun install` / `cargo build` on a project with no lockfile hits `EROFS` against the read-only source. The package manager has nowhere to write the lockfile it's trying to create.
- The host-touch is logged at INFO via `tracing` so the user sees the mutation. The stub file is zero bytes; functionally indistinguishable from `touch package-lock.json` before the run.
- On each `run`, before `docker run`/`start`, the seed under `lockfiles/<name>` is created if missing by copying the host project's current lockfile. The seed is never overwritten on subsequent runs — state-dir is the source of truth in safe/paranoid.
- The host project tree never sees the modifications (intentional: the threat model T2 requires source RO).
- Bringing the modified lockfile back to the host (so it can be committed) is deferred to Phase 3 as `sandbox sync-lock` (or equivalent). Until then, users who need the new lockfile in their working tree promote to `unsafe` (where the lockfile is part of the source bind RW) and re-run.
- `sandbox nuke` removes the entire `containers/<hash>/` subtree, including `lockfiles/`, so promoting a project from `safe` to `unsafe` cleanly is a `nuke` away.

In `unsafe`, no extra mount is added: the `/app` bind is RW and Docker writes pass through to the host file directly.

### Package directory mountpoints (safe / paranoid)

Named volumes are mounted at `/app/<package_dir>` paths *inside* the read-only `/app` bind. Docker creates each inner mountpoint with `mkdirat`, which fails when the parent (`/app`) is a read-only bind whose host source doesn't already contain the directory. To avoid this, `sandbox run` ensures every manifest-declared `package_dir` exists on the host source before invoking `docker run`, creating an empty directory if missing (logged at INFO via `tracing`). This is a benign mutation — any package manager would create the same directory on first install — and matches the principle that the *source files* stay read-only while their carrier paths may need to exist.

`--unsafe` skips this pre-creation: the source bind is already RW and Docker handles missing paths itself.

### Named volume ownership (chown-on-create)

Docker creates named volumes owned by `root:root` inside the container. The project container always runs as the host UID (per ADR-0009 / OQ-004) so `npm install`, `cargo build`, `pip install`, etc. hit `EACCES` on first write — they're trying to `mkdir` inside a volume whose top-level is root-owned.

Fix: `sandbox-docker::volume::ensure_owned(name, uid, gid)` creates the volume if missing and then runs a one-shot init container — `docker run --rm --network none --user 0:0 -v <vol>:/v alpine:3 chown -R uid:gid /v` — to remap ownership. Returns `true` when chown ran (volume was new), `false` on no-op subsequent runs.

Trade-offs:
- First `sandbox run` per project pulls `alpine:3` once (~6 MB) and runs one chown per declared `package_dir`. Each chown is sub-second on a fresh volume.
- The init container has `--network none` and no project mounts; the only thing it touches is the named volume.
- Trusting `alpine:3` adds it to the trust boundary, alongside the language base image and `sandbox/scanner:latest`. Acceptable for v0.1; if alpine ever needs replacing, we pin a digest in `volume::INIT_IMAGE`.

`--unsafe` skips the chown (and uses host bind mounts directly), so this only matters in safe/paranoid.

## Alternatives considered

- **(a) Always named volumes (no profile differentiation).** Rejected: the trusted/unsafe flow needs commitable lockfiles and IDE-visible `node_modules`. A consultant who validated a client repo must be able to `git status` and see the lockfile change.
- **(b) Always bind mount.** Rejected: this is the docker-sandbox v0 behaviour that the threat model exists to prevent (T1, T2). The 2026-05-06 incident would have written to the project tree under that model.
- **(c) Symlink in the host project pointing at the Docker volume mount.** Rejected: Docker volume contents live under `/var/lib/docker/volumes/<name>/_data`, which requires root to read on Linux. Editors, `git`, and tooling on the host would hit `EACCES` on every operation. The bind mount in `unsafe` delivers the same observable result without the permission friction.
- **(d) Whitelist specific files (lockfile only) for RW even in safe mode.** Considered: keeps lockfile commitable without losing source RO. Rejected for v0.1: malware could write through the whitelist (e.g. crafted `bun.lockb`-named payload). Revisit if a clean implementation appears.
- **(e) Named volume per lockfile.** Rejected: Docker mounts named volumes as directories; mapping one over a single file path requires either an init container or a `docker cp` seed step. The state-dir bind achieves the same isolation with simpler mechanics (regular file under our XDG data dir, visible to the user, removed by `sandbox nuke`).
- **(f) Frozen-lockfile-only (no lockfile mount in safe/paranoid).** Considered: simpler still — the source bind RO already covers the lockfile read-only. Rejected: `bun install` / `npm install` / `cargo build` (without `--frozen-lockfile` / `npm ci` / `--locked`) all attempt to rewrite the lockfile and would fail with `EROFS`. Friction outweighs the marginal simplification, especially since the state-dir bind keeps the threat model intact.

## Consequences

Positive:

- Default mode preserves the threat model (T2): source is RO, package dirs are isolated. Postinstall scripts run inside the container without touching host files.
- Unsafe mode behaves like a normal dev container: `bun i` writes a real `node_modules` and a real lockfile that `git diff` shows.
- The same `LangManifest` schema drives all profiles — `lock_files` is one new field, not a separate config tree.

Negative / open:

- **Lockfile commits in safe/paranoid require an explicit sync-out step.** The state-dir bind keeps the modified file under `~/.local/share/sandbox/containers/<hash>/lockfiles/<name>`; `git status` on the host shows nothing. A `sandbox sync-lock` (or equivalent) command in Phase 3 will copy the seed file back into the project tree. Until then, `cp` from the state dir or promoting to `--unsafe` are the documented escapes.
- **Switching profiles on the same project** (run safe, then run unsafe) leaves a stale seed under `lockfiles/`. Unsafe binds the host's (possibly different) lockfile and ignores the seed; Phase 3 sync-out should warn when seed and host disagree. `sandbox nuke` clears the state dir wholesale.
- **`unsafe` writes to the host source tree.** Documented and intentional: `unsafe` is the "I trust this project" switch, in line with ADR-0009 and the threat model.

## References

- `../threat-model.md` T1, T2, T5
- `../srs.md` § `run` (`--unsafe`, `--profile`)
- ADR-0009 (container reuse — package_dirs survival)
- ADR-0006 (language manifest schema — `lock_files` is added here)
- `../open-questions.md` OQ-002 (commits in `/app` RO), OQ-003 (trust persistence)
