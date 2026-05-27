# AGENTS.md — sandbox-proxy

## Responsibility

Manages the **Traefik reverse proxy sidecar** and generates Traefik labels for project containers, so users can reach `http://<projname>.sandbox.localhost` from the host browser.

## Boundaries

- **Owns:**
  - Traefik compose file generation at `$XDG_DATA_HOME/sandbox/proxy/`
  - Lifecycle: `start`, `stop`, `status`, `logs`
  - Label generation for project containers (rule, service, port)
  - Port detection: parses `.env`, regex over source for `app.listen(N)`, `PORT=`, `bind = "0.0.0.0:N"`, etc.
- **Does not own:** running the project containers (that's `sandbox-docker`). It only emits labels that `sandbox-docker` applies to the `docker run` command.
- **Depends on:** `sandbox-core`, `tokio`, `regex`. Calls out to `docker` to start/stop the Traefik sidecar.

## Layout (target shape — Phase 5+)

```
src/
├── lib.rs                  re-exports
├── error.rs
├── traefik.rs              compose template, render, lifecycle
├── labels.rs               Traefik label generation
└── ports/
    ├── mod.rs              Port, PortMap
    ├── env.rs              .env parser
    └── source.rs           regex scanners per language
```

Today (Phase 0): `lib.rs` only.

## Conventions

- **The proxy is a singleton** per host. Only one Traefik sidecar at a time.
- **Default domain is `sandbox.localhost`.** Override via config. User must add `*.sandbox.localhost 127.0.0.1` to `/etc/hosts` or dnsmasq once.
- **Project containers join `sandbox-proxy` network** in addition to whatever else; the proxy joins the same network and routes by Host header.
- **Auto-detection is best-effort.** If we can't find a port, fall back to manifest's `default_port_hint`. Always emit warning. Manual override via `--expose PORT[:NAME]`.
- **Port detection runs once per project hash** and caches in state. Re-run on `--rebuild` or hash change.

## Source scanning patterns (per language)

- **node/bun:** `app\.listen\((\d+)`, `\.listen\(\s*(\d+)`, `process\.env\.PORT`, `PORT=`, `server\.listen\((\d+)`
- **rust:** `bind\(\s*"0\.0\.0\.0:(\d+)"`, `actix_web::HttpServer::new\(.+\)\.bind\("[^:]*:(\d+)"\)`, `axum::Server::bind\(&"\d+\.\d+\.\d+\.\d+:(\d+)"`
- **`.env`:** key matches `^(PORT|APP_PORT|HTTP_PORT|SERVER_PORT|API_PORT|FRONTEND_PORT)=`

Patterns are heuristics; users can override.

## Commands

```sh
cargo test -p sandbox-proxy
sandbox proxy start
sandbox proxy status
```

## Points of attention

- Traefik logs verbose by default. Use file rotation for the sidecar's logs in `$XDG_DATA_HOME/sandbox/proxy/logs/`.
- Some users may already run their own Traefik on port 80. Detect the port collision and surface a clear error (`port 80 in use; configure proxy.port in config.toml`).
- TLS is out of scope for v0.1 — we serve HTTP only. The dev domain `sandbox.localhost` is fine for HTTP; a future ADR can add mkcert + HTTPS.
