# Roadmap

Source of truth for "what's done, what's next, where are we." A fresh session should read this first to resume work.

## Current status

**Phase 0 — workspace skeleton.** Started 2026-05-06.

The repo compiles (`cargo check`) but the CLI is a stub. No subcommand actually does anything yet.

## Phases

### Phase 0 — Skeleton ✅ in progress

- [x] Workspace `Cargo.toml` with 5 crates and shared lints
- [x] Crate skeletons (`sandbox-cli`, `sandbox-core`, `sandbox-docker`, `sandbox-scan`, `sandbox-proxy`)
- [x] Per-crate `AGENTS.md` (placeholders with responsibility + boundaries)
- [x] Root docs scaffold (`AGENTS.md`, `README.md`, `playbook.md`, `sad.md`, `srs.md`, `threat-model.md`, `roadmap.md`, `open-questions.md`)
- [x] ADR index with 10 drafts (titles + status only — content deferred to phases that need them)
- [x] `languages/` manifests for node, bun, rust (ported from `~/Dev/docker-sandbox/`)
- [x] `scripts/dev/` (lint, test, fmt)
- [x] `cargo check` passes
- [x] First commit on `main` (`bed3741`)
- [x] Git Flow: `main` / `dev` branches; work happens on `feat/*`

### Phase 1 — Run/Down/Nuke + manifests + dotfiles

Goal: the CLI subset that replicates current `docker-sandbox` functionality, but secure-by-default.

Branch: `feat/lifecycle-mvp`.

- [x] ADR-0001 (Rust binary) accepted
- [x] ADR-0002 (Docker shell-out vs bollard) accepted
- [x] ADR-0006 (TOML manifest) accepted
- [x] ADR-0007 (XDG state storage) accepted
- [x] ADR-0009 (container reuse semantics) accepted
- [x] OQ-004 (UID strategy) resolved → numeric `--user $(id -u):$(id -g)`
- [x] OQ-005 (multi-match priority) resolved → `priority` field, ties on detect-count, error otherwise
- [ ] `sandbox-core::paths` (XDG resolution)
- [ ] `sandbox-core::lang` (LangManifest loader + detector)
- [ ] `sandbox-core::hash` (`git ls-files` based, walkdir fallback)
- [ ] `sandbox-core::profile` + `config` (load `~/.config/sandbox/config.toml`)
- [ ] `sandbox-core::project` (`Project` resolution + container_name)
- [ ] `sandbox-core::state` (per-project state at `$XDG_DATA_HOME/sandbox/containers/<hash>/`)
- [ ] `sandbox-docker::Plan` (pure data describing a `docker run`)
- [ ] `sandbox-docker::run/start/exec/stop/rm`
- [ ] `sandbox-docker::volume::ensure` (named volumes)
- [ ] `sandbox-docker::network::ensure` (`sandbox-internal` network)
- [ ] `sandbox-cli::commands::run` wires it all up
- [ ] `sandbox-cli::commands::down`
- [ ] `sandbox-cli::commands::nuke`
- [ ] Dotfiles bind mount (zshrc + starship; hybrid with `~/.config/sandbox/zsh/.zshrc.sandbox`)
- [ ] Integration test: `sandbox run` on a node project, drop into shell, see `package.json`

### Phase 2 — Volume strategy + network isolation

- [ ] Project mount as `:ro` in default mode
- [ ] Named volumes for each `package_dir` from manifest
- [ ] `sandbox-internal` network (created on first run)
- [ ] `--unsafe`, `--network`, `--profile` flags
- [ ] Profiles loaded from `~/.config/sandbox/config.toml`
- [ ] ADR-0003 (volume strategy) finalized
- [ ] ADR-0004 (network isolation) finalized
- [ ] ADR-0007 (state storage XDG) finalized

### Phase 3 — Lifecycle observability

- [ ] `sandbox ps` (table + json)
- [ ] `sandbox logs PROJECT [--follow]`
- [ ] `sandbox exec PROJECT -- CMD`
- [ ] Per-project state at `$XDG_DATA_HOME/sandbox/containers/<hash>/`

### Phase 4 — Scan pipeline

- [ ] `sandbox-scan::yara` (using `yara-x` crate)
- [ ] Bundled YARA rules for known IoCs (Contagious Interview, etc.)
- [ ] `sandbox-scan::heuristics` (regex patterns: `Function.constructor`, `runOn: "folderOpen"`, `Buffer.from(.*base64)`, suspicious `child_process.exec`, etc.)
- [ ] `sandbox-scan::compose` (compose file validator: `privileged`, host mounts, `network_mode: host`, etc.)
- [ ] Scan cache at `$XDG_CACHE_HOME/sandbox/scan/<hash>.toml`
- [ ] `sandbox scan [PATH]` standalone command
- [ ] Pre-flight scan integrated into `run`
- [ ] ADR-0008 (scan pipeline tiers) finalized

### Phase 5 — Reverse proxy + port detection

- [ ] `sandbox-proxy::traefik` (compose template + label generation)
- [ ] `sandbox proxy start|stop|status` subcommand
- [ ] Port auto-detection: `.env` parser + regex over source for `app.listen(N)`, `PORT=`, `bind = "0.0.0.0:N"`
- [ ] `--expose PORT[:NAME]` flag wires labels
- [ ] `*.sandbox.local` documentation (user adds to `/etc/hosts` or dnsmasq)
- [ ] ADR-0005 (Traefik) finalized

### Phase 6 — Runtime network toggle + project compose

- [ ] `sandbox net on|off|status PROJECT`
- [ ] Project compose detection
- [ ] `sandbox-scan::compose::validate` runs before `docker compose up`
- [ ] Sandbox container joins project's compose network
- [ ] `sandbox down --with-deps` and `sandbox nuke` cleanup compose deps
- [ ] ADR-0010 (project compose deps integration) finalized

### Phase 7 — Hardening + polish

- [ ] `--print-cmd` everywhere
- [ ] `--dry-run` mode end-to-end
- [ ] CPU/memory limits from profile
- [ ] Optional image digest pinning
- [ ] CI: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`
- [ ] User documentation site (mdbook) — optional

## Cross-session resume protocol

When starting a new Claude Code session in this repo:

1. **Always** read `docs/roadmap.md` first (you're here).
2. Read `AGENTS.md` for repo shape and reading priority.
3. Identify the **current phase** above; the first unchecked `[ ]` item is the next task.
4. Consult `docs/open-questions.md` for unresolved decisions that may block progress.
5. Pick up from where the last session left off.

If the user gives a high-level instruction (e.g. "vamos pra fase 2"), the assistant should:
- Verify all phase 1 boxes are checked (or explicitly waived).
- Read the relevant ADRs (status `Draft` → write content if missing, then implement).
- Update this roadmap as items are completed.

## Out of scope (for now)

See `docs/open-questions.md` and `docs/sad.md` "Future directions". Notably:

- macOS support (Linux-only for v0.1)
- LLM-assisted scan
- Bollard async Docker
- VM-based isolation backend
- Multi-shell support (only zsh for v0.1)
