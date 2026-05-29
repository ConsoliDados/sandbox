# Roadmap

Source of truth for "what's done, what's next, where are we." A fresh session should read this first to resume work.

## Current status

**v0.1.0 shipped** — published to crates.io (`cargo install sandbox-cli`) and released to `main` (2026-05-27). Phases 1–6 complete on `dev`/`main`. Next milestone: **1.0.0** — see [§ Road to 1.0.0](#road-to-100).

`sandbox run/down/nuke/ps/logs/exec/scan/proxy` are wired end-to-end against a real Docker daemon. Pre-flight scan runs before `docker run` in safe/paranoid (`--with-clamav` adds the AV motor); blocking findings (severity ≥ High) exit 30. `sandbox proxy start` brings up Traefik with one entryPoint per registered port; project containers join both `sandbox-internal` (egress restricted) and `sandbox-proxy` (Traefik routing). Reachable via `<slug>.sandbox.localhost:<PORT>` — `.localhost` resolves to loopback per RFC 6761, so no `/etc/hosts` edits needed.

Phase 6 adds, on this branch: `sandbox net on|off|status` (runtime egress toggle by attaching/detaching `bridge`), project compose deps via `sandbox run --with-deps` (rewired onto an `--internal` network in safe mode), the compose registry allowlist scan rule, and `sandbox attach` — re-enter a running sandbox's shell without re-scanning (exiting the shell leaves the container running; PID 1 is a keepalive). 200+ tests pass headlessly; tests that drive Docker for real are behind the `docker-tests` feature.

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
- [x] `sandbox logs PROJECT [--follow] [--tail] [--since]`
- [x] `sandbox exec PROJECT -- CMD [--user] [--workdir]`
- [x] Per-project state at `$XDG_DATA_HOME/sandbox/containers/<hash>/`
- [x] Exit code 40 for container-not-found / not-running (per SRS)

### Phase 4a — Scan pipeline (YARA + heuristics + compose)

- [x] `sandbox-scan::yara` (using `yara-x` crate)
- [x] Bundled YARA rules for known IoCs (Contagious Interview / Lazarus 2026-05-06)
- [x] `sandbox-scan::heuristics` (vscode autorun, package.json hooks, eval shapes, base64+network)
- [x] `sandbox-scan::compose` (privileged, host namespaces, dangerous caps, host mounts)
- [x] Scan cache at `$XDG_CACHE_HOME/sandbox/scan/<hash>.toml` with ruleset versioning
- [x] User-global suppression at `~/.config/sandbox/scan-ignore.toml` (OQ-007 resolved)
- [x] `sandbox scan [PATH] [--no-cache] [--explain] [--format json|table]` standalone
- [x] Pre-flight scan integrated into `run` (exit 30 on severity ≥ High)
- [x] `--no-scan` flag (requires `--unsafe` per SRS)
- [x] ADR-0008 (scan pipeline tiers) accepted

### Phase 4b — ClamAV motor

- [x] `sandbox/scanner:latest` image (alpine + clamav + freshclam) — bundled Dockerfile, built on demand from `crates/sandbox-scan/scanner-image/`
- [x] Ephemeral scan container in `sandbox-docker::scanner` (named volume `sandbox-scanner-db`)
- [x] `sandbox scan --update-db` (bridge network, run freshclam, exit)
- [x] `sandbox-scan::clamav` output parser (clamscan `--no-summary --infected` → Findings, Critical severity)
- [x] ClamAV stage opt-in via `--with-clamav` on `sandbox scan` and `sandbox run` (profile-driven default deferred to Phase 7)

### Phase 5 — Reverse proxy + port detection

- [x] `sandbox-proxy::traefik` (compose + static config + lifecycle ops)
- [x] `sandbox-proxy::labels` (Traefik label generation per ADR-0005)
- [x] `sandbox-proxy::ports` (port detection: `.env` + regex source + manifest fallback)
- [x] `sandbox proxy start|stop|status|logs` subcommand
- [x] `--expose PORT` flag (repeatable) — short-circuits detection
- [x] `Plan.labels` + `Plan.additional_networks` + `lifecycle::run` create+connect+start path
- [x] `*.sandbox.localhost` resolves natively via `nss-myhostname` (RFC 6761) — zero host setup
- [x] ADR-0005 (Traefik) Accepted (port-distinguishes-services model)

### Phase 6 — Runtime network toggle + project compose

- [x] `sandbox net on|off|status PROJECT` — ephemeral toggle (no Meta persistence), table+JSON output, exit 50 on would-strand guard
- [x] `sandbox attach [PATH]` (alias `shell`) — re-enter a running sandbox's shell via `docker exec`, no scan, no auto-start; missing/stopped → exit 40. Pairs with the keepalive PID 1 (`exit` no longer kills the container).
- [x] Project compose detection — `sandbox-docker::compose::discover` (regex `^(docker-compose|compose).*\.ya?ml$`, depth 4, skip-dir set, multi-match → Error, `--compose-file` override validated + canonicalized)
- [x] `sandbox-scan::compose::validate` runs before `docker compose up` — pre-flight scan in `sandbox run` already exercises `scan::compose::scan`; with `--with-deps` the same validator gates `compose_up_flow`. Exit 30 (scan blocked) on findings ≥ High.
- [x] **Registry/namespace allowlist** in compose validator (`compose/registry_not_allowed`, severity High) — default allow: `docker.io/library/*`, `ghcr.io/*`. Image-ref parser handles tags + `@digest`. User config extension still pending. RULESET_VERSION bumped 2 → 3.
- [x] Sandbox container joins project's compose network (3 networks: internal + proxy + compose) — `Plan.additional_networks` now ingests `ctx.compose_state.network`; same create+connect+start path Phase 5 added.
- [x] Post-`up` network rewire in safe mode — `sandbox-docker::rewire_to_internal` disconnects each service from compose-default and reconnects to `sandbox-compose-<short>` (`--internal`) with the service name as DNS alias. `--network` mode keeps deps on compose-default bridge.
- [x] `sandbox down --with-deps` and `sandbox nuke` cleanup compose deps — both read `Meta.compose`; `compose_down` is idempotent; `--internal` network rm is best-effort.
- [x] ADR-0010 (project compose deps integration) finalized — Accepted 2026-05-16

### Phase 7 — Hardening + polish

- [ ] `--print-cmd` everywhere
- [ ] `--dry-run` mode end-to-end
- [ ] CPU/memory limits from profile
- [ ] Optional image digest pinning (closes mutable-tag swap vector)
- [ ] CI: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`
- [ ] User documentation site (mdbook) — optional

### Phase 8 — Image supply chain hardening (post-v0.1, study first)

Status: **backlog, pending study.** Registry allowlist (Phase 6) and digest pinning (Phase 7) already cover the highest-signal vectors. This phase bundles the deeper, costlier work that needs design decisions before implementation.

- [ ] **Image signing / provenance verification** — cosign / sigstore. Verify pulled images are signed by a trusted maintainer. Ecosystem still settling; needs survey of what fraction of the registries we care about actually sign.
- [ ] **CVE scanning** — Trivy / Grype / Syft against pulled images. Hard problem is **noise**: base images carry dozens of Low/Medium CVEs that are not exploitable in our context. Decision needed on severity gating, suppression scope, cache strategy, and whether to scan deps or just base images.
- [ ] **Layer content scan** — extract image layers and run YARA / heuristics over file contents (analogous to the source scan we already do). Catches malware embedded in images that registry allowlist would miss. Expensive — `docker save` + tar extract + per-file scan.

See OQ-008 for the open questions that must close before any of this lands on a real phase.

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

Items previously listed here but now tracked as future phases:

- **Image supply chain (signing, CVE, layer scan)** — moved to Phase 8 backlog above. Blocked on OQ-008.

## v0.1.0 release track

The first distributable binary. Pre-release security review is done (no critical blocker) and
the remaining work is captured as an ordered backlog. See:

- [security-review-v0.1.md](security-review-v0.1.md) — pre-release audit + verdict.
- [release-v0.1-backlog.md](release-v0.1-backlog.md) — ordered workstreams: close Phase 6,
  bind-localhost hardening (ADR-0012), `lang`/`config`, and release engineering (CI, crates.io,
  install script).
- [release-runbook.md](release-runbook.md) — crate-name reservations and the publish procedure.
- [ADR-0012](adrs/0012-localhost-port-binding.md) — proxy ports bind to loopback by default.

Distribution: published to crates.io as `sandbox-cli` (binary stays `sandbox`) **and** prebuilt
binaries via CI, with `install.sh` and `cargo install`.

## Road to 1.0.0

v0.1.0 shipped (crates.io + `main`, 2026-05-27). v0.1.1 followed (2026-05-29) — same surface, but **first tag through the new release pipeline** (CI gate + cross-compile + binaries + auto-publish). **1.0.0 = a complete CLI surface + real distribution + the Phase 7 hardening + persistent trust + a meaningful detection catalog.** Phase 8 (image supply chain) and first-class platform support are explicitly post-1.0. Target = **A + B + C + D + E** below.

### Priority order (decided 2026-05-29 post-v0.1.1)

Sections A→E below are grouped by **category**. The actual **ship order** is:

1. **E.1** — Bundle more curated YARA families *(highest priority — the biggest user-visible defense gap; today the catalog is one family, the engine is generic)*
2. **C.1 + E.2 bundled** — Implement `sandbox lang` **together with** user-overridable YARA rules (`sandbox rules` family). Both share the **XDG drop-in pattern** (`~/.config/sandbox/{languages,rules}/`), the manifest-loading helpers, and the `list|show|add|validate` subcommand shape. Shipping them apart writes the same plumbing twice.
3. **C.2** — `sandbox config edit|show|path`
4. **B** — Hardening (`--print-cmd` everywhere, `--dry-run`, CPU/mem limits, image digest pinning, mdbook)
5. **D** — Trust + scan defaults (persistent trust, `paranoid` → mandatory ClamAV)
6. **A cleanup** — Linux musl x86_64, CHANGELOG automation
7. **E.3** — Feed subscription *(post-1.0)*

This sequence puts **detection coverage first** (the strongest user-visible promise of the project — quality of the scan gate), then closes the **public CLI contract** (subcommands the SRS promises but the binary returns `NotImplemented`), then internal polish. The reason E.1 leads everything: the scan motor is generic but the catalog is minimal — any other YARA-catalogued family in the wild today passes through untouched. That's a bigger user-visible gap than any of the unimplemented subcommands.

### A — Release engineering
- [x] Prebuilt binaries + CI release workflow on tag `v*` for `x86_64`/`aarch64` Linux (glibc) and macOS — `install.sh` verifies SHA256, falls back to `cargo install` only when no asset matches. See [`docs/sandbox/release-process.md`](release-process.md).
- [x] PR CI: `cargo fmt --check` + `cargo clippy -- -D warnings` + `cargo test` (+ `docker-tests` job + MSRV check).
- [x] GitHub Release object per tag (page + notes auto-generated from `git log`).
- [ ] Linux musl x86_64 (fully-static binary, distroless/Alpine-friendly) added to the release matrix.
- [ ] `CHANGELOG.md` automation (`git-cliff`).

### B — Hardening + polish (Phase 7)
- [ ] `--print-cmd` on every command; `--dry-run` end-to-end.
- [ ] CPU/memory limits from the profile — sane defaults baked in, user overrides in `config.toml`.
- [ ] Optional image digest pinning (closes the mutable-tag swap vector).
- [ ] Docs site (mdbook) — hosting TBD (ConsoliDados site / GitHub Pages / mdbook).

### C — Complete the CLI surface
- [ ] **C.1 — Ships bundled with E.2.** Implement `sandbox lang list|show|add|validate` AND `sandbox rules list|show|add|validate` in the same cycle. Both manage user-extensible drop-in directories under XDG config (`~/.config/sandbox/languages/*.toml` and `~/.config/sandbox/rules/*.yar` respectively); they reuse the same dir-scanner + validator + subcommand shape. See [Priority order](#priority-order).
- [ ] **C.2** — Implement `sandbox config edit|show|path` (today returns `NotImplemented`).

### D — Trust + scan defaults
- [ ] **OQ-003** — persistent trust (`trusted.toml`: project hash → trust level) so frequently-used projects skip the trust dial.
- [ ] `paranoid` profile runs ClamAV **mandatorily** (today it's opt-in via `--with-clamav`).

### E — Expand detection coverage

Today the YARA engine ships **one** rule file (`contagious_interview.yar`, the family that originated the project). The engine itself (`yara-x` via [`YaraEngine::builtin`](../../crates/sandbox-scan/src/yara/mod.rs)) is generic — it's the **catalog** that's intentionally minimal. To stay valuable beyond the originating incident, two priorities and one stretch:

- [ ] **E.1 — Bundle more curated families** *(next ship, highest priority across the whole 1.0 plan)*. Add `*.yar` files under `crates/sandbox-scan/src/yara/rules/` for high-confidence shapes that warrant default-block:
  - **JS / npm supply-chain shapes** with stable IoCs (`ua-parser-js`-class postinstall hijacks, `event-stream`-class hidden-dep injectors, classic typosquat C2 patterns).
  - **Discord/Telegram webhook exfil** patterns common in junior-bait challenges.
  - **Crypto-stealer JS** (clipboard hijack + address-replace + wallet-API drain shapes).
  - **Classic obfuscator signatures** when they're unambiguous (e.g. `obfuscator.io` headers in dependency code).
  - Each family must ship with **both positive and negative fixtures** validating `clean_passes` (the FP-rate gate). Bump `RULESET_VERSION` in `cache.rs` per add so existing scan caches re-evaluate. See `sandbox-scan/AGENTS.md` "How to extend".

- [ ] **E.2 — User-overridable YARA rules** (the threat-intel / DFIR escape hatch). **Ships bundled with C.1** (`sandbox lang`/`sandbox rules` are sister subcommands over the same XDG drop-in pattern). Load `*.yar` from `~/.config/sandbox/rules/` (same pattern as `languages/*.toml`) at engine startup, in addition to bundled. Config knob:
  ```toml
  [scan.yara]
  extra_rule_dirs    = ["~/.config/sandbox/rules", "~/work/threat-intel/yara"]
  severity_floor     = "warn"   # user rules below this floor never block; only flag in --explain
  ```
  Default `severity_floor = "warn"` so dropped-in public feeds (florianroth/signature-base et al.) **flag without blocking**, keeping default-mode usable. Power users (forensics, IR, threat hunters) drop their own curated `.yar` and the engine compiles them at startup — no rebuild needed.

- [ ] **E.3** *(Post-1.0)* **Feed subscription** — `sandbox scan --update-rules` pulling from curated public feeds (Elastic protections-artifacts, Abuse.ch YARAify, DFIR Report). Cached at `~/.cache/sandbox/yara/<feed>/`, signed manifest, severity always clamped to `warn` (only bundled/user-explicit blocks). Out of scope for 1.0 because it brings ongoing maintenance overhead (feed signing, version pinning, FP-mitigation per feed) — better proven first with the user-overridable path.

### Post-1.0 (stays on the roadmap)
- **OQ-002 — commit-from-sandbox path:** 99% of projects ship their lockfiles (no commit-from-container needed), and the rare install/build *inside* the sandbox runs under `--unsafe` just fine — deferred until a real workflow demands it. Candidate when it does: a `sandbox sync-lock` (copy lockfile volume → host) or a surgical RW mount of `.git`.
- **Platform & surface (1.1+):** macOS / WSL2 best-effort → first-class; a formal CLI-surface stability guarantee; more bundled language manifests.
- **Phase 8 — image supply chain:** signing/cosign, CVE scan, layer scan (pending OQ-008).
