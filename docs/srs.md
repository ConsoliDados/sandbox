# Software Requirements Specification (SRS)

The CLI surface. Stable contract — changes require an ADR.

## Global

```
sandbox <SUBCOMMAND> [SUBCOMMAND_FLAGS]
       [--config PATH]    Override config file location
       [--verbose | -v]   Increase logging verbosity (repeat for more)
       [--quiet | -q]     Suppress non-error output
       [--print-cmd]      Print the underlying docker commands instead of running them
       [--no-color]       Disable ANSI colors
       [--help | -h]
       [--version]
```

Exit codes:

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | Generic failure |
| 2 | Argument parsing error |
| 10 | Project not found / cannot detect language |
| 20 | Docker not available / daemon not running |
| 30 | Scan blocked the run (default mode) |
| 31 | Compose validation failed (default mode) |
| 40 | Container not found (for ops on existing project) |
| 50 | Network operation failed |

## Subcommands

### `run`

Start (or resume) a sandbox for a project.

```
sandbox run [PATH]
    [--lang NAME]         Force a language (default: auto-detect)
    [--profile NAME]      Use a named profile from config (default: "default")
    [--unsafe]            Disable all paranoid defaults: r/w volume, full network, skip scan
    [--network]           Allow internet egress (otherwise no egress)
    [--no-scan]           Skip pre-flight scan (requires --unsafe)
    [--no-cache]          Force re-scan even if cache hit
    [--expose SPEC ...]   Map a port to Traefik. SPEC = PORT or PORT:NAME (e.g. 3000 or 3000:web)
    [--shell zsh|bash]    Shell to launch (default: zsh)
    [--rebuild]           Force rebuild of the container image
```

`PATH` defaults to `.`.

Behavior:
1. Resolve `PATH` to absolute, error if not a directory.
2. Detect language via manifest files (`languages/*.toml::detect`).
3. Compute project hash (sha256 of `git ls-files` + `git status --porcelain`, fallback to walkdir if not a git repo).
4. Look up container by name `sandbox-<hash[..12]>`:
   - Running → `docker exec` shell into it.
   - Stopped → `docker start` then exec.
   - Missing → create.
5. (Default mode only) Run pre-flight scan. Block on findings unless `--unsafe`.
6. (If project has compose) Validate + start dependency services.
7. Create/reuse named volumes for `package_dirs` from manifest.
8. Apply network policy (`sandbox-internal` by default; `bridge` if `--network`).
9. Launch container with all hardening flags.
10. Enter shell or print attach hint.

### `down`

Stop running container; keep state and named volumes.

```
sandbox down [PROJECT]
    [--all]               Stop every sandbox container
    [--with-deps]         Also stop project compose deps
```

`PROJECT` accepts: project name, hash prefix (≥4 chars), or `.` (resolve from cwd). If omitted and `--all` not set, defaults to cwd.

### `nuke`

Remove container, named volumes, and per-project state.

```
sandbox nuke [PROJECT]
    [--all]
    [--keep-volumes]      Remove container only; keep named volumes
    [--keep-state]        Remove container + volumes; keep state dir
    [--yes | -y]          Skip confirmation
```

### `ps`

List sandboxes.

```
sandbox ps
    [--all]               Include stopped containers
    [--format json|table] (default: table)
```

Columns: `NAME | HASH | LANG | PATH | STATUS | NETWORK | UPTIME | DEPS`.

### `logs`

Tail container logs.

```
sandbox logs PROJECT
    [--follow | -f]
    [--tail N]            (default: 200)
    [--since DURATION]    e.g. 5m, 1h
```

### `exec`

Run a command inside the running sandbox.

```
sandbox exec PROJECT -- COMMAND [ARGS...]
    [--user USER]         (default: container's default non-root user)
    [--workdir PATH]      (default: /app)
```

If container is not running, errors with code 40 (suggest `sandbox run` first).

### `net`

Toggle internet egress at runtime.

```
sandbox net on PROJECT      Connect container to bridge network
sandbox net off PROJECT     Disconnect from bridge network
sandbox net status PROJECT  Show current network membership
```

### `scan`

Standalone security scan (no container launched).

```
sandbox scan [PATH]
    [--no-cache]          Force re-scan
    [--explain]           Show details for each finding
    [--format text|json|sarif] (default: text)
    [--severity LEVEL]    Minimum severity to report: info|warn|high|critical (default: warn)
```

Exit code 0 if clean (or only `info` findings), 30 if blocking findings.

### `lang`

Manage language manifests.

```
sandbox lang list                      List all available languages
sandbox lang show NAME                 Print a language manifest
sandbox lang add FILE                  Install a manifest into ~/.config/sandbox/languages/
sandbox lang validate FILE             Lint a manifest without installing
```

### `proxy`

Control the Traefik reverse proxy sidecar.

```
sandbox proxy start [--port 80] [--dashboard]
sandbox proxy stop
sandbox proxy status
sandbox proxy logs [--follow]
```

### `config`

```
sandbox config edit                    Open ~/.config/sandbox/config.toml in $EDITOR
sandbox config show                    Print effective config (defaults + user overrides)
sandbox config path                    Print the config file path
```

## Project resolution rules

`PROJECT` argument resolution (in order of precedence):

1. Exact container name (e.g. `sandbox-a1b2c3d4e5f6`).
2. Hash prefix (≥4 chars).
3. Project alias from state (e.g. `ctrading` if registered).
4. `.` → resolve from current working directory.

If multiple matches → error with disambiguation hint.

## State directory layout (XDG)

```
$XDG_CONFIG_HOME/sandbox/        (default: ~/.config/sandbox/)
├── config.toml                  Global config + profile definitions
├── languages/                   User-overridable language manifests
└── zsh/.zshrc.sandbox           Optional sandbox-specific zshrc

$XDG_DATA_HOME/sandbox/          (default: ~/.local/share/sandbox/)
├── containers/<hash>/
│   ├── meta.toml                Container name, lang, source path, ports
│   ├── volumes.toml             Named volume names + sizes
│   ├── compose-deps.toml        Project compose services we manage
│   └── logs/                    Run logs (rotated)
└── proxy/
    └── traefik.yaml             Generated Traefik config

$XDG_CACHE_HOME/sandbox/         (default: ~/.cache/sandbox/)
└── scan/<hash>.toml             Cached scan result keyed by source hash
```

## Configuration

`~/.config/sandbox/config.toml` schema (also defines profiles):

```toml
[defaults]
shell = "zsh"
language_dirs = ["~/.config/sandbox/languages"]   # in addition to bundled
profile = "default"

[scan]
cache = true
severity_threshold = "warn"

[proxy]
domain = "sandbox.local"
auto_start = true

[profile.default]
unsafe = false
network = false
ephemeral_home = true
cap_drop = "ALL"
no_new_privileges = true
cpu = 2.0
memory_mb = 4096

[profile.unsafe]
unsafe = true
network = true

[profile.paranoid]
unsafe = false
network = false
ephemeral_home = true
cap_drop = "ALL"
no_new_privileges = true
cpu = 1.0
memory_mb = 2048
no_compose_deps = true
```
