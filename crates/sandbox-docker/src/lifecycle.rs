//! Container lifecycle: existence checks, run from Plan, start/stop/exec/rm.
//!
//! Every operation either captures stdout (silent ops, used by orchestrators)
//! or attaches to the user's terminal (`docker run -it`, `docker exec -it`).

use sandbox_core::ContainerName;
use serde::Deserialize;

use crate::cmd::{run_attached, run_capture, run_probe};
use crate::plan::Plan;
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

/// Create + (optionally) connect extra networks + start the container, all
/// detached. The container's PID 1 (set in `Plan.entrypoint`) is expected
/// to be a long-running keepalive (`sleep infinity` for the dev-environment
/// flow) so the container survives the user's interactive shell coming and
/// going via `exec`.
///
/// `docker run --network` accepts only a single network, so when
/// `plan.additional_networks` is non-empty we go through
/// `docker create → docker network connect each extra → docker start`.
/// When it's empty we skip the network-connect step.
pub async fn run(plan: &Plan) -> Result<()> {
    let mut create_args = plan.to_args();
    if let Some(first) = create_args.first_mut() {
        *first = "create".into();
    }
    let argv: Vec<&str> = create_args.iter().map(String::as_str).collect();
    run_capture(&argv).await?;

    for net in &plan.additional_networks {
        run_capture(&[
            "network",
            "connect",
            net.as_str(),
            plan.container_name.as_str(),
        ])
        .await?;
    }

    run_capture(&["start", plan.container_name.as_str()]).await?;
    Ok(())
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

/// Options for `docker exec`. `user` is a free-form spec passed through to
/// `--user` (name, uid, or `uid:gid`); `None` lets Docker pick the image's
/// default user.
#[derive(Debug, Clone)]
pub struct ExecOpts {
    pub user: Option<String>,
    pub workdir: Option<String>,
    pub interactive: bool,
    pub tty: bool,
}

/// Options for `docker logs`. `tail = None` means stream the full history;
/// callers should set the SRS default (200) themselves when invoking this from
/// `sandbox logs`. `since` is a free-form duration string (`5m`, `1h`) or
/// timestamp accepted by Docker.
#[derive(Debug, Clone, Default)]
pub struct LogsOpts {
    pub follow: bool,
    pub tail: Option<u32>,
    pub since: Option<String>,
}

/// Build the `docker logs` argv for the given container + opts. Pulled out of
/// [`logs`] so `--print-cmd` can render the invocation without executing.
pub fn logs_args(name: &ContainerName, opts: &LogsOpts) -> Vec<String> {
    let mut args: Vec<String> = vec!["logs".into()];
    if opts.follow {
        args.push("--follow".into());
    }
    if let Some(n) = opts.tail {
        args.push(format!("--tail={n}"));
    }
    if let Some(since) = &opts.since {
        args.push(format!("--since={since}"));
    }
    args.push(name.as_str().into());
    args
}

/// Stream container logs to the user's terminal. With `follow=true` blocks
/// until the user kills the stream or the container exits.
pub async fn logs(name: &ContainerName, opts: &LogsOpts) -> Result<()> {
    let args = logs_args(name, opts);
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    run_attached(&argv).await
}

/// Summary of a container as reported by `docker ps`. Field set is whatever
/// the `{{json .}}` formatter emits in Docker 24+; we keep only the columns
/// `sandbox ps` renders.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ContainerInfo {
    /// Container name (e.g. `sandbox-abc123def456`). Docker's ps format
    /// uses the singular field "Names" containing comma-joined aliases;
    /// for our prefix-named containers it is always a single entry.
    pub names: String,
    /// Human status string: `Up 5 hours`, `Exited (0) 2 minutes ago`, etc.
    pub status: String,
    /// Raw state: `running`, `exited`, `created`, `paused`, `restarting`.
    pub state: String,
    /// Comma-joined network names: `sandbox-internal`, `bridge`, etc.
    pub networks: String,
    pub image: String,
    pub running_for: String,
}

