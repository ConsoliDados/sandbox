//! Named-volume operations. Idempotent by design.

use crate::cmd::{run_capture, run_probe};
use crate::{Error, Result};

/// Create a named volume if it doesn't already exist. Idempotent.
pub async fn ensure(name: &str) -> Result<()> {
    if exists(name).await? {
        return Ok(());
    }
    run_capture(&["volume", "create", name]).await?;
    Ok(())
}

pub async fn exists(name: &str) -> Result<bool> {
    let probe = run_probe(&["volume", "inspect", name]).await?;
    Ok(probe.success)
}

/// Remove a named volume. Treats "no such volume" as success (idempotent
/// teardown). Returns an error for any other failure.
pub async fn remove(name: &str) -> Result<()> {
    match run_capture(&["volume", "rm", name]).await {
        Ok(_) => Ok(()),
        Err(Error::DockerFailed { stderr, .. }) if stderr.contains("no such volume") => Ok(()),
        Err(e) => Err(e),
    }
}
