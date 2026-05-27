# Software Architecture Description (SAD)

## Goals

- **Secure-by-default** isolated dev environments for untrusted code.
- **Composable**: new languages = new TOML, no code change.
- **Auditable**: every Docker action is loggable / printable; per-project state is on disk.
- **Simple**: shell out to `docker`/`docker compose` instead of speaking the API directly (ADR-0002).

## Entry point

- **Binary name:** `sandbox` (defined in `crates/sandbox-cli/Cargo.toml`).
- **Entry function:** `crates/sandbox-cli/src/main.rs::main` → `run()` → `match Command::<…>` → `commands::<name>::execute(Args)`.
- **No business logic in `main.rs`.** It owns clap definitions and the dispatch table only. Each `commands/<name>.rs` is a thin orchestrator that calls into the library crates below.

## Crate map

```
                       ┌──────────────────┐
                       │   sandbox-cli    │  bin: `sandbox`
                       │  main.rs +       │  argparse (clap), dispatch,
                       │  commands/*.rs   │  exit codes, logging, error display
                       └────────┬─────────┘
              ┌────────────┬────┴────┬────────────┐
              ▼            ▼         ▼            ▼
       ┌────────────┐ ┌─────────┐ ┌────────┐ ┌──────────┐
       │sandbox-core│ │ -docker │ │ -scan  │ │ -proxy   │
       └─────▲──────┘ └────┬────┘ └───┬────┘ └────┬─────┘
             │             │          │           │
             └─────────────┴──────────┴───────────┘
                  (the 3 adapters depend only on core)
```

`sandbox-core` is foundational — it depends on nothing in the workspace. The three adapter crates (`-docker`, `-scan`, `-proxy`) depend **only on core**, never on each other. `sandbox-cli` is the single place where all four meet.

## Module breakdown

What each crate actually contains (live as of Phase 5):

```
sandbox-cli/                          bin: argparse + dispatch
├── main.rs                           clap defs, dispatch table, tokio runtime
├── error.rs                          cli::Error (composes lib errors via #[from])
└── commands/
    ├── run.rs   down.rs   nuke.rs    lifecycle (Phase 1)
    ├── ps.rs    logs.rs   exec.rs    observability (Phase 3)
    ├── scan.rs                       standalone scan (Phase 4)
    ├── proxy.rs                      Traefik sidecar control (Phase 5)
    └── dotfiles.rs                   host dotfile discovery

sandbox-core/                         pure domain — no Docker, no network
├── paths.rs                          XDG resolution (Paths)
├── lang.rs                           LangManifest, LanguageRegistry, PortDetection
├── hash.rs                           ProjectHash, project_hash (path-based)
├── profile.rs                        Profile (safe/paranoid/unsafe + overrides)
├── config.rs                         Config (~/.config/sandbox/config.toml)
├── project.rs                        Project, ContainerName, NamedVolume
└── state.rs                          Meta (per-project state at $XDG_DATA/.../containers/<hash>/)

sandbox-docker/                       adapter: shell-out to the docker CLI
├── plan.rs                           Plan (pure data), Mount, NetworkSpec, SecuritySpec, …
├── lifecycle.rs                      run / start / exec / stop / rm
├── volume.rs                         named volumes (ensure + first-run chown)
├── network.rs                        ensure_internal + ensure_bridge
├── scanner.rs                        ephemeral container for ClamAV
└── cmd.rs                            wrapper over Command::new("docker")

sandbox-scan/                         adapter: multi-tier scanner
├── engine.rs                         orchestrates YARA + heuristics + compose + (optional) ClamAV
├── yara/                             yara-x + rules/contagious_interview.yar
├── heuristics/                       vscode, package_json, eval_patterns, network
├── compose/                          parse + rules (privileged, host ns, dangerous caps, host mounts)
├── clamav/                           clamscan output parser
├── cache.rs                          content-hashed cache, RULESET_VERSION-gated
├── suppress.rs                       IgnoreList (user-global)
├── findings.rs                       Finding, Severity
└── project_hash.rs                   content_hash (separate from core::ProjectHash)

sandbox-proxy/                        adapter: Traefik reverse proxy
├── traefik.rs                        render compose + static config; start/stop/status/logs
├── labels.rs                         Traefik label generation (port = service, ADR-0005)
├── ports/
│   ├── env.rs                        .env key reader
│   └── source.rs                     regex scan over source files
└── error.rs
```

## Dataflow: `sandbox run .`

Reflects the live Phase 5 code in `crates/sandbox-cli/src/commands/run.rs`. Steps marked *(planned)* belong to later phases and are kept here to flag where they will slot in.

