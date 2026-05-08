# AGENTS.md — sandbox-cli

## Responsibility

Top-level binary. Argument parsing (clap), subcommand dispatch, top-level error reporting, logging setup.

This crate owns the **user-facing surface** of the tool. The SRS at `docs/sandbox/srs.md` is its specification.

## Boundaries

- **Owns:** argparse, command dispatch table, exit code translation, `--print-cmd` formatting, logging init, error display.
- **Does not own:** business logic. Each `commands/<name>.rs` is a thin orchestrator that calls into `sandbox-core`, `sandbox-docker`, `sandbox-scan`, `sandbox-proxy`.
- **Depends on:** all other crates in the workspace.

## Layout

```
src/
├── main.rs                  entry: clap defs, dispatch, tokio runtime
├── error.rs                 cli::Error (composes lib errors via #[from])
└── commands/
    ├── mod.rs               module re-exports
    ├── dotfiles.rs          host dotfile discovery (zshrc, starship)
    ├── run.rs               sandbox run     ← Phase 1
    ├── down.rs              sandbox down    ← Phase 1
    ├── nuke.rs              sandbox nuke    ← Phase 1
    ├── ps.rs                sandbox ps        (Phase 3)
    ├── logs.rs              sandbox logs      (Phase 3)
    ├── exec.rs              sandbox exec      (Phase 3)
    ├── net.rs               sandbox net       (Phase 6)
    ├── scan.rs              sandbox scan      (Phase 4)
    ├── lang.rs              sandbox lang      (Phase 3)
    ├── proxy.rs             sandbox proxy     (Phase 5)
    └── config.rs            sandbox config    (Phase 3)
```

`tests/lifecycle.rs` covers `--print-cmd` end-to-end (no Docker required) and
exposes a `docker-tests` feature for tests that need a live daemon.

## Conventions

- **Typed errors only — no `anyhow`.** `error.rs` defines `cli::Error` with
  `#[from]` variants for each library crate's `Error`, plus `clap::Error`,
  `std::io::Error`. See ADR-0011 and `docs/sandbox/playbook.md` § 6.
- **Pattern for command modules:** each `commands/<name>.rs` exposes
  `pub(crate) struct Args { … }` and `pub(crate) async fn execute(args: Args) -> Result<()>`.
  The dispatcher in `main.rs` decodes clap into `Args` and calls `execute`.
- **All items in this crate are `pub(crate)`** (binary, not a library). The
  workspace `unreachable_pub` lint enforces this.
- **Print user-facing errors to stderr.** `main()` runs `run()`, prints
  `eprintln!("error: {err}")` on `Err`, and exits with code 1. Per-error exit
  codes per SRS will land when we add `exit.rs` (Phase 3).
- **Logging is `tracing`.** `RUST_LOG` is honored; `-v` adjusts default filter.
- **Every subcommand opens a span.** `let _enter = tracing::info_span!("run", path = %path).entered();`
- **No business logic here.** If you find yourself writing Docker invocations
  or YARA rules in a `commands/*.rs` file, push it down into the appropriate
  crate.

## Commands

```sh
cargo run -p sandbox-cli -- --help
cargo run -p sandbox-cli -- run .
```

## Points of attention

- The CLI surface is a public contract. Renaming a subcommand or flag requires an ADR and a deprecation period.
- Default values should match `docs/sandbox/srs.md`. If they diverge, fix the code or fix the doc — don't let drift accumulate.
