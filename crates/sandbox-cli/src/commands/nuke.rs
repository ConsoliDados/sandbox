//! `sandbox nuke [PROJECT]` — remove container, named volumes, and state.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use sandbox_core::{LanguageRegistry, Meta, Paths, Project};

use crate::{Error, Result};

#[derive(Debug)]
pub(crate) struct Args {
    pub(crate) project: Option<String>,
    pub(crate) all: bool,
    pub(crate) keep_volumes: bool,
    pub(crate) keep_state: bool,
    pub(crate) yes: bool,
}

pub(crate) async fn execute(args: Args) -> Result<()> {
    if args.all {
        return Err(Error::NotImplemented);
    }
    let path = resolve_path(args.project.as_deref());
    let registry = LanguageRegistry::builtin()?;
    let project = Project::resolve(&path, &registry, None)?;

    let what = describe_targets(&project, args.keep_volumes, args.keep_state);
    if !args.yes && !confirm(&project.container_name.to_string(), &what)? {
        println!("aborted");
        return Ok(());
    }

    // Tear down compose deps BEFORE we drop the state dir — the
    // `Meta.compose` block is our only record of what `sandbox` started.
    // Reading it must happen while it's still on disk.
    let paths = Paths::discover()?;
    let state_dir = paths.container_state_dir(project.hash.short().as_str());
    tear_down_compose_if_recorded(&project, &state_dir).await?;

    sandbox_docker::rm(&project.container_name, true).await?;
    println!("removed container {}", project.container_name);

    if !args.keep_volumes {
        for vol in project.named_volumes() {
            sandbox_docker::remove_volume(vol.as_str()).await?;
        }
        println!("removed named volumes for {}", project.container_name);
    }

    if !args.keep_state && state_dir.exists() {
        std::fs::remove_dir_all(&state_dir)?;
        println!("removed state dir {}", state_dir.display());
    }
    Ok(())
}

/// `nuke` is "remove everything for this project" — compose deps brought up
/// by `sandbox run --with-deps` are part of "everything". No opt-in flag:
/// matches the rest of the nuke semantics (volumes + state are also taken
/// down by default; `--keep-*` flags carve exceptions). The check is
/// gracefully no-op when there's no `[compose]` block recorded.
async fn tear_down_compose_if_recorded(
    project: &Project,
    state_dir: &std::path::Path,
) -> Result<()> {
    if !Meta::exists_at(state_dir) {
        return Ok(());
    }
    let meta = Meta::load(state_dir)?;
    let Some(compose) = meta.compose else {
        return Ok(());
    };
    sandbox_docker::compose_down(&compose.project_name).await?;
    println!("compose deps stopped: project={}", compose.project_name);
    let expected_internal = sandbox_docker::compose_internal_name(project.hash.short().as_str());
    if compose.network == expected_internal
        && let Err(e) = sandbox_docker::network_rm(&compose.network).await
    {
        tracing::warn!(network = %compose.network, error = %e, "could not remove compose-internal network");
    }
    Ok(())
}

fn describe_targets(project: &Project, keep_volumes: bool, keep_state: bool) -> String {
    let mut parts = vec!["container".to_string()];
    if !keep_volumes {
        let count = project.named_volumes().len();
        if count > 0 {
            parts.push(format!("{count} named volume(s)"));
        }
    }
    if !keep_state {
        parts.push("state directory".to_string());
    }
    parts.join(", ")
}

/// Prompt the user on stderr. Returns true only if the answer is an explicit
/// `y` / `yes`. EOF or any other input (including empty line) defaults to
/// abort, matching the SRS `[--yes | -y]` semantics.
fn confirm(name: &str, targets: &str) -> Result<bool> {
    let mut stderr = std::io::stderr().lock();
    write!(
        stderr,
        "About to remove {targets} for `{name}`. Continue? [y/N] "
    )?;
    stderr.flush()?;
    drop(stderr);

    let mut line = String::new();
    let stdin = std::io::stdin();
    if stdin.lock().read_line(&mut line)? == 0 {
        return Ok(false);
    }
    let answer = line.trim().to_ascii_lowercase();
    Ok(matches!(answer.as_str(), "y" | "yes"))
}

fn resolve_path(arg: Option<&str>) -> PathBuf {
    match arg {
        None | Some(".") => PathBuf::from("."),
        Some(p) => PathBuf::from(p),
    }
}
