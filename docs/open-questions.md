# Open Questions

Running list of things we haven't decided. Keep entries dated. Move to ADRs when resolved.

## Active

### OQ-001 — Should `sandbox run` rebuild the image on `Dockerfile` change? (2026-05-06)

Options:
- (a) Always rebuild (slow on cold cache, predictable).
- (b) Rebuild only when manifest changes (fast, but stale if base image rotates).
- (c) `--rebuild` flag, never auto.

Leaning (c) for predictability. Decide in Phase 1.

### OQ-002 — How to handle `git` commits from inside the read-only container? (2026-05-06)

In default mode, `/app` is read-only. `git commit` writes to `.git/`, which is inside `/app`. Options:

- (a) Carve `/app/.git/` out as a named volume too. Pros: commits work. Cons: more complexity, and `.git/` writes from a malicious container could plant hooks.
- (b) Forbid commits from inside the container. User commits from the host. Pros: simplest. Cons: friction.
- (c) `--rw-git` flag opt-in.

Leaning (c). Decide in Phase 2.

### OQ-003 — Where does the user mark a project as "trusted" persistently? (2026-05-06)

`--unsafe` is per-invocation. For frequently-used trusted projects, retyping is annoying. Options:

- (a) `sandbox trust add PROJECT` registers the hash + path; subsequent runs default to unsafe.
- (b) `.sandbox.toml` in the project root with `trusted = true`. Risky — malware could plant this.
- (c) `~/.config/sandbox/trusted.toml` with hash → trust level. Editable manually.

Leaning (c). Decide in Phase 2.

### OQ-004 — Default UID for container user (2026-05-06)

Options:
- (a) Match host UID/GID via `--user $(id -u):$(id -g)`. Pros: file ownership clean. Cons: container's `node_modules` etc. own files as host user.
- (b) Fixed nonroot UID (e.g. 1500). Pros: predictable. Cons: ownership mismatch with host.

Leaning (a). Decide in Phase 1.

### OQ-005 — Auto-detect lang failures: which manifest takes precedence? (2026-05-06)

If a project has both `package.json` and `Cargo.toml` (rare but possible — e.g. Tauri), which lang wins?

- (a) Most-specific wins (rust because Cargo.toml is rarer than package.json — heuristic).
- (b) Manifests have a `priority` field; highest wins.
- (c) Error and force user to specify `--lang`.

Leaning (b). Decide in Phase 1.

### OQ-006 — Future shell support (2026-05-06)

User asked for an adendum: when going OSS, support bash and fish in addition to zsh. Plan:

- Manifest `extra_packages` already extensible.
- The `--shell` flag exists in SRS.
- Need to factor out "starship + dotfile mount" into a per-shell strategy.

Defer to post-v0.1.

### OQ-007 — How to express "I want this scan rule to ignore this finding here" (2026-05-06)

Suppression file in the project (`.sandbox-scan-ignore.toml`) with rule_id + path? Risky — malware would plant it.

Better: suppression in user-global config, keyed by `(rule_id, project_hash)`. Survives source changes within reason.

Decide in Phase 4.

## Resolved

(None yet.)
