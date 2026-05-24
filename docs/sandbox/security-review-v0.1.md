# Pre-release security review — v0.1.0

- **Date:** 2026-05-24
- **Scope:** the whole codebase as it stands ahead of the first distributable binary, with
  emphasis on the secure-by-default guarantees from `threat-model.md`.
- **Verdict:** **No CRITICAL or HIGH weakness open.** One MEDIUM finding (LAN exposure of
  proxy-published ports) is addressed by [ADR-0012](adrs/0012-localhost-port-binding.md) and
  scheduled in [release-v0.1-backlog.md](release-v0.1-backlog.md). Ship-readiness for the
  stated threat model is good.

This document is the audit trail for "was the security posture checked before the first
build?" It records what was verified, with evidence, so a future reviewer doesn't have to
re-derive it.

## Method

Read `threat-model.md`, then audited the implementation against it: Docker invocation and flag
assembly (`sandbox-docker`), the scan-bypass gate (`sandbox-scan` + CLI), command construction
(ADR-0002 compliance), untrusted-input flow into Docker/paths, and crash/DoS vectors
(`unwrap`/`panic`/`unsafe`). Line references are to the state reviewed on 2026-05-24.

## Findings

### MEDIUM — Proxy-published ports bound to all interfaces (addressed)

`crates/sandbox-proxy/src/traefik.rs::render_compose` published host ports as `"<port>:<port>"`
(≈ line 198) and the dashboard as `"8090:8090"` (≈ line 201). A bare mapping binds `0.0.0.0`,
exposing untrusted-code services — and the `api.insecure: true` Traefik dashboard — to the
whole LAN, even though the access model (`*.sandbox.localhost` → loopback) only needs
`127.0.0.1`.

- **Why it matters:** the tool's premise is running untrusted code; its inbound surface should
  not default to the local network.
- **Scope-limiting facts:** only the Traefik sidecar publishes host ports — project containers
  carry no `-p` flag (reached over the `sandbox-proxy` network). So the fix is one chokepoint.
  Exposure also only occurs when the user opted into the proxy (`--expose` / detected port).
- **Resolution:** bind to `127.0.0.1` by default with a `proxy.bind_address` opt-out — see
  [ADR-0012](adrs/0012-localhost-port-binding.md). Tracked in the v0.1 backlog.

### No other open weaknesses — defaults verified locked down

The secure-by-default posture holds. Evidence:

| Guarantee | Evidence | Status |
|---|---|---|
| Source mounted read-only in safe mode; RW only under `--unsafe` | `commands/run.rs` `read_only: !ctx.profile.unsafe_mode` | ✅ |
| Network isolated by default; egress opt-in via `--network`/`net on`/`--unsafe` | `commands/run.rs` `NetworkSpec::Internal(SANDBOX_INTERNAL)` else `Bridge` | ✅ |
| `cap_drop ALL` + `no-new-privileges` in **all** modes incl. `--unsafe` | `core/profile.rs` (`unsafe_profile` keeps `cap_drop = "ALL"`) | ✅ |
| Ephemeral tmpfs `$HOME`; no host secrets mounted | `core/profile.rs` `ephemeral_home: true`; only opt-in dotfiles, read-only | ✅ |
| CPU/memory limits applied | `docker/plan.rs:224-229` (`--cpus`/`--memory`); default profile 2 CPU / 4 GB | ✅ |
| Scan mandatory; `--no-scan` requires `--unsafe` | `commands/run.rs` `if args.no_scan && !args.unsafe_mode { return Err(NoScanRequiresUnsafe) }` | ✅ |
| Scan always runs in safe/paranoid; blocks on severity ≥ High | `commands/run.rs` `pre_flight_scan` (skips only on `--unsafe`/`--no-scan`) | ✅ |
| Compose validator blocks privileged / `network_mode: host` / dangerous host mounts / cap_add / non-allowlisted registries | `scan/src/compose/rules.rs` | ✅ |
| No `bash -c` / shell string concat — argv arrays only (ADR-0002) | `docker/src/*` use `Command::new("docker").args([...])`; grep for `bash -c`/`sh -c` → none | ✅ |
| Reverse proxy does **not** mount the docker socket | `proxy/src/traefik.rs` file provider (`providers.file`), no `docker.sock` (tested: `render_compose_does_not_mount_docker_socket`) | ✅ |
| No `unwrap()`/`expect()`/`panic!()`/`unsafe` in production code | workspace lints + grep; tests use `Result<_, Box<dyn Error>>` + `?` | ✅ |
| Untrusted input doesn't reach Docker/paths unescaped | image from trusted manifest; project/compose paths canonicalized; proxy slug sanitized to `[a-z0-9-]`; container name from source hash; env explicitly constructed | ✅ |

### Notes carried as known limitations (not blockers)

- **Trust boundaries are by design out of scope** (threat-model N1–N7): kernel/container escape,
  side channels, a compromised `sandbox` binary, compromised upstream images, host network
  attacks, and social engineering. No regression here.
- **Image supply chain** (signing, CVE scan, layer scan) is deferred to Phase 8 (OQ-008). The
  Phase 6 registry allowlist + planned digest pinning cover the highest-signal vectors for v0.1.
- **Exit codes 10 and 31** currently collapse into 1/30 (`cli/src/error.rs`). Cosmetic vs the
  SRS; tracked in the backlog, not a security issue.

## Conclusion

For the documented threat model, the implementation enforces its paranoid defaults and the scan
gate cannot be silently bypassed. The single MEDIUM finding has an accepted ADR and a backlog
item. **Clear to proceed to the first build** once the bind-localhost change and the rest of the
v0.1 backlog land.
