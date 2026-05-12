//! Container lifecycle: existence checks, run from Plan, start/stop/exec/rm.
//!
//! Every operation either captures stdout (silent ops, used by orchestrators)
//! or attaches to the user's terminal (`docker run -it`, `docker exec -it`).

use sandbox_core::ContainerName;

use crate::cmd::{run_attached, run_capture, run_probe};
use crate::plan::{Plan, UserSpec};
use crate::{Error, Result};

pub async fn exists(name: &ContainerName) -> Result<bool> {
    let probe = run_probe(&["container", "inspect", name.as_str()]).await?;
    Ok(probe.success)
}

pub async fn is_running(name: &ContainerName) -> Result<bool> {
    let probe = run_probe(&[
        "container",
        "inspect",
        "--format",
        "{{.State.Running}}",
        name.as_str(),
    ])
    .await?;
    Ok(probe.success && probe.stdout.trim() == "true")
}

/// Execute the plan via `docker run`. Attaches stdio when interactive+tty so
/// the user gets a real shell; otherwise captures output for tracing.
pub async fn run(plan: &Plan) -> Result<()> {
    let args = plan.to_args();
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    if plan.interactive && plan.tty {
        run_attached(&argv).await
    } else {
        run_capture(&argv).await.map(|_| ())
    }
}

pub async fn start(name: &ContainerName) -> Result<()> {
    run_capture(&["start", name.as_str()]).await?;
    Ok(())
}

pub async fn stop(name: &ContainerName) -> Result<()> {
    run_capture(&["stop", name.as_str()]).await?;
    Ok(())
}

/// Remove a container. `force` translates to `--force` (kills if running).
pub async fn rm(name: &ContainerName, force: bool) -> Result<()> {
    let result = if force {
        run_capture(&["rm", "--force", name.as_str()]).await
    } else {
        run_capture(&["rm", name.as_str()]).await
    };
    match result {
        Ok(_) => Ok(()),
        Err(Error::DockerFailed { stderr, .. }) if stderr.contains("No such container") => Ok(()),
        Err(e) => Err(e),
    }
}

#[derive(Debug, Clone)]
pub struct ExecOpts {
    pub user: Option<UserSpec>,
    pub workdir: Option<String>,
    pub interactive: bool,
    pub tty: bool,
}

pub async fn exec(name: &ContainerName, opts: &ExecOpts, cmd: &[String]) -> Result<()> {
    let mut args: Vec<String> = vec!["exec".into()];
    if opts.interactive {
        args.push("--interactive".into());
    }
    if opts.tty {
        args.push("--tty".into());
    }
    if let Some(u) = opts.user {
        args.push("--user".into());
        args.push(format!("{}:{}", u.uid, u.gid));
    }
    if let Some(wd) = &opts.workdir {
        args.push("--workdir".into());
        args.push(wd.clone());
    }
    args.push(name.as_str().into());
    args.extend(cmd.iter().cloned());

    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    if opts.interactive && opts.tty {
        run_attached(&argv).await
    } else {
        run_capture(&argv).await.map(|_| ())
    }
}
