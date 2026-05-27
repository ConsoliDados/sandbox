//! Docker network operations. The default sandbox network is `--internal`,
//! meaning containers connected to it have no internet egress. Per-project
//! egress toggling (`sandbox net on`) attaches the bridge network on top.

use crate::cmd::{run_capture, run_probe};
use crate::{Error, Result};

pub const SANDBOX_INTERNAL: &str = "sandbox-internal";

/// Docker's default `bridge` network. Attached by `sandbox net on` to grant
/// internet egress at runtime; detached by `sandbox net off`. See ADR-0004.
pub const BRIDGE: &str = "bridge";

/// Ensure `name` exists and is an `--internal` network (no internet egress).
///
/// If a network by that name already exists but is **not** internal, it is
/// recreated as internal. This matters for `sandbox-proxy`: older builds
/// created it as a regular bridge (egress-allowed), which silently leaked
/// internet access to every port-exposing sandbox (see ADR-0004). Recreation
/// fails loudly if containers are still attached — stop them first
/// (`sandbox down` / `sandbox proxy stop`).
pub async fn ensure_internal(name: &str) -> Result<()> {
    if exists(name).await? {
        if is_internal(name).await? {
            return Ok(());
        }
        rm(name).await?;
    }
    run_capture(&["network", "create", "--internal", name]).await?;
    Ok(())
}

/// Create `name` as a regular (egress-allowed) bridge network if missing.
/// Reserved for the post-MVP dedicated proxy edge network; the reverse proxy
/// currently reaches the host via Docker's default `bridge` and routes to
/// sandboxes over the `--internal` `sandbox-proxy` network. See ADR-0004.
#[allow(dead_code)]
pub async fn ensure_bridge(name: &str) -> Result<()> {
    if exists(name).await? {
        return Ok(());
    }
    run_capture(&["network", "create", name]).await?;
    Ok(())
}

/// Whether a Docker network was created with `--internal`.
async fn is_internal(name: &str) -> Result<bool> {
    let out = run_capture(&["network", "inspect", name, "--format", "{{.Internal}}"]).await?;
    Ok(out.trim() == "true")
}

pub async fn exists(name: &str) -> Result<bool> {
    let probe = run_probe(&["network", "inspect", name]).await?;
    Ok(probe.success)
}

/// Connect a (possibly running) container to a network.
pub async fn connect(network: &str, container: &str) -> Result<()> {
    run_capture(&["network", "connect", network, container]).await?;
    Ok(())
}

/// Make a running container's internet egress match `want_egress`, reconciling
/// any leftover runtime `sandbox net on/off` from a previous session.
///
/// This is what makes `sandbox run` authoritative: the runtime toggle survives
/// a `sandbox down` (the bridge stays attached to the stopped container), so a
/// later `docker start` would silently resume **with** egress. Calling this on
/// every `run` re-enforces the profile — a default (safe) run revokes a stale
/// `net on`; `attach` deliberately does NOT call it, so it preserves state.
///
/// Detaching is strand-safe: `sandbox-internal` is (re)attached first, so the
/// container always keeps a network even if `bridge` was its only one (the
/// `--network`-at-create case).
pub async fn reconcile_egress(container: &str, want_egress: bool) -> Result<()> {
    let nets = inspect_networks(container).await?;
    let has_bridge = nets.iter().any(|n| n == BRIDGE);
    if want_egress {
        if !has_bridge {
            connect(BRIDGE, container).await?;
        }
    } else if has_bridge {
        if !nets.iter().any(|n| n == SANDBOX_INTERNAL) {
            connect(SANDBOX_INTERNAL, container).await?;
        }
        disconnect(BRIDGE, container).await?;
    }
    Ok(())
}

/// Remove a Docker network by name. Used to clean up
/// `sandbox-compose-<hash>` after `sandbox down --with-deps` /
/// `sandbox nuke`. Returns an error when other containers still attach to
/// the network — by design: that would indicate the rewire/teardown left
/// something behind.
pub async fn rm(name: &str) -> Result<()> {
    run_capture(&["network", "rm", name]).await?;
    Ok(())
}

/// Disconnect a container from a network. Treats "is not connected" as
/// success.
pub async fn disconnect(network: &str, container: &str) -> Result<()> {
    match run_capture(&["network", "disconnect", network, container]).await {
        Ok(_) => Ok(()),
        Err(Error::DockerFailed { stderr, .. }) if stderr.contains("is not connected") => Ok(()),
        Err(e) => Err(e),
    }
}

/// Per-project name of the `--internal` network that hosts compose deps in
/// safe mode (ADR-0010 § Decision item 5). Stable per project hash, so a
/// re-run reuses the same network.
pub fn compose_internal_name(short_hash: &str) -> String {
    format!("sandbox-compose-{short_hash}")
}

/// Ensure `sandbox-compose-<short_hash>` exists as `--internal`. Idempotent.
/// Returns the network name so the caller can pass it straight to
/// `Plan.additional_networks` and the rewire helper.
pub async fn ensure_compose_internal(short_hash: &str) -> Result<String> {
    let name = compose_internal_name(short_hash);
    ensure_internal(&name).await?;
    Ok(name)
}

/// Move every compose-managed service container off whatever networks it
/// joined at `docker compose up` time, onto `target_network`, preserving the
/// service name as a DNS alias so siblings can still reach `postgres`,
/// `redis`, etc. by name.
///
/// Used in safe mode (ADR-0010 § Decision item 5) to inherit the sandbox's
/// no-egress posture on the deps themselves. In `--network` mode this is
/// not called — the deps stay on the compose-default bridge.
///
/// Order matters: connect to the target first (with alias), then disconnect
/// from every other network. Doing it the other way around would leave the
/// container momentarily networkless and break in-flight init traffic.
pub async fn rewire_to_internal(target_network: &str, services: &[(String, String)]) -> Result<()> {
    for (service, container) in services {
        // Connect first, with the service name as the DNS alias on our
        // network. `--alias` is repeated per network connect, so this
        // doesn't disturb whatever aliases compose set up on the other
        // network (which we're about to drop anyway).
        run_capture(&[
            "network",
            "connect",
            "--alias",
            service,
            target_network,
            container,
        ])
        .await
        .or_else(|e| match e {
            // Already on it — treat as success so the operation is
            // idempotent (matters when the user re-runs `sandbox run
            // --with-deps` against an already-rewired project).
            Error::DockerFailed { stderr, .. } if stderr.contains("already exists") => {
                Ok(String::new())
            }
            other => Err(other),
        })?;

        let nets = inspect_networks(container).await?;
        for net in nets.iter().filter(|n| n.as_str() != target_network) {
            disconnect(net, container).await?;
        }
    }
    Ok(())
}

/// List the names of every Docker network a container is currently attached
/// to. Order is whatever `docker inspect` returns (not guaranteed stable, so
/// callers that compare should sort).
pub async fn inspect_networks(container: &str) -> Result<Vec<String>> {
    // The Go template walks `.NetworkSettings.Networks` (a map keyed by
    // network name) and prints each key on its own line. Empty output means
    // the container has no network attachments (rare; usually `none`).
    let stdout = run_capture(&[
        "inspect",
        container,
        "--format",
        "{{range $k,$v := .NetworkSettings.Networks}}{{$k}}{{println}}{{end}}",
    ])
    .await?;
    Ok(stdout
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect())
}
