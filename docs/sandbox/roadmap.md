# Roadmap

Source of truth for "what's done, what's next, where are we." A fresh session should read this first to resume work.

## Current status

**Phase 3 — lifecycle observability.** In progress on `feat/phase-3-observability`. `sandbox ps` ships table + json output; `logs` and `exec` are next. Phases 1+2 merged into `dev` via PR #1 (2026-05-12).

`sandbox run/down/nuke/ps` are wired end-to-end against a real Docker daemon.
`--print-cmd` shows the rendered `docker` invocation for every wired command.
62 tests pass headlessly (42 core + 11 docker + 4 cli unit + 5 cli integration);
tests that drive Docker for real are behind the `docker-tests` feature.

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
- [x] `sandbox-core::paths` (XDG resolution) — `c8ec734`
- [x] `sandbox-core::lang` (LangManifest loader + detector) — `c8ec734`
- [x] `sandbox-core::hash` (canonical-path based, see ADR-0009) — `c8ec734`
- [x] `sandbox-core::profile` + `config` (load `~/.config/sandbox/config.toml`) — `6ccd08b`
- [x] `sandbox-core::project` (`Project` resolution + container_name) — `6ccd08b`
- [x] `sandbox-core::state` (per-project state at `$XDG_DATA_HOME/sandbox/containers/<hash>/`) — `6ccd08b`
- [x] `sandbox-docker::Plan` (pure data describing a `docker run`)
- [x] `sandbox-docker::run/start/exec/stop/rm`
- [x] `sandbox-docker::volume::ensure` (named volumes)
- [x] `sandbox-docker::network::ensure` (`sandbox-internal` network)
- [x] `sandbox-cli::commands::run` wires it all up
- [x] `sandbox-cli::commands::down`
- [x] `sandbox-cli::commands::nuke`
- [x] Dotfiles bind mount (zshrc + starship; hybrid with `~/.config/sandbox/zsh/.zshrc.sandbox`)
- [x] Integration test: `sandbox run --print-cmd` on a node project (full lifecycle test gated behind `docker-tests` feature)

### Phase 2 — Volume strategy + network isolation

- [x] Project mount as `:ro` in default mode
- [x] Named volumes for each `package_dir` from manifest
- [x] Lockfile mounts: state-dir bind RW in safe/paranoid (per ADR-0003)
- [x] `sandbox-internal` network (created on first run)
- [x] `--unsafe`, `--network`, `--profile` flags
- [x] Profiles loaded from `~/.config/sandbox/config.toml`
- [x] ADR-0003 (volume strategy) finalized
- [x] ADR-0004 (network isolation) finalized
- [x] ADR-0007 (state storage XDG) finalized

### Phase 3 — Lifecycle observability

- [x] `sandbox ps` (table + json)
- [ ] `sandbox logs PROJECT [--follow]`
- [ ] `sandbox exec PROJECT -- CMD`
- [x] Per-project state at `$XDG_DATA_HOME/sandbox/containers/<hash>/`

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

1. **Always** read `roadmap.md` first (you're here).
2. Read `AGENTS.md` for repo shape and reading priority.
3. Identify the **current phase** above; the first unchecked `[ ]` item is the next task.
4. Consult `open-questions.md` for unresolved decisions that may block progress.
5. Pick up from where the last session left off.

If the user gives a high-level instruction (e.g. "vamos pra fase 2"), the assistant should:
- Verify all phase 1 boxes are checked (or explicitly waived).
- Read the relevant ADRs (status `Draft` → write content if missing, then implement).
- Update this roadmap as items are completed.

## Out of scope (for now)

See `open-questions.md` and `sad.md` "Future directions". Notably:

- macOS support (Linux-only for v0.1)
- LLM-assisted scan
- Bollard async Docker
- VM-based isolation backend
- Multi-shell support (only zsh for v0.1)
