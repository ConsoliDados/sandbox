# AGENTS.md — sandbox-cli

## Responsibility

Top-level binary. Argument parsing (clap), subcommand dispatch, top-level error reporting, logging setup.

This crate owns the **user-facing surface** of the tool. The SRS at `docs/srs.md` is its specification.

## Boundaries

- **Owns:** argparse, command dispatch table, exit code translation, `--print-cmd` formatting, logging init, error display.
- **Does not own:** business logic. Each `commands/<name>.rs` is a thin orchestrator that calls into `sandbox-core`, `sandbox-docker`, `sandbox-scan`, `sandbox-proxy`.
- **Depends on:** all other crates in the workspace.

## Layout (target shape — Phase 1+)

```
src/
├── main.rs                  entry, log init, top-level dispatch
├── args.rs                  clap definitions for all subcommands
├── exit.rs                  ExitCode mapping per SRS
└── commands/
    ├── mod.rs               re-exports
    ├── run.rs               sandbox run
    ├── down.rs              sandbox down
    ├── nuke.rs              sandbox nuke
    ├── ps.rs                sandbox ps
    ├── logs.rs              sandbox logs
    ├── exec.rs              sandbox exec
    ├── net.rs               sandbox net on|off|status
    ├── scan.rs              sandbox scan
    ├── lang.rs              sandbox lang
    ├── proxy.rs             sandbox proxy
    └── config.rs            sandbox config
```

Today (Phase 0): just `main.rs` with a clap stub that prints help. No command bodies.

## Conventions

- **Use `anyhow::Result` and `anyhow::Context::context`** at this boundary. Library crates return typed errors; the CLI wraps them with user-facing context.
- **Translate every error to a SRS exit code.** See `exit.rs`. `anyhow::Error` → exit code is decided by inspecting the source via `downcast_ref`.
- **Print user-facing errors to stderr in red.** Do not use `eprintln!("error: {:?}", e)` (debug formatter is ugly); use a printer that walks `e.chain()` and prints each level.
- **Logging is `tracing`.** `RUST_LOG` env var honored. `--verbose` adjusts default filter.
- **Every subcommand opens a span.** `let _span = tracing::info_span!("run", project = %hash).entered();`
- **No business logic here.** If you find yourself writing Docker invocations or YARA rules in a `commands/*.rs` file, push it down into the appropriate crate.

## Commands

```sh
cargo run -p sandbox-cli -- --help
cargo run -p sandbox-cli -- run .
```

## Points of attention

- The CLI surface is a public contract. Renaming a subcommand or flag requires an ADR and a deprecation period.
- Default values should match `docs/srs.md`. If they diverge, fix the code or fix the doc — don't let drift accumulate.
