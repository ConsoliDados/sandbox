# Software Architecture Description (SAD)

## Goals

- **Secure-by-default** isolated dev environments for untrusted code.
- **Composable**: new languages = new TOML, no code change.
- **Auditable**: every Docker action is loggable / printable; per-project state is on disk.
- **Simple**: shell out to `docker`/`docker compose` instead of speaking the API directly (ADR-0002).

## Crate map

```
                          ┌─────────────────────┐
                          │   sandbox-cli (bin) │  argparse, dispatch
                          └─────────┬───────────┘
                                    │ uses
              ┌─────────────────────┼─────────────────────┐
              ▼                     ▼                     ▼
   ┌─────────────────┐   ┌───────────────────┐   ┌───────────────────┐
   │  sandbox-core   │   │  sandbox-docker   │   │   sandbox-scan    │
   │                 │   │                   │   │                   │
   │ - Project       │   │ - Compose plan    │   │ - YARA engine     │
   │ - Profile       │   │ - run/exec/stop   │   │ - Heuristic regex │
   │ - LangManifest  │   │ - Network ops     │   │ - Cache (toml)    │
   │ - State store   │   │ - Volume mgmt     │   │ - Compose validator│
   │ - Hash          │   │                   │   │                   │
   └─────────────────┘   └───────────────────┘   └───────────────────┘
              ▲                                           ▲
              │ uses                                      │ uses
              │                                           │
              │             ┌───────────────────┐         │
              └─────────────│  sandbox-proxy    │─────────┘
                            │                   │
                            │ - Traefik labels  │
                            │ - Sidecar mgmt    │
                            │ - Port detection  │
                            └───────────────────┘
```

`sandbox-core` is foundational — no other crate depends on adapters; adapters depend on core.

## Dataflow: `sandbox run .`

```
┌──────────────────────────────────────────────────────────────────────┐
│ cli::commands::run                                                   │
└──┬───────────────────────────────────────────────────────────────────┘
   │ 1. parse args                                                     core::Args
   │ 2. load config + profile                                          core::Config
   │ 3. resolve project path                                           core::Project::resolve
   │ 4. detect language (manifest match)                               core::LangManifest::detect
   │ 5. compute project hash                                           core::hash::project_hash
   │ 6. load or initialize per-project state                           core::State::load
   │ 7. run scan (unless --unsafe and profile permits)                 scan::scan
   │    └─ block on findings or print + abort                          scan::Findings
   │ 8. detect & validate project compose                              docker::compose::validate
   │    └─ block on bad config or auto-launch deps                     docker::compose::up
   │ 9. ensure named volumes exist                                     docker::volumes::ensure
   │10. ensure network exists (sandbox-internal by default)            docker::network::ensure
   │11. ensure proxy is running (if required by profile)               proxy::ensure_running
   │12. compute final docker run plan (mounts, env, caps, ports)       docker::Plan
   │13. either docker exec (existing container) or docker run          docker::run_or_attach
   │14. save state (container id, ports, deps)                         core::State::save
   │15. attach shell                                                   docker::attach
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

### `core::State`

Per-project state on disk in `$XDG_DATA_HOME/sandbox/containers/<hash>/`. Append-only metadata + log dir.

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
