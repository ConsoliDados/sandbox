//! Docker network operations. The default sandbox network is `--internal`,
//! meaning containers connected to it have no internet egress. Per-project
//! egress toggling (`sandbox net on`) attaches the bridge network on top.

use crate::cmd::{run_capture, run_probe};
use crate::{Error, Result};

pub const SANDBOX_INTERNAL: &str = "sandbox-internal";

/// Create `name` as an `--internal` network if it doesn't exist.
pub async fn ensure_internal(name: &str) -> Result<()> {
    if exists(name).await? {
        return Ok(());
    }
    run_capture(&["network", "create", "--internal", name]).await?;
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
