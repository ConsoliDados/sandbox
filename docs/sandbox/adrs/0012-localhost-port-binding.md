# ADR-0012 — Reverse-proxy published ports bind to loopback (`127.0.0.1`) by default

- **Status:** Accepted
- **Date:** 2026-05-24
- **Phase:** 7 (pre-v0.1 hardening)

## Context

The whole point of `sandbox` is to run **untrusted** code under isolation. ADR-0005 routes
inbound HTTP to project containers through a Traefik sidecar: each registered project port
becomes a Traefik entryPoint, and Traefik **publishes that port on the host** via the compose
`ports:` mapping it generates in `crates/sandbox-proxy/src/traefik.rs::render_compose`.

Today that mapping is written as:

```yaml
ports:
  - "3000:3000"          # render_compose, ~line 198
  - "8090:8090"          # dashboard, when --dashboard, ~line 201
```

A bare `"<port>:<port>"` mapping tells Docker to publish on `0.0.0.0` — i.e. **every host
interface**, including the LAN/Wi-Fi address. The consequence is that a service started by
untrusted code inside the sandbox is reachable from any other machine on the local network,
not just from the operator's own browser. The Traefik dashboard is worse: it runs with
`api.insecure: true` (read access to the full routing table), and at `0.0.0.0:8090` that is
exposed to the LAN too.

This contradicts the threat model. The intended access path is already loopback-only: the dev
domain is `*.sandbox.localhost`, which resolves to `127.0.0.1` per RFC 6761 (ADR-0005). So
publishing on `0.0.0.0` is strictly *broader* than the access model requires — nobody is
supposed to reach these services by LAN IP in the first place.

Relevant facts established during the pre-release review:

- **The Traefik sidecar is the only thing that publishes host ports.** Project containers do
  **not** get a `-p`/`--publish` flag; they are reached over the `sandbox-proxy` network by
  Traefik (verified in `crates/sandbox-cli/src/commands/run.rs` and
  `crates/sandbox-docker/src/plan.rs` — no publish path). So loopback binding only has to be
  fixed in one place: `render_compose`.
- This is a change to a **security-relevant default**, which per `AGENTS.md` requires an ADR
  and a threat-model update — hence this record.

## Decision

We will **bind every host port the proxy publishes to `127.0.0.1` by default** — both project
entryPoints and the dashboard — and expose an explicit opt-out via config.

Concretely:

1. `render_compose` emits `"127.0.0.1:<port>:<port>"` instead of `"<port>:<port>"`, for both
   the per-project entryPoints and the `--dashboard` port.
2. A new config key `proxy.bind_address` (default `"127.0.0.1"`) controls the host interface.
   `ProxyConfig` carries it through from the CLI; setting it to `"0.0.0.0"` restores the old
   LAN-exposed behavior for operators who knowingly want remote access.
3. The default is loopback. Widening to all interfaces is an explicit, documented opt-out — the
   same shape as `--network`/`--unsafe`: secure by default, relax on purpose.

## Alternatives considered

- **(a) Keep `0.0.0.0`.** Reject. Publishing untrusted-code services to the whole LAN by
  default is exactly the kind of paranoid-default violation this project exists to avoid. The
  insecure Traefik dashboard on a routable interface makes it worse.
- **(b) Bind only the dashboard to loopback, leave project ports on `0.0.0.0`.** Reject.
  Inconsistent, and the project ports are the ones actually serving untrusted code. The
  dashboard is read-only metadata; the app ports are the real exposure.
- **(c) Solve it with a host firewall rule.** Reject. Not portable (nftables/ufw/firewalld
  differ), requires privileges we don't take, and silently depends on host state we don't
  control. Binding at publish time is declarative and lives with the proxy config we already
  generate.
- **(d) Per-project bind address.** Reject for v0.1. A single host-wide `proxy.bind_address`
  matches the proxy's singleton-per-host model (ADR-0005). Revisit only if real usage needs
  per-project remote exposure.

## Consequences

Positive:

- Untrusted-code services and the Traefik dashboard are unreachable from the LAN by default.
  The access model (loopback via `*.sandbox.localhost`) and the bind address now agree.
- One-line conceptual change at a single chokepoint (`render_compose`); no change to project
  container launch.
- Opt-out is explicit and documented, consistent with the rest of the trust model.

Negative / follow-up:

- **Remote access needs the opt-out.** Operators who reach the dev server from another device
  (phone on the same Wi-Fi, a second machine) must set `proxy.bind_address = "0.0.0.0"`. We
  document this in `usage.md`.
- **Docker Desktop / WSL2 nuance.** On Docker Desktop (macOS) and some WSL2 setups, published
  ports are reached through the VM's networking; `127.0.0.1` from the host still works for the
  loopback case, but anyone relying on cross-VM access should validate. Noted for the
  smoke-test pass.
- Existing `render_compose` unit tests that assert `"3000:3000"` must be updated to expect the
  loopback-prefixed form.

## References

- `../threat-model.md` — inbound exposure via the reverse proxy (new note); T1/T3 posture
- ADR-0004 (network isolation; egress is opt-in) — this ADR is the inbound analogue
- ADR-0005 (Traefik sidecar; `*.sandbox.localhost` → loopback per RFC 6761)
- `crates/sandbox-proxy/src/traefik.rs` (`render_compose` — the single publish chokepoint;
  `DASHBOARD_PORT`, `ProxyConfig`)
- Implementation tracked in `../release-v0.1-backlog.md`
