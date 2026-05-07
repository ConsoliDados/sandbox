# AGENTS.md — sandbox

Canonical entry point for any agent or contributor working in this repo.

`CLAUDE.md` is a symlink to this file. Both names load the same content.

## What this repo is

A Rust CLI (`sandbox`) that wraps Docker to provide **secure-by-default** isolated dev environments for **untrusted code**. Born from a real DPRK Lazarus malware incident (see [`docs/threat-model.md`](docs/threat-model.md)).

Design tenets:
- **Paranoid defaults.** Volumes read-only, no internet, no caps, ephemeral `$HOME`. Trust is opt-in.
- **Transparent.** Every Docker action has a `--print-cmd` / `--dry-run` echo. No magic.
- **Composable.** Languages are TOML manifests, not hardcoded.
- **Auditable.** Per-project state in `~/.local/share/sandbox/containers/<hash>/`.

## Repo shape

```
sandbox/
├── crates/
│   ├── sandbox-cli       bin: argparse, command dispatch, top-level orchestration
│   ├── sandbox-core      domain: project, profile, hash, language manifest, lifecycle state
│   ├── sandbox-docker    adapter: docker CLI shell-out, compose lifecycle, network ops
│   ├── sandbox-scan      adapter: YARA + heuristic regex + scan cache
│   └── sandbox-proxy     adapter: Traefik label generation + sidecar lifecycle
├── docs/                 architecture (sad), requirements (srs), playbook, ADRs, threat model, roadmap
├── languages/            language manifests (TOML)
└── scripts/dev/          lint.sh, test.sh, fmt.sh
```

Each `crates/*/` has its own `AGENTS.md` with responsibility + conventions specific to that crate.

## Priority reading

In order:

1. [`docs/threat-model.md`](docs/threat-model.md) — what's in/out of scope, defines security posture
2. [`docs/srs.md`](docs/srs.md) — CLI surface (subcommands, flags, exit codes)
3. [`docs/sad.md`](docs/sad.md) — crate boundaries, dataflow, key abstractions
4. [`docs/playbook.md`](docs/playbook.md) — coding conventions
5. [`docs/roadmap.md`](docs/roadmap.md) — current phase + what's next
6. The crate-level `AGENTS.md` of whatever you're working on
7. [`docs/adrs/`](docs/adrs/) when touching cross-cutting decisions
8. [`docs/open-questions.md`](docs/open-questions.md) — unresolved stuff

## Conventions (high level)

Full text in [`docs/playbook.md`](docs/playbook.md). Highlights:

- **Errors:** `thiserror` for library crates (typed enums), `anyhow` only at the CLI boundary. No `unwrap()`, no `expect()`, no `panic!()` outside tests. Lints enforce.
- **`unsafe` is forbidden** at workspace level.
- **File size:** soft 300 LOC, hard 500. If a file grows past, split.
- **Function size:** soft 50 LOC, hard 100.
- **Comments:** WHY only. Names carry the WHAT.
- **Doc comments (`///`):** required on every public item in `sandbox-core`. Optional in adapters but encouraged for non-trivial APIs.
- **Commits:** Conventional Commits. `feat(scan): ...`, `fix(docker): ...`, `docs(adr): ...`. Use the affected crate name as scope.
- **Branches:** Git Flow lite. `main` always builds. Feature branches off `main`, squash merge.

## Things to NOT do

1. **Don't shell out to `bash -c "<string>"` or build commands by string concat.** Build `Command::new("docker").args([...])`. ADR-0002 explains.
2. **Don't `unwrap()` / `expect()`** in non-test code. Lint warns; CI will fail.
3. **Don't hardcode language detection.** Read from `languages/*.toml`. Adding a stack must not require code changes (only manifest).
4. **Don't bypass the scan in default mode.** `--unsafe` is the explicit override. Scan-skipping silently is a footgun.
5. **Don't introduce wide-permission Docker flags** (`--privileged`, `--cap-add=ALL`, `--pid=host`, etc.) anywhere in code paths reachable without `--unsafe`.
6. **Don't write to the project source tree from within the container in default mode.** That's the whole point of read-only mount.

## Per-crate AGENTS.md

Each crate owns its own conventions for its domain. When working inside `crates/sandbox-X/`, read `crates/sandbox-X/AGENTS.md` first — it overrides/extends this root file for that scope.

## How to extend

| Task | What to touch |
|---|---|
| Add a language stack | Drop `languages/<name>.toml`. No code change. |
| Add a scan rule | `crates/sandbox-scan/rules/` (YARA) or `crates/sandbox-scan/src/heuristics/` (regex). Add tests. |
| Add a subcommand | `crates/sandbox-cli/src/commands/<name>.rs` + register in `commands/mod.rs` + update `docs/srs.md`. |
| Change Docker behavior | `crates/sandbox-docker/`. Document deviation from previous via ADR if user-visible. |
| Change network/security defaults | Requires ADR. Update `docs/threat-model.md`. |


