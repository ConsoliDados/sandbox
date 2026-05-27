# ADR-0010 — Project compose deps: opt-in `--with-deps`, three networks, glob discovery, egress mirrors profile

- **Status:** Accepted
- **Date:** 2026-05-16
- **Phase:** 6

## Context

Real projects rarely run as a single container. The frontend talks to a backend that talks to a Postgres on `:5432` and a Redis on `:6379`. Today the user typically maintains a `docker-compose.yml` to bring those siblings up. Phase 6 has to integrate that flow without weakening the threat model.

Constraints in play before this decision:

- **ADR-0004** sets the security baseline: containers join `sandbox-internal` (`--internal` flag, no egress) by default. Egress is opt-in via `--network` or runtime `sandbox net on`.
- **ADR-0005** puts inbound routing through Traefik on a separate `sandbox-proxy` bridge.
- **Threat-model T6** explicitly lists malicious `docker-compose.yml` (privileged, host namespaces, dangerous caps, host mounts) as in-scope. `sandbox-scan::compose::validate` already exists from Phase 4a and detects those.
- The user does **not** want us to mutate or generate override files inside the project tree — the project's compose file is user-owned and stays untouched.

The two threats specific to compose deps:

1. **Malicious image** — a postgres-looking service that actually exfiltrates on first start (and would do so during `docker compose up`, before any sandboxed code runs).
2. **Implicit egress widening** — naive integration would put the sandbox container on the compose's default network (egress-enabled bridge), silently neutralizing ADR-0004.

## Decision

We will integrate project compose deps **opt-in via `--with-deps`**, attach the sandbox container to **three networks** (internal + proxy + compose), discover compose files via **glob**, and make compose deps **inherit the sandbox's network policy** (no egress in safe; egress with `--network`).

Concretely:

1. **Opt-in flag, not auto.** `sandbox run` does **not** detect or start compose deps unless `--with-deps` is passed (or set in the active profile). Without the flag, the compose file is ignored entirely. Rationale: an arbitrary repo with a `docker-compose.yml` should never trigger `docker compose up` as a side effect of `sandbox run`.

2. **Three networks for the sandbox container.** When `--with-deps` is active, the sandbox container is attached to:
   - `sandbox-internal` (always; primary network, `--internal`)
   - `sandbox-proxy` (when ports are detected/exposed; from ADR-0005)
   - **the compose project network** (joined post-`up`)

   Same `docker create → docker network connect (×N) → docker start --attach` path that Phase 5 already added for proxy attach. No override file generated.

