# AGENTS.md — sandbox-core

## Responsibility

Foundational domain types and pure logic. **No I/O against external services** (no Docker, no network, no spawning processes). Filesystem reads/writes are allowed (config, state, manifests) but should be confined to `state.rs` and `config.rs` modules.

## Boundaries

- **Owns:**
  - `Project` (resolved path, hash, language)
  - `ProjectHash` (newtype around sha256)
  - `LanguageId`, `LangManifest` (loader + matcher for `languages/*.toml`)
  - `Profile` (security/network/resource bundle from config)
  - `Config` (user config from `~/.config/sandbox/config.toml`)
  - `State` (per-project state at `$XDG_DATA_HOME/sandbox/containers/<hash>/`)
  - `paths` (XDG resolution, default locations)
- **Does not own:** anything that calls Docker, runs scans, or starts proxies. Those are adapters.
- **Depends on:** stdlib, `serde`, `sha2`, `walkdir`, `directories`, `thiserror`, `tracing`. **Not on tokio** — this crate is sync.

## Layout (target shape — Phase 1+)

```
src/
├── lib.rs                  re-exports public API
├── error.rs                Error enum
├── paths.rs                XDG resolution
├── project.rs              Project struct + resolution
├── hash.rs                 ProjectHash + git-aware hashing
├── lang.rs                 LangManifest, LanguageId, detection
├── profile.rs              Profile + serialization
├── config.rs               Config loader + merge with defaults
└── state.rs                Per-project State store
```

Today (Phase 0): `lib.rs` only, with module placeholders.

## Conventions

- **Library error handling**: define `pub enum Error` with `thiserror`. Re-export `pub type Result<T>`. Never use `anyhow` here.
- **Newtypes for IDs**: `ProjectHash`, `ContainerName`, `LanguageId`. Don't pass raw `String` for things that have semantics.
- **No global state.** Pass `Config` and `Paths` explicitly.
- **All public items have `///` doc comments.** Format per `docs/sandbox/playbook.md` § 5.
- **Hashing is deterministic.** Same input → same output. Test with fixtures.
- **Filesystem operations** go through `paths` module — never hardcode `~/.config/sandbox`.

## Commands

```sh
cargo test -p sandbox-core
cargo doc -p sandbox-core --open
```

## Points of attention

- This crate's API is consumed by every other crate. Breaking changes ripple. When in doubt, add a new function rather than change an existing signature.
- `LangManifest` is a public schema (users edit TOML). Bumping its required fields is a breaking change for users — bump a version field in the manifest itself and migrate.
