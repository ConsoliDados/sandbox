# Playbook

Conventions every contributor (human or agent) must follow. Lints enforce most of it; the rest is reviewed.

## 1. Code structure

- **One module = one responsibility.** Split when a file grows past 300 LOC (soft) or 500 LOC (hard).
- **One function = one job.** Soft 50 LOC, hard 100 LOC. Extract helpers freely.
- **Public API of every crate goes in `lib.rs` (or `mod.rs`)**, re-exporting from internal modules. External callers should never reach into `crate::internal`.
- **Adapters depend on core, not vice versa.** `sandbox-core` defines traits; `sandbox-docker`, `sandbox-scan`, `sandbox-proxy` implement them.

## 2. Error handling

- **Library crates use `thiserror`.** Each crate exposes an `Error` enum:
  ```rust
  #[derive(Debug, thiserror::Error)]
  pub enum Error {
      #[error("docker daemon unreachable: {0}")]
      DaemonUnreachable(String),
      #[error("compose validation failed: {0}")]
      ComposeInvalid(#[from] ComposeError),
      // ...
  }
  pub type Result<T> = std::result::Result<T, Error>;
  ```
- **`sandbox-cli` uses `anyhow`** to compose errors from multiple crates with context (`anyhow::Context::context`).
- **No `unwrap()`. No `expect()`. No `panic!()`** outside `#[cfg(test)]`. The lints warn; treat warnings as build failures in CI.
- **No `Box<dyn Error>` in public APIs.** Use a typed enum.
- **Error messages are user-facing.** They tell the user what to do, not just what failed.

## 3. Strict typing

- **Newtype wrappers** for IDs and paths that have semantics:
  ```rust
  pub struct ProjectHash(pub [u8; 32]);
  pub struct ContainerName(String);
  pub struct ProjectPath(PathBuf);
  ```
- **No `&str`/`String` for things that are not arbitrary text.** Wrap.
- **`Option<T>` over sentinel values.** `Option<u16>` for "maybe a port", not `0` or `-1`.

## 4. `unsafe`

Forbidden at workspace level. If you find a case where you think you need it, write a comment and a test, raise an ADR, get review.

## 5. Comments and doc comments

- **Comments only when WHY is non-obvious.** Names carry the WHAT.
- **`///` doc comments required on every public item in `sandbox-core`.** Format:
  ```rust
  /// Computes the canonical hash of a project's source tree.
  ///
  /// # Inputs
  /// - `path`: must be an absolute, canonical directory path.
  ///
  /// # Behavior
  /// Hashes the output of `git ls-files` if the project is a git repo,
  /// otherwise walks the directory excluding `package_dirs`.
  ///
  /// # Errors
  /// Returns `Error::Io` if the path is unreadable.
  pub fn project_hash(path: &ProjectPath) -> Result<ProjectHash> { ... }
  ```
- **Adapter crates may use lighter doc comments** but must explain non-obvious external interactions.
- **Never leave TODO/FIXME without an associated `open-questions.md` entry or GH issue.**

## 6. Logging

- **`tracing` everywhere.** Never `println!` for diagnostics (only for user-facing CLI output).
- **Spans on every command.** `cli::run` opens a span `run`, every step inside is a child event.
- **Fields, not strings.** `info!(project = %hash, lang = ?lang, "starting");`, not `info!("starting {} {}", hash, lang);`.
- **Levels**: `error` (failure), `warn` (degraded), `info` (high-level milestones), `debug` (per-step), `trace` (verbose).

## 7. Testing

- **Unit tests next to the code.** `mod tests { ... }` at the bottom of each `.rs`.
- **Integration tests** for cross-crate behavior in `crates/sandbox-cli/tests/`.
- **No mocking of Docker** in tests — use a real Docker daemon in CI; gate with `#[cfg(feature = "docker-tests")]` for local skipping.
- **Property tests with `proptest`** for parsers (manifests, compose validators).

## 8. Commits

Conventional Commits. Scope = crate name without `sandbox-` prefix.

```
feat(scan): yara rule for vscode autorun
fix(docker): handle stopped container in run command
docs(adr): draft ADR-0003 (volume strategy)
refactor(core): extract LangManifest::detect into separate file
test(scan): add fixtures for compose validation
chore: bump tokio to 1.x
```

Git Flow: `feat/*` branches off `dev`, squash-merge back into `dev`. Release branches off `dev`, merge into `main` (which is tagged) and back into `dev`. `main` always builds.

## 9. ADRs

Required when:
- Changing a default that affects security posture.
- Adding a new dependency on an external service or daemon.
- Changing the CLI surface (subcommand added/removed/renamed).
- Choosing between two approaches that have lasting trade-offs.

Format: `adrs/NNNN-title.md`. Use the template in `adrs/README.md`.

## 10. Things to NOT do

1. **Don't shell out via string.** Use `Command::new("docker").args([...])`. Never `bash -c`.
2. **Don't introduce wide Docker permissions** without explicit user opt-in path.
3. **Don't write to the project source from the container in default mode.** Test for it.
4. **Don't hardcode language detection.** Read manifests.
5. **Don't bypass scan silently.** `--no-scan` requires `--unsafe`.
6. **Don't `.clone()` to placate the borrow checker.** Refactor.
7. **Don't write integration tests that assume specific host UID/GID.** Use `id -u`/`id -g` at runtime.
8. **Don't break `--print-cmd`.** Every Docker action must be representable as a printable command.
