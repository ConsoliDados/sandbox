# ADR-0005 — Reverse proxy via Traefik sidecar

- **Status:** Accepted
- **Date:** 2026-05-07
- **Phase:** 5

## Context

A real project rarely runs as a single port. The common shape is a frontend and a backend (e.g. Vite on `:3000` + a backend on `:5007`), often plus support services from `docker-compose` (Postgres on `:5432`, Redis on `:6379`, etc.). Two open requirements:

1. **Predictable hostnames** — the user should not have to remember which Docker bridge IP got assigned this morning. `localhost:3000` already works for single-port projects but breaks down when two projects compete for `:3000`.
2. **Multi-service support** — for monorepos like `project_name/{frontend, backend, …}` and for compose-managed siblings (db, cache), every exposed port should be reachable from the host browser.

We considered three exposure models during the design checkpoint of 2026-05-07:

- **(a) Subdomain-by-name** — `web.<proj>.sandbox.local`, `api.<proj>.sandbox.local`, etc. Requires `--expose PORT:NAME` syntax. Scales but each service needs a name.
- **(b) Path prefix** — `<proj>.sandbox.local/` and `<proj>.sandbox.local/api`. Single host but the backend has to accept the prefix; many frameworks need explicit `basePath` config or break.
- **(c) Host + port** — `<proj>.sandbox.local:3000`, `<proj>.sandbox.local:5007`, `<proj>.sandbox.local:5432`. One host per project; one entryPoint per port.

## Decision

**We will route all project services through Traefik using `<projname>.sandbox.local:PORT`.**

For each port detected (or explicitly exposed), the proxy generates a Traefik entryPoint that binds that port on the host and forwards to the corresponding container on the project's bridge network. The hostname stays constant; the port distinguishes services.

Detection sources, in order:

1. CLI override: `--expose 3000 5007 5432 ...`.
2. Heuristic regex from the language manifest (`port_detection.patterns`, `env_keys`).
3. Manifest `default_port` as a last resort.

Compose-managed siblings (db, redis, …) get their published ports auto-registered the same way.

## Alternatives considered

- **(a) Subdomain-by-name** — rejected: the user has to invent and remember a name per service. The `:PORT` model carries the same information that the dev already uses on `localhost:PORT` and matches mental models without configuration. (Sintaxe `--expose PORT:NAME` é descartada.)
- **(b) Path prefix** — rejected: too many backend frameworks fail under a forwarded prefix without explicit configuration; would either require per-framework rewrites in Traefik or surprise the user.
- **(d) Random host port allocation** — rejected: defeats predictable URLs; user would need to inspect `sandbox ps` after every restart.

## Consequences

Positive:

- Mental model is identical to plain `localhost:PORT` development.
- Monorepos and compose-managed siblings just work without additional flags.
- Cookies, CORS, and SameSite behave consistently — same host, different ports — matching what the dev already sees on `localhost`.
- Hostname is stable across restarts, even if the container's bridge IP rotates.

Negative / open:

- **Port conflicts between simultaneous projects.** Two projects both want `:3000`. Strategy: the first `sandbox run` host-binds the port; the second either picks a free neighbour or aborts with a clear message pointing at `--expose ALT_PORT`. Detail to be settled when the proxy crate is implemented (Phase 5).
- **Privileged ports (`<1024`)** require either `CAP_NET_BIND_SERVICE` on the proxy or a port shift. Out of scope for v0.1; users should expose `8080` instead of `80`.
- **Wildcard DNS** (`*.sandbox.local`) needs to resolve. We rely on `/etc/hosts` entries written by `sandbox proxy start`, or systemd-resolved / dnsmasq for users who prefer real wildcard resolution. Documented in playbook usage flow.
- **TLS** is out of scope for the MVP. Traefik can mint local certs (mkcert / step-ca) in a later phase.

## References

- `../srs.md` § `run` (the `--expose` flag); § `proxy` (the proxy subcommand)
- `../sad.md` (proxy crate boundary)
- `crates/sandbox-proxy/AGENTS.md`
- ADR-0009 (container reuse semantics — name stability)
