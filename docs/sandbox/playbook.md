# Playbook

Conventions every contributor — human or AI agent — must follow. Lints enforce most of it; the rest is reviewed.

This document is normative. If you disagree with a rule, raise an ADR before deviating in code.

## 1. Why this playbook exists

The codebase is small but the surface (Docker, security scan, reverse proxy, manifests, lifecycle) is wide. Two pressures shape the rules:

- **Code is written and read by AI agents alongside the human author.** The agent's context window is the binding constraint. Distinctive names and small files are not stylistic — they are operational.
- **Security boundary.** This tool is what stands between the user's host and untrusted code. Sloppy `unwrap()`, an unhandled `Result`, or a `Plan` built without `--cap-drop=ALL` is a vulnerability, not just a bug. Lints and the type system are part of the threat model.

## 2. Why not canonical Clean Architecture

We keep the **spirit** of Clean Architecture (dependency inversion where it matters, domain isolation, testable use cases) and drop the **ceremony** (one interface per operation, DTO-per-layer, mapper classes, `*UseCase` structs).

Specifically:

- Use cases are **free functions**, not classes.
- Each adapter crate exposes its public API via `lib.rs` re-exports. There is no `domain` / `application` / `infrastructure` triplet inside every crate — the **crate boundary** *is* the layer boundary.
- Repositories appear when there's an external store to abstract. We do not invent `FooRepository` for in-memory state; we pass `&mut Foo` directly.
- Dependencies are passed as `&impl Trait` parameters to the function that needs them. They are never stored on a struct just to obey "constructor injection".

When you find yourself writing `pub struct DoXUseCase { repo: Box<dyn Repo> }`, stop. Write `pub fn do_x(input, repo: &impl Repo) -> Result<...>` instead.

## 3. Conventions (summary)

The full text follows in §4 onward. Quick reference for skim:

| # | Convention |
|---|---|
| 1 | Files ≤ 500 lines, ideally 200–300 |
| 2 | Functions 4–60 lines, single responsibility |
| 3 | Library crates use `thiserror`; the binary has its own typed `Error` (no `anyhow`) |
| 4 | Use cases and orchestration are **free functions** with explicit `&impl Trait` deps |
| 5 | Distinctive, greppable names (target: <5 hits project-wide for unique identifiers) |
| 6 | Rich `///` doc-comments on public items in `sandbox-core`; provenance-style |
| 7 | Newtype wrappers for IDs and paths with semantics |
| 8 | Errors include the offending value and the expected shape |
| 9 | No `unwrap()` / `expect()` / `panic!()` in non-test code; `?` everywhere |
| 10 | Tests return `Result<(), Box<dyn std::error::Error>>` and use `?` |
| 11 | Early returns; max 2 indentation levels in any function |
| 12 | `unsafe` is forbidden at workspace level |
| 13 | DRY only after the third occurrence; two is a coincidence |
| 14 | Conventional Commits with crate-name scope; Git Flow with `feat/* → dev → main` |
| 15 | `AGENTS.md` at the root of every crate |

## 4. Size table

| Unit | Soft limit | Hard limit | What to do at the limit |
|---|---|---|---|
| File (LOC including tests) | 300 | 500 | Split by sub-responsibility. A 400-line `lang.rs` is a candidate for `lang/manifest.rs` + `lang/registry.rs`. |
| Function | 50 | 100 | Extract helper functions. A function that walks five collections and writes three files is doing five jobs. |
| Match arm body | 5 | 15 | Extract a function. Keep `match` legible. |
| Indentation depth | 2 | 3 | Use early returns; flatten nested conditions. |
| Public items per crate | — | — | If `lib.rs` re-exports more than ~30 items, the crate is doing too much. |

These are guidelines, not laws — but the team-wide ratio of "files at hard limit" should be near zero. If it isn't, the codebase is drifting.

## 5. Module organization

The repo is a Rust workspace with five crates:

