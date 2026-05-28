# Release process

How code gets from a feature branch to a tagged release on crates.io. This doc
describes the **Git Flow shape** we use, the **CI gate** at each step, and the
**branch-protection rules** that enforce the model server-side.

> CI gate today: [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml).
> Release workflow (cross-compile + GitHub Release on tag push) is the next
> Block A PR; this doc already references it but it doesn't exist yet.

---

## Git Flow shape

```
                feat/*   fix/*   docs/*  chore/*
                   \      |       |       /
                    \     |       |      /        squash merge
                     ──>──┴───>───┴──>──┘─────────────────────────>  dev
                                                                       │
                                                       branch cut      │
                                                                       ▼
                                                                 release/X.Y.Z
                                                                       │
                                                  (only fixes here:    │
                                                   version bump, last  │
                                                   docs/CHANGELOG)     │
                                                                       │
                                              PR + CI green            │
                                              + tag vX.Y.Z             ▼
                                  ─────────────────────────────────>  main
                                                                       │
                                                          on tag push  ▼
                                                              release workflow:
                                                                 cross-compile
                                                                 + GitHub Release
                                                                 + cargo publish
```

**Three branch families, one rule per family:**

| Branch         | Source       | Target                  | Merge style | Lifetime |
|----------------|--------------|-------------------------|-------------|----------|
| `feat/*`       | `dev`        | `dev`                   | squash      | short    |
| `fix/*`        | `dev`        | `dev`                   | squash      | short    |
| `docs/*`       | `dev`        | `dev`                   | squash      | short    |
| `chore/*`      | `dev`        | `dev`                   | squash      | short    |
| `release/X.Y.Z`| `dev`        | `main` (+ back to `dev`)| **merge commit** | days     |
| `hotfix/X.Y.Z` | `main`       | `main` (+ back to `dev`)| **merge commit** | hours    |

`release/*` and `hotfix/*` use a real merge commit (no squash) so the tag points
at a commit whose history is reachable from both `main` and `dev`. Everything
else squashes — each merge to `dev` is one logical change, one commit.

---

## Step-by-step: cutting a release

1. **On `dev`**, decide the version bump. SemVer:
   - Breaking CLI surface change → major
   - New subcommand / flag / non-breaking feature → minor
   - Bugfix / docs / internal → patch
