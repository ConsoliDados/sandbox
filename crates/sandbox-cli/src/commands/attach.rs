//! `sandbox attach [PATH]` — re-enter the shell of a *running* sandbox.
//!
//! Drops the user back into the same shell / workdir / host-user that
//! `sandbox run` gives, via `docker exec -it`, but WITHOUT the pre-flight scan
//! or any of `run`'s flight checks. It is the lightweight "get me back in"
//! verb to pair with the fact that exiting the shell leaves the container
//! running (PID 1 is `sleep infinity`; see `run.rs`).
//!
//! The container must already be running. A stopped container is deliberately
//! NOT auto-started here: waking it has to go through `sandbox run`, which
//! re-scans — the host source may have changed since the container was built.
//! Missing → exit 40 (`ContainerNotFound`); stopped → exit 40
//! (`ContainerNotRunning`); both point the user at `sandbox run`.

use std::path::PathBuf;

use sandbox_core::{Config, LanguageRegistry, Paths, Project};
use sandbox_docker::{ExecOpts, UserSpec};

use crate::commands::exec::render_exec_args;
use crate::{Error, Result};

#[derive(Debug)]
pub(crate) struct Args {
    pub(crate) project: Option<String>,
    pub(crate) lang: Option<String>,
    pub(crate) print_cmd: bool,
}

pub(crate) async fn execute(args: Args) -> Result<()> {
    let span = tracing::info_span!("attach");
    let _entered = span.enter();

    let path = resolve_path(args.project.as_deref());
    let (project, opts, cmd) = resolve_target(&path, args.lang.as_deref())?;

    if args.print_cmd {
        let argv = render_exec_args(&project.container_name, &opts, &cmd);
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
    sandbox_docker::exec(&project.container_name, &opts, &cmd).await?;
    Ok(())
}

/// Resolve the project, the shell to exec, and the `ExecOpts`, mirroring
/// `run.rs::Context::load` so the shell/workdir/user match what `run` opens.
/// Returns the single-element `cmd` (the manifest shell) so the caller can feed
/// it to both `--print-cmd` rendering and the real `docker exec`.
fn resolve_target(
    path: &std::path::Path,
    lang: Option<&str>,
) -> Result<(Project, ExecOpts, [String; 1])> {
    let paths = Paths::discover()?;
    let cfg = Config::load_or_default(&paths.config_file())?;

    let mut registry = LanguageRegistry::builtin()?;
    let user_dir = paths.user_languages_dir();
    if user_dir.exists() {
        registry.load_from_dir(&user_dir)?;
    }
    for d in &cfg.defaults.language_dirs {
        registry.load_from_dir(d)?;
    }

    let project = Project::resolve(path, &registry, lang)?;
    let manifest = registry.require(project.language.as_str())?;
    let user = UserSpec::current()?;

    let opts = ExecOpts {
        user: Some(format!("{}:{}", user.uid, user.gid)),
        workdir: Some(manifest.workdir.clone()),
        interactive: true,
        tty: true,
    };
    let cmd = [manifest.shell.clone()];
    Ok((project, opts, cmd))
}

fn resolve_path(arg: Option<&str>) -> PathBuf {
    match arg {
        None | Some(".") => PathBuf::from("."),
        Some(p) => PathBuf::from(p),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sandbox_core::{ContainerName, ProjectHash};

    fn name() -> ContainerName {
        ContainerName::from_hash(&ProjectHash::from_bytes([7u8; 32]))
    }

    #[test]
    fn render_uses_manifest_shell_workdir_and_host_user() {
        let opts = ExecOpts {
            user: Some("1000:1000".into()),
            workdir: Some("/app".into()),
            interactive: true,
            tty: true,
        };
        let argv = render_exec_args(&name(), &opts, &["zsh".to_string()]);
        assert!(argv.contains(&"--interactive".to_string()));
        assert!(argv.contains(&"--tty".to_string()));
        assert!(argv.windows(2).any(|w| w == ["--user", "1000:1000"]));
        assert!(argv.windows(2).any(|w| w == ["--workdir", "/app"]));
        assert_eq!(argv.last().map(String::as_str), Some("zsh"));
    }

    #[test]
    fn resolve_path_defaults_to_cwd() {
        assert_eq!(resolve_path(None), PathBuf::from("."));
        assert_eq!(resolve_path(Some(".")), PathBuf::from("."));
        assert_eq!(resolve_path(Some("/srv/app")), PathBuf::from("/srv/app"));
    }
}
