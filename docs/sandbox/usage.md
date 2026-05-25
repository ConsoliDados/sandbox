# Usage

Practical, command-by-command reference for the `sandbox` CLI. This is the
"how do I actually use it" companion to:

- [`usage-flow.md`](usage-flow.md) — the *trust dial* as a user story (when to relax which default).
- [`srs.md`](srs.md) — the formal CLI contract (every flag, the stable surface).
- [`threat-model.md`](threat-model.md) — what each default defends against.

If the network flags (`--network` vs `net`) ever confused you, read the next
section first — it is the single most common point of confusion.

---

## The network model

A sandbox container can be attached to up to **three** Docker networks, each
with a distinct job. Understanding these removes all the `--network`/`net`
ambiguity:

| Network | Role | Egress to internet? | When attached |
|---|---|---|---|
| `sandbox-internal` | **Primary.** Created with `--internal`. Every sandbox joins it. | **No** — blocked at the driver level | Always |
| `bridge` | Docker's default network. **This is what "internet" means.** | **Yes** | Only when you opt in |
| `sandbox-proxy` | Inbound routing via Traefik (exposing ports). Created `--internal`. | **No** — also `--internal` (ADR-0004) | When the project exposes a port |

Exposing a port does **not** give the sandbox internet. `sandbox-proxy` is
`--internal`; only Traefik is dual-homed onto an egress-capable bridge to
publish host ports and route inward. The sandbox itself stays egress-denied.

**The key idea:** "giving the sandbox internet" always means *attaching the
`bridge` network on top of `sandbox-internal`*. There are exactly two ways to
do that, and they are the same control at different times:

```
                          What it does                        When
  ────────────────────────────────────────────────────────────────────
  sandbox run . --network   attach bridge at container boot   boot-time
  sandbox run . --unsafe    implies --network (+ RW + no scan) boot-time
  ────────────────────────────────────────────────────────────────────
  sandbox net on .          attach bridge to a LIVE container  runtime
  sandbox net off .         detach bridge again                runtime
  sandbox net status .      show which networks are attached   runtime
```

- **`--network`** is the *boot-time* opt-in. The container comes up with egress already on.
- **`sandbox net on/off`** is the *runtime* toggle. You flip egress on a container that is already running, from the host, without recreating it.
- **`sandbox-internal`** is **not** something you toggle — it is the always-present, no-egress primary. It is *not* what `net` refers to.

**Default = no egress.** A plain `sandbox run .` joins `sandbox-internal` only.

**`run` is authoritative; the toggle is not sticky.** A `net on` *survives*
`sandbox down` (the bridge stays attached to the stopped container), but the
next `sandbox run` **reconciles egress to the profile** — so a default `run`
revokes a stale `net on` and you're back to no egress. `sandbox attach`, by
contrast, preserves whatever state the container is in (it's the "peek back in"
verb). If you want egress on every boot, use `--network` (or set
`network = true` in a profile) rather than relying on the runtime toggle.

```sh
sandbox run .              # isolated: no internet
# ... in another terminal, while it runs:
sandbox net status .       # egress: off  (sandbox-internal only)
sandbox net on .           # attach bridge → internet on, no restart
sandbox net off .          # detach bridge → isolated again
```

> `net off` refuses to disconnect `bridge` if it is the container's *only*
> network (exit 50, "would strand") — that only happens to `--unsafe` containers
> whose primary is bridge; use `sandbox down` to stop those instead.

---

## Container lifecycle

`sandbox run` does **not** run your shell as PID 1. The container's PID 1 is a
keepalive (`sleep infinity`); your interactive shell is layered on top via
`docker exec`. The consequence:

**Exiting the shell does not stop the container.** It keeps running until you
explicitly `sandbox down` (stop) or `sandbox nuke` (remove).

```
  sandbox run .        scan → create/resume → reconcile egress → shell
        │
     (exit)            shell ends; container STAYS running (PID 1 alive)
        │
  sandbox attach .     jump back into the shell — no scan, preserves state
  sandbox exec . -- X  run a single command without a shell
  sandbox net on .     toggle egress from the host while it runs
        │
  sandbox down .       stop the container, keep state + volumes
        │
  sandbox run .        wake a stopped container — RE-RUNS the scan AND
        │              re-enforces network policy (revokes a stale `net on`)
  sandbox nuke .       remove container + volumes + per-project state
```

`attach` vs `run` for re-entry: `attach` only works on a **running** container,
skips the scan, and leaves the network untouched (it is just "let me back in").
`run` re-scans and reconciles egress to the profile. Waking a **stopped** container
goes through `run`, which re-scans — by design, since the host source could
have changed.

---

## Commands

### `run` — start or resume a sandbox

```sh
sandbox run [PATH]            # PATH defaults to .
```

| Flag | Effect |
|---|---|
| `--lang NAME` | Force a language instead of auto-detecting from manifests |
| `--profile NAME` | Use a named profile from config (default: `default`) |
| `--network` | Attach `bridge` at boot — internet egress on. Source stays RO, scan still runs. |
| `--unsafe` | Trust dial fully open: RW source, internet on (implies `--network`), scan skipped |
| `--no-scan` | Skip the pre-flight scan. **Requires `--unsafe`** (else rejected). |
| `--with-clamav` | Add the ClamAV motor to the pre-flight scan (run `sandbox scan --update-db` once first) |
| `--expose PORT...` | Override port detection; each PORT becomes a Traefik entryPoint (repeatable) |
| `--with-deps` | Bring up the project's compose deps; they inherit the sandbox's egress policy |
| `--compose-file PATH` | Explicit compose file (overrides discovery; required on multi-match) |

