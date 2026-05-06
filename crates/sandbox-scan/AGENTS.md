# AGENTS.md — sandbox-scan

## Responsibility

Static security analysis of project sources and project compose files. Produces a `Findings` report. Does not decide whether to block the run — that's the CLI's job based on profile + severity.

## Boundaries

- **Owns:**
  - YARA rule loading and matching (Phase 4 — `yara-x` crate)
  - Heuristic regex patterns (`Function.constructor`, `runOn: "folderOpen"`, base64 domain decode patterns, etc.)
  - Compose file validation (privileged, host mounts, network_mode, capabilities)
  - Scan result cache at `$XDG_CACHE_HOME/sandbox/scan/<hash>.toml`
- **Does not own:** running containers, network ops, or LLM inference. LLM tier is deferred (ADR-0008).
- **Depends on:** `sandbox-core`, `regex`, `walkdir`, `serde`, `toml`. Plus `yara-x` from Phase 4.

## Layout (target shape — Phase 4+)

```
src/
├── lib.rs                  re-exports
├── error.rs
├── findings.rs             Finding, Severity, Findings
├── cache.rs                hash → cached Findings
├── engine.rs               orchestrator: cache check → yara → heuristics → compose
├── yara/
│   ├── mod.rs
│   └── rules/              compiled-in .yar files
├── heuristics/
│   ├── mod.rs
│   ├── vscode.rs           tasks.json autorun, devcontainer postCreate, etc.
│   ├── package_json.rs     pre/post install scripts inspection
│   ├── eval_patterns.rs    Function.constructor, eval, atob, etc.
│   └── network.rs          base64 domain decode pattern, suspicious URLs
└── compose/
    ├── mod.rs
    ├── parse.rs            subset of compose spec we validate
    └── rules.rs            allowlist (privileged: false, no network_mode: host, ...)
```

Today (Phase 0): `lib.rs` only.

## Conventions

- **Findings are deterministic given (sources_hash, ruleset_version).** Cache hit returns the same result.
- **Severity levels:** `Info`, `Warn`, `High`, `Critical`. Block thresholds set by profile.
- **Every finding has a `remediation` field** when feasible — a short string suggesting what to do (`add to ~/.config/sandbox/scan-ignore.toml` / `run with --unsafe` / `remove file X`).
- **Rules are versioned.** Bumping any rule increments `ruleset_version`, which invalidates all cache entries.
- **No false-negative tolerance.** Default: missing a real threat is worse than a false positive. Tune later.
- **Heuristics live in dedicated files**, one per category. Test with positive and negative fixtures.

## IoCs we ship by default (seeded from incident-2026-05-06)

- `chainlink-api-v3.live` C2 domain
- `Function.constructor` eval pattern in JS
- `.vscode/tasks.json` with `runOn: "folderOpen"` + `node <file>` command
- `Buffer.from(<base64>, 'base64')` followed by network call within N lines
- npm package names from public Lazarus campaign reports

Full list maintained in `src/yara/rules/contagious_interview.yar` and `src/heuristics/`.

## Commands

```sh
cargo test -p sandbox-scan
cargo run -p sandbox-cli -- scan ~/some/project --explain
```

## Points of attention

- `yara-x` is pure Rust (no libyara dependency). Rules are compiled at startup; bench latency once we have real-world rule count.
- The compose validator must keep up with new compose features. Pin a compose schema version and document supported subset.
- The cache file is **trusted** (it's user-owned in `~/.cache`). If a malicious project plants a file at that path before its first scan, we'd accept it. Mitigate: cache key includes ruleset version + a salt; corrupted files fail to deserialize.