2. `git checkout -b release/X.Y.Z dev`
3. Bump `version` in workspace `Cargo.toml` (and the matching `workspace.dependencies` entries) — single source of truth via `version.workspace = true`.
4. Update `CHANGELOG.md` (next PR will automate this with `git-cliff`).
5. Push, open PR `release/X.Y.Z → main`. CI runs the full battery (lint + test ubuntu + msrv + test-docker + test-macos) on this PR.
6. Once green and approved, merge **with a merge commit** (no squash).
7. On `main`, tag the merge commit: `git tag -s vX.Y.Z -m "vX.Y.Z"` and `git push origin vX.Y.Z`.
8. The release workflow ([`.github/workflows/release.yml`](../../.github/workflows/release.yml)) picks up the `push: tags: v*` trigger and runs:
   - **`build`** — cross-compiles 4 targets: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`. Linux aarch64 uses `cross` (QEMU); the rest build natively. Each produces `sandbox-<target>.tar.gz` + a per-asset `.sha256`.
   - **`release`** — aggregates an `SHA256SUMS` file, generates notes from `git log <prev-tag>..<tag>`, and creates the GitHub Release with all tarballs + `SHA256SUMS` attached.
   - **`publish-crates`** — `cargo publish` in dependency order (`sandbox-cli-core` → `sandbox-docker` → `sandbox-scan` → `sandbox-proxy` → `sandbox-cli`) with 30s sleeps for crates.io index propagation. Uses `--no-verify` on the intermediates. **Skipped automatically** when the tag contains `-` (e.g. `v0.2.0-rc1` is prerelease, no crates.io publish).
9. **Back-merge** `main` into `dev` (`git checkout dev && git merge --no-ff main`) so the version bump propagates to the integration branch.

**Pre-requisite (one-time):** add the `CARGO_REGISTRY_TOKEN` secret to the repo (Settings → Secrets and variables → Actions). Generate the token at https://crates.io/me with "Publish update" scope on the affected crates.

### Hotfixes

Identical to a release, but branch off `main` instead of `dev`, and always
back-merge the same merge commit to `dev` after the tag.

---

## CI gate (what every PR must pass)

The workflow at [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml)
**splits jobs by branch target**: cheap jobs run on every change as a quick
safety net; heavy jobs (macOS, docker-tests) only run on the release path,
where the cost of a thorough check is justified.

| Job           | Runner        | Command                                                  | dev (PR + push) | release/* + main (PR + push) | Required |
|---------------|---------------|----------------------------------------------------------|:---:|:---:|---|
| `lint`        | ubuntu-latest | `cargo fmt --check` + `cargo clippy -- -D warnings`      | ✅ | ✅ | yes |
| `test`        | ubuntu-latest | `cargo test --workspace` (no docker-tests)               | ✅ | ✅ | yes |
| `msrv`        | ubuntu-latest | `cargo check --workspace --all-targets` on Rust **1.91** | ✅ | ✅ | yes |
| `test-macos`  | macos-latest  | `cargo test --workspace`                                 | ❌ | ✅ | informational * |
| `test-docker` | ubuntu-latest | `cargo test -p sandbox-cli --features docker-tests`      | ❌ | ✅ | yes (main only) |

\* macOS stays informational until it's been green for a few weeks. Promote it
to required by removing `continue-on-error` on the job and adding it to the
`main` branch-protection required-checks list.

**Why the split:**
- `dev` is the integration WIP branch — many small PRs per sprint. Running
  the heavy battery on each one burns minutes (and time) for marginal gain.
- `release/*` and `hotfix/*` PRs target `main`; that's where the full check
  matters. macOS + docker-tests catch issues that `dev` accumulation hides.
- `push: main` re-runs the full battery as defense-in-depth (the same
  workflow that approved the release PR runs once more on the merged SHA).

**Cost note:** this repo is public, so `macos-latest` minutes are free on
GitHub Actions. If billing changes (private fork, policy shift), the macOS
job is the first to disable; a self-hosted [docker-osx](https://github.com/sickcodes/docker-osx)
runner is a candidate replacement.

Locally, the same gates run via [`lefthook`](../../lefthook.yml), installed
**project-locally** (binary at `./bin/lefthook`, shims in `.githooks/`, wired
via `core.hooksPath` — no global install). One-time setup:

```sh
./scripts/dev/install-hooks.sh
```

Hooks:
- **pre-commit**: `cargo fmt --check` (fast, every commit)
- **pre-push**: `scripts/dev/lint.sh` + `scripts/dev/test.sh` (matches CI default test)

`docker-tests` are *not* run on pre-push — they're slow and need the daemon
warm. Run them manually before opening a release PR:

```sh
cargo test -p sandbox-cli --features docker-tests
```

---

## Branch protection rules

These live in GitHub Settings → Branches (or Rulesets) — they are server-side
state, not code, so they don't ship in this repo. Apply once per repo.

### `main` (production)

- ☑ Require a pull request before merging
- ☑ Require approvals: **1**
- ☑ Dismiss stale approvals on new commits
- ☑ Require status checks to pass before merging:
  - `lint (fmt + clippy)`
  - `test (ubuntu-latest)`
  - `msrv (1.91)`
  - `test-docker`
  - *(promote `test (macos-latest)` here once stable)*
- ☑ Require branches to be up to date before merging
- ☑ Require linear history
- ☑ **Restrict who can push** — no direct pushes; only PRs
- ☑ **Restrict pushes that create matching files** (Rulesets) — restrict source
  branches to `release/*` and `hotfix/*` patterns
- ☐ Require signed commits *(recommended for v1.0)*

### `dev` (integration)

Lighter gate — only the cheap jobs run here.

- ☑ Require a pull request before merging
- ☑ Require status checks to pass before merging:
  - `lint (fmt + clippy)`
  - `test (ubuntu-latest)`
  - `msrv (1.91)`
- ☑ Require branches to be up to date before merging
- ☑ Allow squash merging only (disable merge commits + rebase for `dev` PRs)
- ☐ Approvals not required (solo dev; flip to 1 once the project takes contributors)

### Apply via `gh api`

The CLI snippet below applies the `main` rules to `ConsoliDados/sandbox`. Run it
once; rerun whenever you add a required check (e.g., a future `cargo-deny` job).
Requires `gh auth status` showing admin scope on the repo.

**`main` (heavy gate):**

```sh
gh api -X PUT \
  /repos/ConsoliDados/sandbox/branches/main/protection \
  -H "Accept: application/vnd.github+json" \
  -f required_status_checks.strict=true \
  -f 'required_status_checks.contexts[]=lint (fmt + clippy)' \
  -f 'required_status_checks.contexts[]=test (ubuntu-latest)' \
  -f 'required_status_checks.contexts[]=msrv (1.91)' \
  -f 'required_status_checks.contexts[]=test (docker-tests)' \
  -F enforce_admins=true \
  -F required_pull_request_reviews.required_approving_review_count=1 \
  -F required_pull_request_reviews.dismiss_stale_reviews=true \
  -F required_linear_history=true \
  -F allow_force_pushes=false \
  -F allow_deletions=false \
  -F restrictions=null
```

**`dev` (light gate — heavy jobs don't run here, so don't require them):**

```sh
gh api -X PUT \
  /repos/ConsoliDados/sandbox/branches/dev/protection \
  -H "Accept: application/vnd.github+json" \
  -f required_status_checks.strict=true \
  -f 'required_status_checks.contexts[]=lint (fmt + clippy)' \
  -f 'required_status_checks.contexts[]=test (ubuntu-latest)' \
  -f 'required_status_checks.contexts[]=msrv (1.91)' \
  -F enforce_admins=false \
  -F required_linear_history=true \
  -F allow_force_pushes=false \
  -F allow_deletions=false \
  -F restrictions=null
```

> **Important:** if `dev` required `test-docker` or `test (macos-latest)`,
> every PR to `dev` would be stuck — those jobs are skipped there. The lists
> above intentionally differ.

For the source-branch restriction on `main` (`release/*`, `hotfix/*` only), use
**Rulesets** via Settings → Rules → Rulesets — the legacy branch protection API
doesn't express source patterns. The ruleset shape:

- **Target**: branch `main`
- **Bypass**: empty (no exceptions, including admins)
- **Rules**: `pull_request` with `dismiss_stale_reviews_on_push: true`,
  `required_approving_review_count: 1`; `required_status_checks` (the main-gate
  list above); `non_fast_forward` denied; `required_signatures` (when ready)
- **Restrict pull request sources**: branches matching `release/*` or `hotfix/*`

---

## Related

- [`roadmap.md § Road to 1.0.0 — A. Release engineering`](roadmap.md#a--release-engineering) — what still has to ship to get to a full release pipeline.
- [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) — the workflow this doc describes.
- [`lefthook.yml`](../../lefthook.yml) — local hooks that mirror the CI gate.
- [`scripts/dev/lint.sh`](../../scripts/dev/lint.sh) / [`test.sh`](../../scripts/dev/test.sh) — the underlying commands.
