//! Helpers around `tokio::process::Command` for invoking `docker`.
//!
//! Two flavours: `run_capture` returns the stdout when the command succeeds and
//! converts non-zero exits into a typed error; `run_probe` is a "did it
//! succeed?" predicate that never errors on non-zero exit. The probe is for
//! "does this volume/network/container exist?" lookups where exit-non-zero is
//! the expected negative answer.

use tokio::process::Command;

use crate::{Error, Result};

const DAEMON_DOWN_NEEDLES: &[&str] = &[
    "Cannot connect to the Docker daemon",
    "Is the docker daemon running?",
];

pub(crate) async fn run_capture(args: &[&str]) -> Result<String> {
    let output = Command::new("docker")
        .args(args)
        .output()
        .await
        .map_err(|source| Error::Io { source })?;

    let stdout = String::from_utf8(output.stdout).map_err(|_| Error::NonUtf8Output {
        cmd: format!("docker {}", args.join(" ")),
    })?;

    if output.status.success() {
        return Ok(stdout);
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if DAEMON_DOWN_NEEDLES.iter().any(|n| stderr.contains(n)) {
        return Err(Error::DaemonUnreachable(stderr));
    }
    Err(Error::DockerFailed {
        cmd: format!("docker {}", args.join(" ")),
        code: output.status.code().unwrap_or(-1),
        stderr,
    })
}

pub(crate) async fn run_probe(args: &[&str]) -> Result<Probe> {
    let output = Command::new("docker")
        .args(args)
        .output()
        .await
        .map_err(|source| Error::Io { source })?;
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() && DAEMON_DOWN_NEEDLES.iter().any(|n| stderr.contains(n)) {
        return Err(Error::DaemonUnreachable(stderr));
    }
    Ok(Probe {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr,
    })
}

/// Inherit stdio (interactive) and wait. Treats a non-zero exit as an error —
/// for `docker build` / `docker run <oneshot>` where a non-zero status really
/// is a failure of the operation we asked for.
pub(crate) async fn run_attached(args: &[&str]) -> Result<()> {
    let status = Command::new("docker")
        .args(args)
        .status()
        .await
        .map_err(|source| Error::Io { source })?;
    if status.success() {
        return Ok(());
    }
    Err(Error::DockerFailed {
        cmd: format!("docker {}", args.join(" ")),
        code: status.code().unwrap_or(-1),
        stderr: "(stderr inherited)".to_string(),
    })
}

/// Inherit stdio and wait, returning the attached process's exit code. A
/// non-zero code is **not** an error here: for an interactive shell it's the
/// user's last command status (or 130 from Ctrl-C), and exiting the shell is
/// the normal way out — surfacing it as "docker exec failed" is noise. Only a
/// failure to spawn `docker` is an `Err`. Callers decide whether the code
/// matters: `sandbox exec -- CMD` propagates it; the `run`/`attach` shell
/// ignores it.
pub(crate) async fn run_interactive(args: &[&str]) -> Result<i32> {
    let status = Command::new("docker")
        .args(args)
        .status()
        .await
        .map_err(|source| Error::Io { source })?;
    // `code()` is `None` when the child was killed by a signal; 130 (128+SIGINT)
    // is the conventional stand-in and matches what an interactive Ctrl-C yields.
    Ok(status.code().unwrap_or(130))
}

#[derive(Debug)]
pub(crate) struct Probe {
    pub success: bool,
    pub stdout: String,
    #[allow(dead_code)]
    pub stderr: String,
}