/// Returns the docker arguments used by [`list_sandboxes`]. Exposed so
/// `--print-cmd` can show what would run without actually executing it.
pub fn list_sandboxes_args() -> &'static [&'static str] {
    &[
        "ps",
        "--all",
        "--no-trunc",
        "--filter",
        "name=^sandbox-",
        "--format",
        "{{json .}}",
    ]
}

/// List all containers managed by sandbox (name starts with `sandbox-`).
/// Includes stopped containers; callers filter by `state` if they want only
/// running ones.
pub async fn list_sandboxes() -> Result<Vec<ContainerInfo>> {
    let stdout = run_capture(list_sandboxes_args()).await?;
    parse_ps_json(&stdout)
}

fn parse_ps_json(stdout: &str) -> Result<Vec<ContainerInfo>> {
    let mut out = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let info: ContainerInfo =
            serde_json::from_str(trimmed).map_err(|e| Error::InvalidJson {
                cmd: "docker ps".into(),
                reason: e.to_string(),
            })?;
        out.push(info);
    }
    Ok(out)
}

pub async fn exec(name: &ContainerName, opts: &ExecOpts, cmd: &[String]) -> Result<()> {
    let mut args: Vec<String> = vec!["exec".into()];
    if opts.interactive {
        args.push("--interactive".into());
    }
    if opts.tty {
        args.push("--tty".into());
    }
    if let Some(u) = &opts.user {
        args.push("--user".into());
        args.push(u.clone());
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

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn list_sandboxes_args_carries_name_filter() {
        let args = list_sandboxes_args();
        assert!(args.contains(&"ps"));
        assert!(args.contains(&"--all"));
        assert!(args.windows(2).any(|w| w == ["--filter", "name=^sandbox-"]));
        assert!(args.windows(2).any(|w| w == ["--format", "{{json .}}"]));
    }

    #[test]
    fn parse_ps_json_collects_one_per_line() -> TestResult {
        let fixture = r#"
{"Names":"sandbox-abc123","Status":"Up 5 hours","State":"running","Networks":"sandbox-internal","Image":"node:24","RunningFor":"5 hours ago"}
{"Names":"sandbox-def456","Status":"Exited (0) 2 minutes ago","State":"exited","Networks":"bridge","Image":"rust:1.85","RunningFor":"3 hours ago"}
"#;
        let infos = parse_ps_json(fixture)?;
        let summary: Vec<_> = infos
            .iter()
            .map(|i| (i.names.as_str(), i.state.as_str(), i.networks.as_str()))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("sandbox-abc123", "running", "sandbox-internal"),
                ("sandbox-def456", "exited", "bridge"),
            ]
        );
        Ok(())
    }

    #[test]
    fn parse_ps_json_yields_empty_for_no_containers() -> TestResult {
        assert!(parse_ps_json("")?.is_empty());
        assert!(parse_ps_json("   \n  \n")?.is_empty());
        Ok(())
    }

    #[test]
    fn parse_ps_json_errors_on_invalid_line() {
        let result = parse_ps_json("not json\n");
        assert!(matches!(result, Err(Error::InvalidJson { .. })));
    }

    fn cn() -> ContainerName {
        ContainerName::from_hash(&sandbox_core::ProjectHash::from_bytes([1u8; 32]))
    }

    #[test]
    fn logs_args_default_only_carries_name() {
        let name = cn();
        let args = logs_args(&name, &LogsOpts::default());
        assert_eq!(args.first().map(String::as_str), Some("logs"));
        assert_eq!(args.last().map(String::as_str), Some(name.as_str()));
        assert_eq!(args.len(), 2);
    }

    #[test]
    fn logs_args_renders_follow_tail_and_since() {
        let opts = LogsOpts {
            follow: true,
            tail: Some(50),
            since: Some("5m".into()),
        };
        let args = logs_args(&cn(), &opts);
        assert!(args.contains(&"--follow".to_string()));
        assert!(args.contains(&"--tail=50".to_string()));
        assert!(args.contains(&"--since=5m".to_string()));
    }
}