- `sandbox-cli` — bin: argparse, command dispatch, top-level orchestration. Depends on every other crate.
- `sandbox-core` — domain: project, profile, hash, language manifest, lifecycle state. Pure data + logic; no I/O against external services.
- `sandbox-docker` — adapter: shells out to `docker` and `docker compose`. Owns the `Plan` data structure for `docker run` invocations.
- `sandbox-scan` — adapter: YARA + heuristics + scan cache + compose validation.
- `sandbox-proxy` — adapter: Traefik label generation + sidecar lifecycle.

**Inside a crate**, files are small modules with focused responsibility — analogous to "bounded contexts" but lighter (we don't have aggregate roots, domain events, or transactional outboxes). Example from `sandbox-core/src/`:

```
src/
├── lib.rs          public re-exports only
├── error.rs        Error enum + Result type
├── paths.rs        XDG paths
├── hash.rs         ProjectHash + project_hash function
├── lang.rs         LangManifest + LanguageRegistry + detection
├── profile.rs      Profile struct + built-in factories
├── config.rs       Config loader (config.toml + profiles)
├── project.rs      Project + ContainerName + NamedVolume + resolve
└── state.rs        Meta on-disk state per project
```

Rules:

- **`lib.rs` is the public API.** Re-export only. External callers must not reach into private modules.
- **No `mod.rs` games for re-exports** — write the re-exports inline in `lib.rs`.
- **Adapters depend on `sandbox-core`**, not on each other. If `sandbox-docker` needs something from `sandbox-scan`, lift it into core.
- **`sandbox-core` does NOT depend on tokio.** Async lives in adapters and the CLI. Core is sync.

## 6. Error handling

The most important section in this playbook. Read it twice.

### 6.1 Library crates use `thiserror`

Every crate exposes a single `Error` enum and a `Result<T>` typedef:

```rust
// crates/sandbox-core/src/error.rs

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("project path does not exist or is not a directory: {0}")]
    ProjectPathInvalid(PathBuf),
    #[error("invalid manifest at {path}: {reason}")]
    InvalidManifest { path: PathBuf, reason: String },
    // ...
}

pub type Result<T> = std::result::Result<T, Error>;
```

### 6.2 `sandbox-cli` has its own typed `Error` — **no `anyhow`**

The binary composes errors from the four library crates by listing each as a `#[from]` variant. No `anyhow`. No `Box<dyn Error>` in non-test code.

```rust
// crates/sandbox-cli/src/error.rs

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Core(#[from] sandbox_core::Error),
    #[error(transparent)]
    Docker(#[from] sandbox_docker::Error),
    #[error(transparent)]
    Scan(#[from] sandbox_scan::Error),
    #[error(transparent)]
    Proxy(#[from] sandbox_proxy::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    // ... cli-specific variants
}

pub type Result<T> = std::result::Result<T, Error>;
```

Why: the CLI's surface is human-facing. `anyhow::Context::context("...")` chains erase type information, which makes "if it's a `LanguageNotDetected`, also suggest `--lang`" awkward. A typed enum keeps every error inspectable. The cost is one variant per library crate — trivial.

### 6.3 Errors carry the offending value and the expected shape

A good error message tells the user **what's wrong**, **what was expected**, and **what to do**:

```rust
// Bad
#[error("invalid score")]
InvalidScore,

// Good
#[error("invalid ats score: {0} (expected 0..=100)")]
InvalidAtsScore(u8),

// Good
#[error("ambiguous language match for {path} (candidates: {candidates:?}); use --lang")]
AmbiguousLanguageMatch { path: PathBuf, candidates: Vec<String> },
```

The receiver of `Display::fmt(&err)` should know how to fix the problem without re-reading the code that produced it.

### 6.4 No `unwrap()`, `expect()`, `panic!()` outside tests

The workspace lints (`clippy::unwrap_used = "warn"`, `expect_used = "warn"`, `panic = "warn"`) treat these as warnings; CI runs `clippy -D warnings`, so they are effectively errors in non-test code.

If you find yourself wanting `.unwrap()`, the answer is `?`. If you can't `?`, the function should return `Result`. If it can't return `Result` (e.g. it's `Display::fmt`), redesign so that it can.

### 6.5 Tests return `Result<(), Box<dyn std::error::Error>>` and use `?`

Every test that does I/O (tempdir, file write, parse) uses `?` for setup errors:

```rust
#[test]
fn save_then_load_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir()?;
    let dir = tmp.path().join("containers").join("abc");
    fixture().save(&dir)?;
    let loaded = Meta::load(&dir)?;
    assert_eq!(loaded, fixture());
    Ok(())
}
```

When this test fails because of an infrastructure error (e.g. `tempdir()` can't write), Cargo's test runner prints the full error chain via `Display`. We **do not** use `.expect("tempdir")` even in tests — it short-circuits the chain and adds noise.

`assert!`, `assert_eq!`, `assert_ne!`, and `assert_matches!` are the only acceptable panicking forms in tests. They state the test's actual claim.

### 6.6 No `Box<dyn Error>` in production code

Box-typed errors hide the typed enum behind a vtable. They prevent pattern matching, which prevents the CLI from offering targeted remediation. Library crates and the CLI return their crate's `Error`. Only test return types use `Box<dyn Error>`, and only because tests need to compose tempfile + serde + our own errors transparently.

## 7. Strict typing

- **Newtype wrappers** for IDs, paths, and short strings that carry semantics:
  ```rust
  pub struct ProjectHash([u8; 32]);
  pub struct ContainerName(String);
  pub struct NamedVolume(String);
  pub struct LanguageId(String);
  ```
- **No `&str` / `String`** for things that are not arbitrary text. If a function argument is "a project path", its type is `&Path`, not `&str`.
- **`Option<T>` over sentinel values.** `Option<u16>` for "maybe a port", not `0`.
- **Enums for closed sets.** `Severity { Info, Warn, High, Critical }`, not stringly-typed `severity: String`.
- **`#[serde(deny_unknown_fields)]`** on every public deserialised struct (manifests, config, state). Typos must fail at parse time, not be silently dropped.

## 8. `unsafe`

Forbidden at workspace level (`unsafe_code = "forbid"`). If you find a case where you think you need it, write a test, raise an ADR, get review. The bar is high.

## 9. Comments and doc comments

- **Comments only when WHY is non-obvious.** Names carry the WHAT. A comment that paraphrases the code below it is noise.
- **`///` doc comments are required on every public item in `sandbox-core`.** Optional in adapters, but encouraged for non-trivial APIs.
- **Provenance format** for non-trivial public items:
  ```rust
  /// Computes the project hash from a directory path.
  ///
  /// The path must exist and be a directory; otherwise `Error::ProjectPathInvalid`.
  /// Symbolic links are resolved before hashing so two paths that point at the
  /// same target produce the same hash. See ADR-0009.
  ///
  /// # Errors
  /// - `Error::Io` — path cannot be canonicalised.
  /// - `Error::ProjectPathInvalid` — path is not a directory.
  pub fn project_hash(path: &Path) -> Result<ProjectHash> { ... }
  ```
- **Never strip provenance.** A comment that links to an ADR or names an invariant is part of the code's contract. Refactors must carry the comment forward.
- **Never leave `TODO` / `FIXME` without an associated `open-questions.md` entry or GitHub issue.**

## 10. Naming

- **Distinctive and greppable.** Unique identifiers should produce <5 hits project-wide for `rg`.
- **`Project`, `Profile`, `Config` are too generic.** If the type is in `sandbox-core`, the crate prefix narrows enough; in tests, prefer fully-qualified `sandbox_core::Profile`.
- **Types are nouns.** Functions are verbs. Booleans read as predicates: `is_running`, `should_block`, `has_compose`.

## 11. Logging

- **`tracing` everywhere.** Never `println!` for diagnostics; reserve stdout for user-facing CLI output.
- **Spans on every command and every adapter call.** `cli::run` opens a span `run`; everything inside is a child event.
- **Fields, not strings.** `info!(project = %hash, lang = ?lang, "starting");` not `info!("starting {} {}", hash, lang);`.
- **Levels:**
  - `error` — failure that aborts the operation
  - `warn` — degraded but proceeding
  - `info` — high-level milestone visible to a normal user with `--verbose`
  - `debug` — per-step detail for the maintainer
  - `trace` — verbose, off by default

## 12. Testing

- **Unit tests next to the code** in `mod tests { ... }` at the bottom of each `.rs`.
- **Integration tests** for cross-crate behaviour live in `crates/sandbox-cli/tests/`.
- **Tests return `Result<(), Box<dyn Error>>`** and use `?` (see §6.5).
- **No mocking of Docker.** Tests that need the daemon are gated behind `#[cfg(feature = "docker-tests")]`. CI provides the daemon.
- **Property tests** with `proptest` are appropriate for parsers (manifests, compose validators).
- **One command per crate.** `cargo test -p <crate>` should always run that crate's tests headless without setup.

## 13. Commits and branches

Conventional Commits. Scope is the affected crate name without the `sandbox-` prefix:

```
feat(scan): yara rule for vscode autorun
fix(docker): handle stopped container in run command
docs(adr): accept ADR-0011 typed errors throughout
refactor(core): extract LangManifest::detect into a separate file
test(scan): add fixtures for compose validation
chore: bump tokio to 1.x
```

Multi-line commit messages preferred when the change has any subtlety. The body explains *why*; the diff explains *what*.

**Git Flow.** `main` is release-tagged and always builds. `dev` is the integration branch. Feature work happens on `feat/<name>` branches off `dev` and squash-merges back into `dev`. Release branches off `dev` merge into `main` (tagged) and back into `dev`.

## 14. ADRs

Required when:

- Changing a default that affects security posture.
- Adding a new external dependency (a crate that pulls non-trivial transitive deps, or a new system-level requirement).
- Changing the CLI surface (subcommand added, removed, renamed; flag semantics changed).
- Choosing between two approaches that have lasting trade-offs the next contributor would need to understand.

Format: `adrs/NNNN-title.md`. Use the template in `adrs/README.md`. Status starts as `Draft`, moves to `Accepted` when the corresponding code lands.

## 15. AGENTS.md

Every crate has its own `AGENTS.md` describing responsibility, boundaries, conventions specific to that crate, and integration notes. The root `AGENTS.md` is the entry point — it points at the priority-reading chain (threat-model, srs, sad, playbook, roadmap) and at each crate's `AGENTS.md`.

When working inside `crates/sandbox-X/`, read that crate's `AGENTS.md` first. It refines this playbook for the crate's domain and may add rules specific to it.

## 16. Things to NOT do

1. **Don't shell out via string.** `Command::new("docker").args([...])`. Never `bash -c "..."`. ADR-0002 explains.
2. **Don't introduce wide Docker permissions** (`--privileged`, `--cap-add=ALL`, `--pid=host`, host bind mounts outside the project) on any code path reachable without `--unsafe`.
3. **Don't write to the project source tree from inside the container in default mode.** That's the entire point of read-only mount.
4. **Don't hardcode language detection.** Read manifests. Adding a stack must require zero code changes.
5. **Don't bypass scan silently.** `--no-scan` requires `--unsafe` (or an explicit profile that disables it).
6. **Don't `.clone()` to placate the borrow checker.** Refactor.
7. **Don't write integration tests that assume specific host UID/GID.** Use `id -u` / `id -g` at runtime.
8. **Don't break `--print-cmd`.** Every Docker action must be representable as a literal command line.
9. **Don't use `anyhow`.** Anywhere. See §6 and ADR-0011.
10. **Don't catch `panic`s with `std::panic::catch_unwind` to recover.** A panic in our code is a bug — it should abort the process and be visible.
