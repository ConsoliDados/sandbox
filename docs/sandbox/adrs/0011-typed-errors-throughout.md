# ADR-0011 — Typed errors throughout (no anyhow)

- **Status:** Accepted
- **Date:** 2026-05-06
- **Phase:** 1

## Context

The first cut of `sandbox-cli` used `anyhow::Result` and `anyhow::bail!` at the CLI boundary, with library crates using `thiserror` for typed enums underneath. This is the dominant Rust ecosystem pattern: typed errors in libraries, `anyhow` for the binary that composes them.

Two pressures pushed us to revisit:

1. The `expect()` / `unwrap()` lint (`clippy::unwrap_used = "warn"`, `expect_used = "warn"`) is set strictly in this workspace. Tests still needed `.expect("...")` for setup (tempdir, file writes), so an `#![cfg_attr(test, allow(...))]` was added to allow them. This created two cultures: production code with `?` discipline, test code with `.expect()` shortcuts. The maintainer asked to unify.
2. The reference style at [`rust10x/rust-web-app`](https://github.com/rust10x/rust-web-app) — production-grade, no `anyhow` anywhere in the tree, every crate has its own typed `Error` enum with `#[from]` variants for child errors. This is more disciplined and gives `match err { ... }` access to specific variants for targeted remediation in the CLI.

The maintainer also flagged a UX concern with `anyhow`: the canonical examples in the official docs (`expect()` discouraged, `unwrap_or_else` preferred) reflect a culture where `anyhow` users reach for `expect()` "just for now" and never fix it. We want the type system to make that pattern impossible.

## Decision

**No `anyhow` in this workspace.** Every crate — including the binary — defines its own `Error` enum and a `Result<T>` typedef:

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
    // ... cli-only variants as needed
}

pub type Result<T> = std::result::Result<T, Error>;
```

`main()` returns `cli::Result<()>`; the runtime translates the typed error into an exit code and a user-facing message before exiting.

**Tests return `Result<(), Box<dyn std::error::Error>>` and use `?`.** No `.expect()` for setup. The lint `expect_used` applies uniformly, no `cfg_attr` carve-out for test code. `Box<dyn Error>` here is a deliberate boundary: tests compose tempfile, serde, our own errors, etc., and a single Box-typed return spares us hand-rolling `From` impls for every test-only error path.

**Negative tests** (asserting that an operation returned a specific `Err` variant) use `assert!(matches!(result, Err(Error::Variant { .. })))` instead of `.expect_err()`. Clippy doesn't warn on `assert!` and the matched variant stays in the test's expression.

**`anyhow` is removed from `[workspace.dependencies]`.** Reintroducing it requires a new ADR.

## Alternatives considered

- **(a) Keep `anyhow` at the CLI boundary.** Rejected: the CLI never uses `Context::context` for human-facing remediation strings (we have `Display` on the typed errors with rich messages already). What `anyhow` actually delivered in our code path was a place to put `.context("..."`) calls that we never wrote. The cost (no pattern-matching on errors, two-cultures discipline) was real.
- **(b) Use `derive_more::From` instead of `thiserror`** (the [rust10x](https://github.com/rust10x/rust-web-app) style). Rejected: we want the `#[error("user-facing text {0}")]` mechanism — our errors are read by humans on a CLI, not serialised as JSON to a web client. `thiserror` keeps the `Display` impl rich; `derive_more` would force `Display = Debug`, which is fine for JSON but worse for terminals.
- **(c) `Box<dyn Error>` everywhere.** Rejected: defeats pattern matching on specific variants. The CLI cannot offer "did you mean `--lang`?" suggestions if errors are box-typed.
- **(d) `Result<(), anyhow::Error>` in tests, `Result<(), cli::Error>` in production.** Rejected: introduces the same two-cultures problem we are trying to solve.

## Consequences

Positive:

- One discipline across the workspace: `?` for propagation, no `.unwrap()` / `.expect()` / `panic!()` outside `assert!` macros.
- `match err { Error::Core(sandbox_core::Error::LanguageNotDetected(p)) => ..., ... }` works in the CLI top-level, enabling targeted user remediation.
- The compile-time graph of "which errors propagate through which boundaries" is visible: every `#[from]` in `cli::Error` documents a real cross-crate flow.
- `Cargo.toml` shrinks slightly (`anyhow` removed).
- The `#![cfg_attr(test, allow(clippy::unwrap_used, expect_used, panic))]` opt-out comes off `sandbox-core/src/lib.rs`. Lints apply uniformly.

Negative:

- More boilerplate in `cli::Error` than `anyhow::Error`. Each new library dependency needs a `#[from]` variant. Trivial in practice; we have four library crates.
- Tests look slightly longer because they declare a return type. Net: more or less the same line count, since every `.expect("...")` is one token shorter than `?` plus the return-type signature, and the gain is clarity of error chain.

## Migration

Single refactor commit:

1. New file `crates/sandbox-cli/src/error.rs` with the enum above.
2. `crates/sandbox-cli/src/main.rs` returns `cli::Result<()>`; replace `anyhow::bail!(...)` with `Err(cli::Error::...(...).into())` or a typed variant.
3. Drop `anyhow.workspace = true` from `crates/sandbox-cli/Cargo.toml`.
4. Drop `anyhow = "1"` from workspace `Cargo.toml`.
5. Drop `#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]` from `crates/sandbox-core/src/lib.rs`.
6. For each test in `sandbox-core`: change signature to `-> Result<(), Box<dyn std::error::Error>>`, replace `.expect("...")` with `?`, and rewrite negative tests to use `assert!(matches!(...))`. End every test body with `Ok(())`.

Verify: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. All must pass clean.

## References

- `playbook.md` § 6 (Error handling — normative)
- [`rust10x/rust-web-app`](https://github.com/rust10x/rust-web-app) — reference style (no anyhow)
- [Rust API Guidelines: C-GOOD-ERR](https://rust-lang.github.io/api-guidelines/interoperability.html#c-good-err)
