# Language manifests

Adding a stack to `sandbox` is **just dropping a TOML file** in this directory (or in `~/.config/sandbox/languages/` for user-specific overrides). No code change.

## Schema

```toml
# Required
name           = "snake_case"           # unique identifier
display_name   = "Human Readable"       # shown in CLI output
image          = "registry/image:tag"   # base Docker image (pin a digest if you can)
detect         = ["file1", "file2"]     # presence of any → match

# Optional
priority       = 0                       # tie-breaker on multi-match (higher wins; default 0)
package_dirs   = ["node_modules"]        # writable named volumes; default []
default_port   = 3000                    # fallback when port detection finds nothing
lock_files     = ["package-lock.json", "pnpm-lock.yaml"]  # safe/paranoid bind-mount each from state-dir
primary_lock_file = "package-lock.json"  # which to auto-seed on first run if none present on host
extra_packages = ["zsh", "git", "starship"]  # apt/apk packages to install on top
shell          = "/bin/bash"             # default shell (switch to zsh once custom-image pipeline lands)
workdir        = "/app"                  # default; rarely overridden

# Optional: source scan patterns the proxy uses to find listening ports.
# If omitted, falls back to global patterns from sandbox-proxy.
[port_detection]
patterns = [
    'app\.listen\((\d+)',
    '\.listen\(\s*(\d+)',
]
env_keys = ["PORT", "APP_PORT", "HTTP_PORT"]
```

## Built-in manifests

- `node.toml` — Node.js (`package.json`)
- `bun.toml` — Bun (`bun.lockb`, `bun.lock`, `bunfig.toml`)
- `rust.toml` — Rust (`Cargo.toml`)

## Adding a stack

1. Write `mystack.toml` here (or in `~/.config/sandbox/languages/`).
2. Validate: `sandbox lang validate ./mystack.toml`.
3. Install (if user-specific): `sandbox lang add ./mystack.toml`.
4. Use: `sandbox run /path/to/project --lang mystack`.

## Conflict resolution (multi-match)

If a project matches multiple manifests (e.g. Tauri has both `Cargo.toml` and `package.json`), tie-breaking is decided by:

1. Higher `priority` wins.
2. If equal, the more-specific match wins (more files matched from `detect`).
3. If still equal, error and require `--lang`.

See `docs/open-questions.md` § OQ-005.

## Future: YAML support

The schema will be the same. Loader will accept `.toml` or `.yaml` based on extension. Tracked in roadmap (post-v0.1).