Resume semantics: running → `exec` into it; stopped → `docker start` + exec;
missing → create. The scan runs on every `run` in safe/paranoid mode.

### `attach` — re-enter a running sandbox

```sh
sandbox attach [PATH]         # alias: sandbox shell
    [--lang NAME]
```

Drops you back into the same shell / workdir / host-user as `run`, via
`docker exec -it`, **without** the scan or any flight checks. The container must
already be **running**; a missing or stopped container exits 40 (pointing you at
`sandbox run`). `attach` never starts a stopped container.

### `exec` — run one command inside a running sandbox

```sh
sandbox exec [PATH] -- COMMAND [ARGS...]
    [--user USER]             # default: host uid:gid
    [--workdir PATH]          # default: /app
```

Use this for one-shot commands (`sandbox exec . -- npm install`). Container must
be running (exit 40 otherwise). The `--` separator is required before the command.

### `net` — toggle internet egress at runtime

```sh
sandbox net on  [PATH]        # attach bridge → egress on
sandbox net off [PATH]        # detach bridge → egress off
sandbox net status [PATH] [--format table|json]
```

Operates on a running container. Idempotent (`net on` twice = no-op + a notice).
See [The network model](#the-network-model) above. The toggle survives a
`sandbox down`, but the next `sandbox run` reconciles egress back to the
profile — use `sandbox attach` to re-enter without disturbing it.

### `down` (alias `stop`) — stop, keep state

```sh
sandbox down [PROJECT]        # alias: sandbox stop
    [--all]                   # stop every sandbox
    [--with-deps]             # also stop compose deps brought up by --with-deps
```

Stops the container but keeps named volumes and per-project state. The next
`sandbox run` resumes it (re-scanning first).

### `nuke` — remove everything

```sh
sandbox nuke [PROJECT]
    [--all]
    [--keep-volumes]          # remove container only
    [--keep-state]            # keep the state dir
    [-y | --yes]              # skip the confirmation prompt
```

Removes the container, named volumes, per-project state, and (if recorded) the
compose deps.

### `ps` / `logs` — observe

```sh
sandbox ps [--all] [--format table|json]      # --all includes stopped containers
sandbox logs PROJECT [-f] [--tail N] [--since DURATION]
```

`ps` lists running sandboxes by default; the `NETWORK` column reflects current
egress state. Because exiting a shell leaves the container running, a sandbox you
"left" still shows up in `ps` until you `down` it.

### `scan` — audit without launching

```sh
sandbox scan [PATH]
    [--explain]               # print message + remediation per finding
    [--format table|json]
    [--no-cache]              # bypass the scan cache
    [--with-clamav]           # add the ClamAV motor
    [--update-db]             # refresh the ClamAV signature DB and exit
```

Runs the same pipeline `run` uses (YARA + heuristics + compose; ClamAV opt-in),
but launches no container. Exits 30 when any finding is severity ≥ High; 20 when
`--with-clamav` is used but the signature DB volume is missing.

### `proxy` — Traefik reverse-proxy sidecar

```sh
sandbox proxy start [--dashboard]    # --dashboard exposes the Traefik API on :8090
sandbox proxy stop
sandbox proxy status
sandbox proxy logs [-f]
```

Projects that expose ports become reachable at
`<slug>.sandbox.localhost:<PORT>` once the proxy is up. `*.localhost` resolves to
loopback natively (RFC 6761) — no `/etc/hosts` edits needed.

### `lang` / `config` — *not yet implemented*

`sandbox lang ...` and `sandbox config ...` are scaffolded in the CLI but
currently return "not implemented" (planned polish). The CLI surface is
specified in [`srs.md`](srs.md#lang).

---

## Exit codes

As implemented in `cli::Error::exit_code` (the source of truth). The
[`srs.md`](srs.md#global) table lists the full *planned* set; conditions without
a dedicated code below currently exit `1`.

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | Generic failure (Docker daemon unreachable, invalid project/language, `--no-scan` without `--unsafe`, …) |
| 2 | Argument parsing error |
| 20 | ClamAV signature DB not initialized (run `sandbox scan --update-db`) |
| 30 | Scan blocked the run — findings at severity ≥ High (source or compose) |
| 40 | Container not found, or not running (for `attach` / `exec` / `net` / `logs`) |
| 50 | `sandbox net off` would strand the container (bridge is its only network) |

---

## Global flags

Available on every subcommand:

```
--print-cmd     Print the underlying docker command(s) instead of running them
-v, --verbose   Increase logging verbosity (repeat for more)
-q, --quiet     Suppress non-error output
--config PATH   Override config file location
```

`--print-cmd` is the auditability hatch: every Docker action can be previewed
without touching the daemon. Use it to see exactly what `run`/`attach`/`exec`
would invoke.

## See also

- [`usage-flow.md`](usage-flow.md) — when to step the trust dial
- [`smoke-tests.md`](smoke-tests.md) — copy-paste recipes that exercise each command
- [`srs.md`](srs.md) — the formal CLI contract
- ADR-0004 (network isolation), ADR-0009 (container reuse), ADR-0003 (volume strategy)
