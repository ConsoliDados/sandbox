# ADR-0004 — Network isolation by default (`sandbox-internal`); runtime toggle

- **Status:** Accepted
- **Date:** 2026-05-08
- **Phase:** 2

## Context

Egress from a sandboxed container is the dominant exfiltration channel for the threats this project exists to contain (threat-model T1, T3, T5). The 2026-05-06 Lazarus incident relied on outbound HTTPS to a C2 host. A default that allows arbitrary egress turns every malicious postinstall script into a credential-harvesting payload.

Docker's defaults are unhelpful here: a plain `docker run` joins the `bridge` network, which has internet egress through the host's default route. To deny egress we have to opt in.

The constraints:

1. **Default must deny egress.** Same posture as the source mount: paranoid first, opt out explicitly.
2. **Need runtime toggling.** A real workflow looks like *scan offline → install with internet → keep running offline*. Recreating the container each time we toggle is both slow and would break interactive sessions. Docker supports `network connect/disconnect` against running containers; we should use it (Phase 6 — `sandbox net on/off`).
3. **The first network is fixed at `docker run` time.** Whatever we attach at create time is the container's primary network for the life of the container.

## Decision

**`sandbox` creates and reuses a single Docker network named `sandbox-internal`, declared with `--internal`. Every container starts attached to that network as its primary. Egress is enabled by additionally attaching a `bridge` network — at create time via `--network` / `--unsafe`, or at runtime via `sandbox net on` (Phase 6).**

Concretely:

- The network name and `--internal` flag are constants in `sandbox-docker::network`. We never create unnamed/anonymous networks.
- `ensure_internal()` is idempotent and called from `sandbox-cli::commands::run` before every `docker run` (cheap; one `docker network ls`/`create` call). On first use of the tool on a host the network is created automatically; on subsequent runs it's reused.
- `Plan.network` is `NetworkSpec::Internal("sandbox-internal")` for the default profile and `NetworkSpec::Bridge` when the user passes `--network` (allow internet) or `--unsafe` (which implies `--network`). Profile defaults set `network = false`; CLI flags can flip it on but cannot turn it off if the profile already declared `true`.
- Network state is per-host, not per-project. Multiple projects share `sandbox-internal`. They cannot reach each other through it because `--internal` blocks all inter-container traffic that isn't explicitly enabled (each container is on its own subnet via Docker's default IPAM but cannot reach external hosts; intra-network reachability between sandboxed projects is acceptable since they are sibling containers, not external attackers).
- Future Phase 6 work: `sandbox net on PROJECT` calls `docker network connect bridge <container>`; `sandbox net off PROJECT` disconnects bridge again. The `sandbox-internal` attachment never changes — that's the primary network and always present.

## Alternatives considered

- **(a) `--network=none`.** Rejected: this is "no networking at all," not "no egress." Containers couldn't talk to the project's compose deps (Phase 6) or to a Traefik sidecar (ADR-0005). Internal isolation needs intra-network connectivity to be useful.
- **(b) `--network=host`.** Rejected outright: shares the host's network namespace. Worse than the default `bridge` — the container can hit `localhost` services on the host, including the Docker socket if exposed. Direct contradiction of the threat model.
- **(c) Custom bridge with iptables drop rules.** Considered. We'd create our own network and install firewall rules on the host to block egress. Rejected: adds a host-level dependency (root iptables manipulation), platform-specific (different on Linux distros, different again on macOS where iptables doesn't apply), and Docker's `--internal` flag already does exactly what we need. Reinventing it would only be justified if `--internal` had a known security gap, which it doesn't for our threat model.
- **(d) Per-project network.** Considered. Each project gets its own `sandbox-<hash>` network. Rejected for v0.1: increases lifecycle bookkeeping, and the value (full isolation between sandboxed projects) is small relative to the threat model — the threats here are external attackers reaching the host or the internet, not one sandboxed project attacking another. We can revisit if multi-project compose flows in Phase 6 reveal a real isolation gap.
- **(e) Don't pre-create the network; let `docker run` fail and prompt the user.** Rejected: terrible UX. `ensure_internal` is one round-trip; the cost is invisible and the failure mode is opaque.

## Consequences

Positive:

- A `sandbox run .` on a fresh host has no internet egress without any user action — that's the entire point.
- `--network` is the single, obvious lever for enabling egress, and it composes cleanly with `--unsafe` (which always wants egress).
- Phase 6 runtime toggle (`sandbox net on/off`) is a simple `docker network connect/disconnect` against a long-lived container, not a recreation. No data lost on toggle.
- Network state lives in Docker (not in our state dir). `docker network rm sandbox-internal` is the recovery path; `ensure_internal` recreates it.

Negative:

- **`--internal` blocks DNS lookups against external resolvers**, including some forms of `apt-get update` if a base image relies on it. In practice we don't run package installs in default mode — those happen in `unsafe`/`--network` runs. But if a Dockerfile build inside the sandbox tries to fetch dependencies, it will fail unless the user passes `--network`.
- **Sibling sandboxed projects share `sandbox-internal`** and can in principle reach each other on the internal network. We accept this for v0.1; Phase 6 may revisit per-project networks if compose deps make this an actual attack surface.
- **Network is host-global.** A user who renames or removes `sandbox-internal` outside our control will see runs fail until `ensure_internal` recreates it. Acceptable: the network name is documented, the recovery is automatic, and we don't expect users to manage Docker networks by hand.

## References

- `../threat-model.md` T1 (egress C2), T3 (data exfiltration), T5 (credential theft)
- `../srs.md` § `run` (`--unsafe`, `--network`), `net` (Phase 6)
- ADR-0009 (container reuse — runtime toggle relies on long-lived containers)
- ADR-0010 (compose deps — sibling network for project services)
- `crates/sandbox-docker/src/network.rs` (`SANDBOX_INTERNAL`, `ensure_internal`)
