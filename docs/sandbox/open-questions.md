# Open Questions

Running list of things we haven't decided. Keep entries dated. Move to ADRs when resolved.

## Active

### OQ-001 — Should `sandbox run` rebuild the image on `Dockerfile` change? (2026-05-06)

Options:
- (a) Always rebuild (slow on cold cache, predictable).
- (b) Rebuild only when manifest changes (fast, but stale if base image rotates).
- (c) `--rebuild` flag, never auto.

Leaning (c) for predictability. Decide in Phase 1 implementation.

### OQ-002 — How to handle `git` commits from inside the read-only container? (2026-05-06)

In default mode, `/app` is read-only. `git commit` writes to `.git/`, which is inside `/app`. Options:

- (a) Carve `/app/.git/` out as a named volume too. Pros: commits work. Cons: more complexity, and `.git/` writes from a malicious container could plant hooks.
- (b) Forbid commits from inside the container. User commits from the host. Pros: simplest. Cons: friction.
- (c) `--rw-git` flag opt-in.

Leaning (c). Decide in Phase 2.

**Update 2026-05-07:** ADR-0003 settled volume strategy per profile. Lockfiles (`bun.lockb`, `package-lock.json`, `Cargo.lock`, …) live in named volumes in safe/paranoid and bind-mount into the host source tree in unsafe. Practical consequence: in safe/paranoid the host doesn't see lockfile changes, so committing a lockfile from those profiles requires resolving this question. In unsafe the bind mount already covers the case (commits go through normal host git). The decision here primarily affects safe/paranoid lockfile commits and any `git` activity inside `/app`.

### OQ-003 — Where does the user mark a project as "trusted" persistently? (2026-05-06)

`--unsafe` is per-invocation. For frequently-used trusted projects, retyping is annoying. Options:

- (a) `sandbox trust add PROJECT` registers the hash + path; subsequent runs default to unsafe.
- (b) `.sandbox.toml` in the project root with `trusted = true`. Risky — malware could plant this.
- (c) `~/.config/sandbox/trusted.toml` with hash → trust level. Editable manually.

Leaning (c). Decide in Phase 2.

### OQ-006 — Future shell support (2026-05-06)

User asked for an addendum: when going OSS, support bash and fish in addition to zsh. Plan:

- Manifest `extra_packages` already extensible.
- The `--shell` flag exists in SRS.
- Need to factor out "starship + dotfile mount" into a per-shell strategy.

Defer to post-v0.1.

### OQ-007 — How to express "I want this scan rule to ignore this finding here" (2026-05-06)

Suppression file in the project (`.sandbox-scan-ignore.toml`) with rule_id + path? Risky — malware would plant it.

Better: suppression in user-global config, keyed by `(rule_id, project_hash)`. Survives source changes within reason.

Decide in Phase 4.

## Resolved

### OQ-004 — Default UID for container user (Resolved 2026-05-06, Phase 1)

**Decision:** numeric `--user $(id -u):$(id -g)` derived from the host user at run time. The hardening Plan adds `--user` as a numeric pair, not a username, to avoid mismatches with images that don't have the corresponding user account.

**Rationale:** matching the host UID/GID keeps file ownership clean on bind-mounted directories. Named volumes (`node_modules`, `target`) are owned by the same UID, which is harmless since the host never reads them directly. Numeric form sidesteps the "image has user `node` (uid 1000) but my host is uid 1500" problem.

**Edge case:** if the host UID is 0 (running sandbox as root), we still pass `--user 0:0` — the operator clearly knows what they want. We log a warning.

### OQ-005 — Auto-detect lang failures: which manifest takes precedence? (Resolved 2026-05-06, Phase 1)

**Decision:** language manifests have a `priority: u32` field (default 0). On multi-match, the highest priority wins. Ties are resolved by "more `detect` files matched"; remaining ties produce an error and require `--lang` to disambiguate.

**Rationale:** explicit, predictable, and user-overridable. A user installing a custom manifest with `priority = 100` always wins over bundled defaults.

**Defaults:** `node = 0`, `bun = 10`, `rust = 20`. Rust wins over Node (rare conflict via Tauri). Bun wins over Node (when both `bun.lockb` and `package.json` are present, Bun is the more specific tool).

See: ADR-0006, `languages/README.md`.
