//! Named-volume operations. Idempotent by design.

use crate::cmd::{run_capture, run_probe};
use crate::{Error, Result};

/// Tiny image used by [`chown`] to fix volume ownership on first creation.
/// Pinned to a digest is overkill for v0.1 — alpine:3 is updated rarely and
/// only by trusted maintainers (the publish is GPG-signed).
const INIT_IMAGE: &str = "alpine:3";

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

/// Reset ownership of every file in `name` to `uid:gid`.
///
/// Named volumes are created root-owned by the daemon. When the project
/// container later runs as a non-root host UID (per ADR-0009 / OQ-004),
/// `npm install` / `cargo build` / `pip install` hit EACCES on first
/// write. Running a one-shot chown in an init container fixes this without
/// granting the project container root.
///
/// The init container itself runs as root *inside the container*
/// (`--user 0:0`) so it can call `chown`, but has `--network none` and
/// no project bind mounts — it only sees the named volume. Safe.
pub async fn chown(name: &str, uid: u32, gid: u32) -> Result<()> {
    let bind = format!("{name}:/v");
    let ownership = format!("{uid}:{gid}");
    run_capture(&[
        "run",
        "--rm",
        "--network",
        "none",
        "--user",
        "0:0",
        "-v",
        &bind,
        INIT_IMAGE,
        "chown",
        "-R",
        &ownership,
        "/v",
    ])
    .await?;
    Ok(())
}

/// Create the volume if missing **and**, on first creation only, chown it
/// to the host user. Returns `true` when the volume was newly created (so
/// callers can log / report a one-time setup cost), `false` when it already
/// existed and no work was needed.
pub async fn ensure_owned(name: &str, uid: u32, gid: u32) -> Result<bool> {
    if exists(name).await? {
        return Ok(false);
    }
    run_capture(&["volume", "create", name]).await?;
    chown(name, uid, gid).await?;
    Ok(true)
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