3. **Discovery via regex walk.** We walk the project root (depth-capped at 4, skipping `node_modules`, `target`, `.git`, `dist`, `build`, `.next`, `vendor`, `.venv`, `__pycache__`) and match basenames against `^(docker-compose|compose).*\.ya?ml$`. Covers `docker-compose.yml`, `docker-compose.yaml`, `docker-compose.dev.yml`, `compose.yml`, `compose.yaml`, `compose.dev.yml`, `services/docker-compose.yml`, etc. **Does not** match `production-compose.yml` or other names that merely contain "compose". An earlier draft listed `**/compose*.y{,a}ml` as the literal glob — that pattern is wrong (the basename `docker-compose.yml` doesn't start with `compose`), corrected in implementation. Multi-match: error with the list and require `--compose-file PATH` to disambiguate. Single match wins automatically. `--compose-file PATH` always overrides discovery; the override path is validated (must exist and be a regular file) and canonicalized before use.

4. **Mandatory scan before `up`.** `sandbox-scan::compose::validate` runs against the selected compose file before any `docker compose up`. Severity ≥ High blocks with exit code **31** (per SRS § Global). `--no-scan` (which already requires `--unsafe`) skips this, consistent with the rest of the scan pipeline.

5. **Compose deps inherit the sandbox's egress policy.** This is the non-obvious part. After `docker compose up -d`:
   - **Safe / paranoid (`--with-deps`, no `--network`):** every service in the compose project is **disconnected from the default compose network** and **reconnected to `sandbox-compose-<hash>`**, a Docker network we create with `--internal`. The sandbox container also joins this network. Result: deps can talk to the sandbox and to each other, but have no internet.
   - **With `--network` (or `--unsafe`, which implies `--network`):** deps stay on the compose-created network (regular bridge with egress). The sandbox joins it as the third network. Result: deps and sandbox both have internet.

   The point: a malicious postgres-looking image cannot phone home unless the user explicitly opted into network with `--network`. The flag controls **both** the sandbox's egress and the deps' egress in one motion — there is no asymmetry.

6. **State tracking in `Meta`.** Per-project `meta.toml` gains:
   ```toml
   [compose]
   file = "docker-compose.yml"          # path relative to project root
   project_name = "ctrading"            # what we passed to `docker compose -p`
   services = ["postgres", "redis"]     # what we brought up
   network = "sandbox-compose-66284ee5" # the internal network we created (safe mode)
   ```
   `sandbox down --with-deps` and `sandbox nuke` use this to call `docker compose -p <name> down` plus `docker network rm sandbox-compose-<hash>`. We only ever tear down what we ourselves started.

7. **Lifecycle ordering** (`sandbox run --with-deps`):
   1. Resolve project + profile + manifest (as today).
   2. Discover compose file via glob (or honor `--compose-file`).
   3. Pre-flight scan: source scan **and** compose validate. Block on either if severity ≥ High and no `--unsafe`.
   4. `docker compose -p sandbox-<short-hash>-deps -f <path> up -d` (the `-p` namespace keeps deps from colliding with whatever the user might have running by hand).
   5. If safe profile: create `sandbox-compose-<short-hash>` `--internal`; for each service, `docker network disconnect <auto-net> <container>` + `docker network connect sandbox-compose-<short-hash> <container>`; then `docker network rm <auto-net>`.
   6. Persist the `[compose]` block to `Meta`.
   7. Build the sandbox `Plan` with `additional_networks` including the compose network. Continue the existing Phase 5 attach path.

## Alternatives considered

- **(a) Auto-detect, no flag.** Reject. Running `docker compose up` on a freshly cloned untrusted repo is exactly the failure mode this project exists to prevent. The Lazarus image could have shipped a postgres-looking service; `--with-deps` makes that an explicit user decision.

- **(b) Marker file (`compose.sandbox.yml`) auto-opts-in.** Reject (despite earlier flirtation). Marker files inside the repo are spoofable in the same way ignore files were — a malicious repo would just commit one. Keeping the trust signal outside the repo (on the CLI flag) is cleaner.

- **(c) Sandbox joins only the compose network (no `sandbox-internal`).** Reject. It would mean the sandbox's egress posture depends on whatever the compose file declared, which the user generally hasn't audited. Three-network model keeps `sandbox-internal` authoritative.

- **(d) Generate a `docker-compose.sandbox-override.yml` to declare an external `--internal` network for the deps.** Reject. The user is explicit about not mutating files in the project tree, and override files create lifecycle/cleanup ambiguity (whose file is it? do we commit it?). The post-`up` network rewire achieves the same result without touching disk.

- **(e) Per-service egress control via manifest annotation.** Reject for v0.1. Tempting (e.g., "postgres needs no egress, but the API service needs to fetch tokens"), but it adds a configuration surface that the user has to maintain per project. The all-or-nothing rule (`--network` flips everything) maps cleanly to the existing trust model. Revisit if real usage reveals friction.

- **(f) Discovery only at project root.** Reject. Monorepos commonly nest the compose file under `infra/`, `docker/`, or `services/`. Glob covers those without surprising root-only projects (which still match `docker-compose.yml`).

## Consequences

Positive:

- The trust boundary remains a single CLI flag. `--with-deps` opts into running compose; `--network` opts into egress (for sandbox **and** deps). The user does not have to reason about each container's network independently.
- Compose deps cannot phone home in safe mode, even if the image is malicious. The validator (Phase 4a) covers the static surface; the network rewrite covers the runtime surface.
- No file is written to the project tree. The repo stays bit-identical after `sandbox run`.
- `sandbox down --with-deps` / `sandbox nuke` clean up only what `sandbox` started — the `Meta.[compose]` block is the source of truth, not heuristic name matching.
- Glob discovery covers monorepos out of the box; `--compose-file` is the escape hatch.

Negative / open:

- **Post-`up` network rewire is several Docker CLI calls per service.** Roughly O(2N+1) network ops per `sandbox run --with-deps` in safe mode (create, disconnect, connect, remove). Acceptable; deps come up once per session.
- **Compose deps cannot pull images at run time in safe mode.** If a referenced image isn't already in the local Docker cache when safe mode starts compose, `docker compose up` will fail because the deps' network gets `--internal` *after* `up`. We document the workaround: pre-pull (`docker compose pull` once with `--network`), then re-run safe. Future ergonomic improvement: detect missing images and prompt.
- **Deps can reach each other and the sandbox.** Intra-network reachability is the whole point — Postgres has to be reachable as `postgres:5432` from the sandbox. We accept the implicit "deps can reach sibling deps" as part of the model.
- **`sandbox-scan::compose` was written in Phase 4a but never wired to `run`.** Phase 6 will exercise it for the first time in the actual `up` path; expect to surface bugs that the standalone `sandbox scan` didn't catch.
- **Paranoid profile does not currently scan the dep images themselves.** Deferred to a future phase (image scanning via Trivy or similar) — out of scope here.

## References

- `../threat-model.md` T3 (egress C2), T6 (malicious compose)
- `../srs.md` § `run` (`--with-deps` flag), § `down` (`--with-deps` for cleanup), § Global (exit 31 for compose validation block)
- ADR-0004 (network isolation; `sandbox-internal` is primary)
- ADR-0005 (proxy network; same three-network attach pattern)
- ADR-0008 (scan pipeline; compose validator)
- ADR-0009 (container reuse; deps live as long as the sandbox container)
- `crates/sandbox-docker/src/network.rs` (`ensure_internal`, `ensure_bridge` — will gain `ensure_compose_internal`)
- `crates/sandbox-scan/src/compose/` (validator that gates `up`)
