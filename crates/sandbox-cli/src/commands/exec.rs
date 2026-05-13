//! `sandbox exec PROJECT -- CMD [ARGS...] [--user USER] [--workdir PATH]` —
//! run a command inside a running sandbox container.
//!
//! Container must be running; if it isn't we exit 40 per SRS § `exec` so the
//! shell suggests `sandbox run` first. The default workdir is `/app`; `--user`
//! is free-form (passed straight to `docker exec --user`).

use std::path::PathBuf;

use sandbox_core::{LanguageRegistry, Project};
use sandbox_docker::ExecOpts;

use crate::{Error, Result};

const DEFAULT_WORKDIR: &str = "/app";

#[derive(Debug)]
pub(crate) struct Args {
    pub(crate) project: Option<String>,
    pub(crate) cmd: Vec<String>,
    pub(crate) user: Option<String>,
    pub(crate) workdir: Option<String>,
    pub(crate) print_cmd: bool,
}

pub(crate) async fn execute(args: Args) -> Result<()> {
    if args.cmd.is_empty() {
        return Err(Error::Clap(clap::Error::raw(
            clap::error::ErrorKind::MissingRequiredArgument,
            "exec requires a command after `--`\n",
        )));
    }

    let path = resolve_path(args.project.as_deref());
    let registry = LanguageRegistry::builtin()?;
    let project = Project::resolve(&path, &registry, None)?;

    let opts = ExecOpts {
        user: args.user,
        workdir: Some(args.workdir.unwrap_or_else(|| DEFAULT_WORKDIR.into())),
        interactive: true,
        tty: true,
    };

    if args.print_cmd {
        let argv = render_exec_args(&project.container_name, &opts, &args.cmd);
        println!("docker {}", argv.join(" "));
        return Ok(());
    }

    if !sandbox_docker::exists(&project.container_name).await? {
        return Err(Error::ContainerNotFound {
            name: project.container_name.as_str().to_string(),
        });
    }
    if !sandbox_docker::is_running(&project.container_name).await? {
        return Err(Error::ContainerNotRunning {
            name: project.container_name.as_str().to_string(),
        });
    }
    sandbox_docker::exec(&project.container_name, &opts, &args.cmd).await?;
    Ok(())
}

fn resolve_path(arg: Option<&str>) -> PathBuf {
    match arg {
        None | Some(".") => PathBuf::from("."),
        Some(p) => PathBuf::from(p),
    }
}

fn render_exec_args(
    name: &sandbox_core::ContainerName,
    opts: &ExecOpts,
    cmd: &[String],
) -> Vec<String> {
    let mut argv: Vec<String> = vec!["exec".into()];
    if opts.interactive {
        argv.push("--interactive".into());
    }
    if opts.tty {
        argv.push("--tty".into());
    }
    if let Some(u) = &opts.user {
        argv.push("--user".into());
        argv.push(u.clone());
    }
    if let Some(w) = &opts.workdir {
        argv.push("--workdir".into());
        argv.push(w.clone());
    }
    argv.push(name.as_str().into());
    argv.extend(cmd.iter().cloned());
    argv
}

#[cfg(test)]
mod tests {
    use super::*;
    use sandbox_core::{ContainerName, ProjectHash};

    fn name() -> ContainerName {
        ContainerName::from_hash(&ProjectHash::from_bytes([7u8; 32]))
    }

    #[test]
    fn render_exec_args_uses_defaults() {
        let opts = ExecOpts {
            user: None,
            workdir: Some(DEFAULT_WORKDIR.into()),
            interactive: true,
            tty: true,
        };
        let argv = render_exec_args(&name(), &opts, &["bash".to_string()]);
        assert!(argv.contains(&"--interactive".to_string()));
        assert!(argv.contains(&"--tty".to_string()));
        assert!(argv.windows(2).any(|w| w == ["--workdir", "/app"]));
        assert!(!argv.contains(&"--user".to_string()));
        assert_eq!(argv.last().map(String::as_str), Some("bash"));
    }

    #[test]
    fn render_exec_args_honors_user_and_workdir() {
        let opts = ExecOpts {
            user: Some("1000:1000".into()),
            workdir: Some("/srv".into()),
            interactive: true,
            tty: true,
        };
        let argv = render_exec_args(
            &name(),
            &opts,
            &["sh".into(), "-c".into(), "echo hi".into()],
        );
        assert!(argv.windows(2).any(|w| w == ["--user", "1000:1000"]));
        assert!(argv.windows(2).any(|w| w == ["--workdir", "/srv"]));
        assert_eq!(
            argv.iter()
                .rev()
                .take(3)
                .rev()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["sh", "-c", "echo hi"]
        );
    }
}