```
┌──────────────────────────────────────────────────────────────────────┐
│ cli::commands::run::execute                                          │
└──┬───────────────────────────────────────────────────────────────────┘
   │ 1. parse args                                                     clap → Args
   │ 2. load profile + config                                          core::Profile, core::Config
   │ 3. resolve project path (canonical, dir check)                    core::Project::resolve
   │ 4. detect language (manifest match)                               core::LangManifest::detect
   │ 5. compute project hash (path-based, ADR-0009)                    core::hash::project_hash
   │ 6. load or initialize per-project state                           core::Meta::load
   │ 7. detect ports + generate proxy labels                           proxy::detect_ports + proxy::labels_for_project
   │ 8. build docker Plan (mounts, env, caps, labels, networks)        docker::Plan
   │ 9. if --print-cmd: print Plan and exit                            docker::Plan: Display
   │10. run pre-flight scan (unless --no-scan + --unsafe)              scan::scan → ScanReport
   │    └─ exit 30 on severity ≥ High
   │11. ensure sandbox-internal network exists                         docker::ensure_internal
   │12. if ports detected: ensure sandbox-proxy bridge exists          docker::ensure_bridge
   │13. ensure named volumes exist (+ first-run chown to host UID)     docker::ensure_volume_owned
   │14. dispatch lifecycle:                                            docker::lifecycle::run
   │    ├─ container missing → create → connect each extra net → start --interactive --attach
   │    ├─ container stopped → start → exec
   │    └─ container running → exec
   │15. *(planned, Phase 6)* validate + launch project compose deps    docker::compose::validate/up
   │16. *(planned)* persist updated Meta (port set, deps, last run)    core::Meta::save
└──────────────────────────────────────────────────────────────────────┘
```

## Key abstractions

### `core::Project`

```rust
pub struct Project {
    pub path: PathBuf,             // absolute, canonical (symlink-resolved)
    pub hash: ProjectHash,         // sha256(canonical_path) — see ADR-0009
    pub language: LanguageId,      // resolved via LangManifest::detect
    pub container_name: String,    // "sandbox-<hash.hex()[..12]>"
    pub volumes: Vec<NamedVolume>, // from language manifest's package_dirs
}
```

Note: container identity is workspace-stable (path-based), not content-sensitive.
Scan cache uses a separate content hash; see `sandbox-scan::ContentHash`.

### `core::Profile`

A Profile is a named bundle of safety flags. Profiles compose with CLI flags (CLI overrides profile).

### `core::LangManifest`

Loaded from `languages/*.toml`. Schema in [`languages/README.md`](../../languages/README.md). Hot-reloaded when the file changes (no rebuild).

### `core::Meta`

Per-project state on disk in `$XDG_DATA_HOME/sandbox/containers/<hash>/meta.toml`. Carries container name, source path, language, registered ports (Phase 5: `ports: Vec<u16>`, `#[serde(default)]` for additive evolution), and compose deps (Phase 6). `Meta::load_all()` is the union iterator used by `sandbox proxy start` to gather every project's ports.

### `docker::Plan`

A pure data structure describing a single `docker run` invocation. Constructed before execution. Inspectable with `--print-cmd`. ADR-0002 makes this the boundary between "intent" and "side effect".

### `scan::Findings`

```rust
pub struct Finding {
    pub severity: Severity,        // Info, Warn, High, Critical
    pub rule_id: String,
    pub path: PathBuf,
    pub line: Option<u32>,
    pub message: String,
    pub remediation: Option<String>,
}
```

`scan::scan(project) -> Findings` is deterministic: same source hash → same findings (modulo rule changes — bumping rule version invalidates cache).

## Security model

See [`threat-model.md`](threat-model.md) for in/out-of-scope. The architecture mirrors the model:

| Threat | Code path |
|---|---|
| T1 host RCE | `docker::Plan::default()` includes `--user`, `--cap-drop=ALL`, `--security-opt=no-new-privileges`, `--read-only` for `/app` |
| T2 volume persistence | `docker::Plan::source_mount` is `:ro` unless `Profile::unsafe` is set |
| T3 C2 callback | `docker::Plan::network` is `sandbox-internal` (no egress) unless `--network` |
| T4 host secrets | `docker::Plan::home_mount` is always `tmpfs`, never bind |
| T5 vector files in editor | `scan::heuristics::vscode_autorun` flags `.vscode/tasks.json` with autorun |
| T6 malicious compose | `scan::compose::validate` checks before `docker compose up` |
| T7 resource exhaustion | `docker::Plan::resource_limits` from profile |
| T8 source mutation | `scan::content_hash` recomputed each run; mismatch invalidates scan cache (separate from container ID hash) |

## Deployment

Single binary, installed via `cargo install --git`. Initial config seeded on first run from compiled-in defaults.

Languages and Traefik config are read from disk so users can iterate without rebuilding the binary.

## Future directions (non-goals for v1)

- Bollard-based async Docker (instead of shell-out) — see ADR-0002.
- LLM-assisted scan tier 3 — see ADR-0008.
- macOS support — currently Linux-first; Docker Desktop networking quirks need investigation.
- Devcontainer.json import (translate to our profile + manifest).
- VM-based isolation backend (Firecracker / krun) for hostile workloads.
