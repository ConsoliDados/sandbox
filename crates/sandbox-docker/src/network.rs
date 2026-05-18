//! Docker network operations. The default sandbox network is `--internal`,
//! meaning containers connected to it have no internet egress. Per-project
//! egress toggling (`sandbox net on`) attaches the bridge network on top.

use crate::cmd::{run_capture, run_probe};
use crate::{Error, Result};

pub const SANDBOX_INTERNAL: &str = "sandbox-internal";

/// Docker's default `bridge` network. Attached by `sandbox net on` to grant
/// internet egress at runtime; detached by `sandbox net off`. See ADR-0004.
pub const BRIDGE: &str = "bridge";

/// Create `name` as an `--internal` network if it doesn't exist.
pub async fn ensure_internal(name: &str) -> Result<()> {
    if exists(name).await? {
        return Ok(());
    }
    run_capture(&["network", "create", "--internal", name]).await?;
    Ok(())
}

/// Create `name` as a regular (egress-allowed) bridge network if missing.
/// Used by the reverse proxy: Traefik needs to reach project containers
/// **and** host packets routed in, so this network must not be `--internal`.
pub async fn ensure_bridge(name: &str) -> Result<()> {
    if exists(name).await? {
        return Ok(());
    }
    run_capture(&["network", "create", name]).await?;
    Ok(())
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
