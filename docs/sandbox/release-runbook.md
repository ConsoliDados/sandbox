# Release runbook — crates.io + binaries

How to reserve the crate names now and publish v0.1.0 later. Companion to
[release-v0.1-backlog.md](release-v0.1-backlog.md).

## Crate naming (collision-checked 2026-05-24)

`cargo install <name>` needs the package **and all its path-dependency crates** published on
crates.io. Two of our preferred names are already taken:

| Crate (path) | Preferred name | crates.io | Published as |
|---|---|---|---|
| `crates/sandbox-cli` | `sandbox` | **TAKEN** | **`sandbox-cli`** |
| `crates/sandbox-core` | `sandbox-core` | **TAKEN** | **`sandbox-cli-core`** |
| `crates/sandbox-docker` | `sandbox-docker` | available | `sandbox-docker` |
| `crates/sandbox-scan` | `sandbox-scan` | available | `sandbox-scan` |
| `crates/sandbox-proxy` | `sandbox-proxy` | available | `sandbox-proxy` |

**The binary stays `sandbox`** regardless — only the published package name and the
`cargo install` command change. Install command becomes `cargo install sandbox-cli`.

### The core-crate rename without touching source

`sandbox-core` is taken, so the core crate publishes under `sandbox-cli-core`. Keep the library
name stable so **no `use sandbox_core::...` in the codebase changes**:

```toml
# crates/sandbox-core/Cargo.toml
[package]
name = "sandbox-cli-core"     # crates.io package name (was sandbox-core)
# ...
[lib]
name = "sandbox_core"         # import path stays `sandbox_core`
```

```toml
# every crate that depends on it, e.g. crates/sandbox-cli/Cargo.toml
[dependencies]
sandbox_core = { package = "sandbox-cli-core", path = "../sandbox-core", version = "0.1.0" }
```

The `package = "..."` rename lets dependents keep referring to it by the old key while the
published artifact carries the new name.

## Step 1 — Reserve the names now (run by the maintainer)

crates.io has no "reserve without publishing"; you reserve a name by publishing a minimal
version. Worth doing now since two names were already lost. **Publishing is irreversible** (you
can only `yank`, not delete) — which is exactly what makes the reservation stick.

```sh
# one-time: paste a token from https://crates.io/settings/tokens
cargo login

# for EACH of: sandbox-cli  sandbox-cli-core  sandbox-docker  sandbox-scan  sandbox-proxy
cargo new --lib /tmp/reserve && cd /tmp/reserve
# edit Cargo.toml:
#   name = "<the-name>"
#   version = "0.0.0"
#   edition = "2021"
#   description = "Placeholder — reserved for the sandbox project"
#   license = "MIT OR Apache-2.0"
#   repository = "https://github.com/johnnycarreiro/sandbox"
cargo publish --allow-dirty
```

The real `0.1.0` later supersedes the `0.0.0` placeholder (higher version, no conflict).

> Note: crates.io policy discourages pure name-squatting. Reserving names for a project under
> active development is fine; just follow through with the real release.
>
> **Alternative:** skip reservation and publish the real crates directly when ready. Lower
> clutter, but risks losing `sandbox-cli` et al. in the meantime. Given two names were already
> taken, reserving now is recommended.

## Step 2 — Prepare crates for the real publish

Per crate to be published, add the metadata crates.io requires/recommends (license is inherited
from the workspace):

```toml
description = "<one line>"
repository  = "https://github.com/johnnycarreiro/sandbox"
readme      = "README.md"          # or a crate-local README
keywords    = ["docker", "sandbox", "security", "isolation"]   # max 5
categories  = ["command-line-utilities", "development-tools"]
```

Bump the workspace version to `0.1.0` (root `Cargo.toml`). Verify each crate builds and that
path deps carry a `version` (required for a path dep to be publishable).

## Step 3 — Dry-run, then publish in dependency order

Deps must exist on crates.io before dependents. Order:

```
1. sandbox-cli-core            (crates/sandbox-core)
2. sandbox-docker  sandbox-scan  sandbox-proxy
3. sandbox-cli                 (crates/sandbox-cli)
```

```sh
# validate first — no upload
cargo publish -p sandbox-cli-core --dry-run
# ... repeat --dry-run for each ...

# then publish for real, waiting for the index to update between tiers
cargo publish -p sandbox-cli-core
cargo publish -p sandbox-docker
cargo publish -p sandbox-scan
cargo publish -p sandbox-proxy
cargo publish -p sandbox-cli
```

Consider `cargo-release` to automate the version bump + tag + ordered publish.

## Step 4 — Binaries and install paths

Three install paths to support:

1. **`cargo install sandbox-cli`** — works once Step 3 lands. Builds from source; needs the
   Rust toolchain on the user's machine.
2. **Prebuilt binaries via GitHub Releases** — the CI release workflow (backlog § D) cross-
   compiles and attaches artifacts on tag `v*`.
3. **`install.sh`** — detects platform, downloads the matching release artifact, drops the
   `sandbox` binary in `~/.local/bin`. `cargo-dist` can generate both the release workflow and
   this script; evaluate it before hand-rolling.

**`cargo install --git` fallback:** even without crates.io, users can run
`cargo install --git https://github.com/johnnycarreiro/sandbox sandbox-cli` — it builds from the
repo with path deps intact and needs no published crates. Useful for early adopters before
Step 3.

## Step 5 — Tag and announce

- Tag `v0.1.0` on `dev`→`main` per Git Flow (release-tagged `main`).
- The release workflow fires on the tag, builds artifacts, creates the GitHub Release.
- Update `CHANGELOG.md` and the README install section.

## Prerequisites to call out to users

- **Docker** on `PATH` with the daemon running, and **`docker compose` v2** (the proxy and
  `--with-deps` invoke `docker compose`, not `docker-compose`).
- Linux is the v0.1 target; macOS/WSL2 are best-effort (see ADR-0012 note on Docker Desktop
  port publishing).
