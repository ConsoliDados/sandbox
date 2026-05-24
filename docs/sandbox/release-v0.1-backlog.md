# Release v0.1.0 — backlog

Source of truth for what remains between today's branch state and the first distributable
binary. Written during the pre-release review (2026-05-24). Companion docs:
[security-review-v0.1.md](security-review-v0.1.md), [release-runbook.md](release-runbook.md),
[ADR-0012](adrs/0012-localhost-port-binding.md).

**Distribution decision (locked):** publish to crates.io **and** ship prebuilt binaries via CI,
with an `install.sh` and `cargo install`. Published crate name is `sandbox-cli` (the binary
stays `sandbox`); see the runbook for the naming/collision details.

Items are ordered by dependency. Checkboxes are the unit of work for the implementation phase
that follows the docs PR merge.

## A. Close Phase 6 (network toggle + compose + attach)

The feature work already lives on `feat/phase-6-network-toggle` (uncommitted/Phase-6 WIP). This
backlog only tracks what's needed to *land* it.

- [ ] Run smoke **6.1** `sandbox net on/off/status` round-trip (live) — the canonical "does net
  work?" check. Recipe: `smoke-tests.md` § 6.1.
- [ ] Run smoke **6.3** (`--with-deps` safe-mode rewire), **6.6** (`down --with-deps` / `nuke`
  cleanup), **6.8** (`attach` re-entry) — live.
- [ ] Make the remaining Phase 6 commits; squash-merge `feat/phase-6-network-toggle` → `dev`.
- [ ] Flip the Phase 6 checkboxes in `roadmap.md` to done.

## B. Security hardening — bind published ports to loopback (ADR-0012)

- [ ] `crates/sandbox-proxy/src/traefik.rs::render_compose`: emit `"127.0.0.1:<port>:<port>"`
  for project entryPoints and for the `--dashboard` port.
- [ ] Add `proxy.bind_address` (default `"127.0.0.1"`) to the config model and thread it through
  `ProxyConfig`.
- [ ] Update the `render_compose` unit tests that currently assert `"3000:3000"`.
- [ ] `threat-model.md` already notes inbound exposure (this PR); confirm wording matches the
  shipped behavior.

## C. Implement `lang` and `config` (minimal)

Both are documented in the SRS but return `Error::NotImplemented` today
(`crates/sandbox-cli/src/main.rs:389-392`).

- [ ] `crates/sandbox-cli/src/commands/lang.rs`: `list` (bundled `languages/*.toml` ∪
  `~/.config/sandbox/languages/`), `show NAME`, `validate FILE`. Reuse the existing manifest
  loader/detector in `sandbox-core::lang` — do not reimplement TOML parsing.
- [ ] `crates/sandbox-cli/src/commands/config.rs`: `show` (effective config) and `path`. `edit`
  (`$EDITOR`) optional for v0.1.
- [ ] Register both in `commands/mod.rs`; replace the `NotImplemented` arm in `main.rs`.
- [ ] Update `srs.md` command status; add smoke recipes for `lang list/show/validate` and
  `config show/path`.
- [ ] (Optional) wire exit code 10 ("cannot detect language") while in `lang`/`run` paths.

## D. Release engineering

Depends on A–C being merged. Detailed procedure in [release-runbook.md](release-runbook.md).

- [ ] Bump workspace version `0.0.1` → `0.1.0` (root `Cargo.toml`).
- [ ] Rename the **published package** of the core crate to `sandbox-cli-core` via
  `[package] name`, keeping `[lib] name = "sandbox_core"` so no `use sandbox_core` changes.
  Update the path-dep in `sandbox-cli/Cargo.toml` with `package = "sandbox-cli-core"`.
- [ ] Add publish metadata to each published crate: `description`, `repository`, `readme`,
  `keywords`, `categories` (license already inherited from the workspace).
- [ ] Add `LICENSE-MIT` and `LICENSE-APACHE` at repo root (dual license is declared but the
  files are missing).
- [ ] README: install section — `cargo install sandbox-cli`, `install.sh`, prerequisites
  (Docker on PATH, `docker compose` v2), supported platforms.
- [ ] `CHANGELOG.md` (keepachangelog) with a `v0.1.0` entry.
- [ ] CI workflow `.github/workflows/ci.yml`: `cargo fmt --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `cargo test --workspace` (Docker-gated tests skipped on the
  runner), `cargo build --release`. Reuse the logic in `scripts/dev/lint.sh` and `test.sh`.
- [ ] Release workflow `.github/workflows/release.yml` on tag `v*`: cross-compile
  (`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, optionally `*-musl`, macOS), attach
  artifacts to the GitHub Release. Evaluate `cargo-dist` to generate this + `install.sh`.
- [ ] `install.sh`: detect platform, download the matching release artifact, install to
  `~/.local/bin`. (If `cargo-dist` is adopted, it generates this.)
- [ ] Publish to crates.io in dependency order (see runbook); gate on `cargo publish --dry-run`.

## E. Polish (non-blocking)

- [ ] Exit codes 10 / 31: wire them or document the 1/30 collapse as a known limitation in the
  SRS § Global.
- [ ] Confirm whether `--dry-run` exists distinct from `--print-cmd`; align the SRS wording.

## Done / not in this track

- Security review complete — [security-review-v0.1.md](security-review-v0.1.md). No critical
  blocker.
- `--version`, CPU/mem limits, release profile (LTO/strip), MSRV 1.85 / edition 2024 on the
  installed toolchain (rustc 1.93.1) — already in place, no work needed.
- Image supply chain (signing/CVE/layer scan) — Phase 8, out of v0.1 (OQ-008).
