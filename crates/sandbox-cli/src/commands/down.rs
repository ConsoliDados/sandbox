//! `sandbox down [PROJECT]` — stop the container, keep state and volumes.

use std::path::PathBuf;

use sandbox_core::{LanguageRegistry, Meta, Paths, Project};

use crate::{Error, Result};

#[derive(Debug)]
pub(crate) struct Args {
    pub(crate) project: Option<String>,
    pub(crate) all: bool,
    pub(crate) with_deps: bool,
}

pub(crate) async fn execute(args: Args) -> Result<()> {
    if args.all {
        return Err(Error::NotImplemented);
    }
    let path = resolve_path(args.project.as_deref());
    let registry = LanguageRegistry::builtin()?;
    let project = Project::resolve(&path, &registry, None)?;

    if !sandbox_docker::exists(&project.container_name).await? {
        println!("no sandbox container for {}", project.container_name);
    } else if sandbox_docker::is_running(&project.container_name).await? {
        sandbox_docker::stop(&project.container_name).await?;
        println!("stopped {}", project.container_name);
    } else {
        println!("already stopped: {}", project.container_name);
    }

    if args.with_deps {
        tear_down_compose(&project).await?;
    }
    Ok(())
}

/// Tear down the compose deps the project's last `sandbox run --with-deps`
/// brought up. Read from `Meta.compose` so we only touch what `sandbox`
/// itself started — never a compose project the user is running by hand.
///
/// Idempotent: missing meta, missing `[compose]` block, or already-down
/// project each report cleanly and exit zero.
async fn tear_down_compose(project: &Project) -> Result<()> {
    let paths = Paths::discover()?;
    let state_dir = paths.container_state_dir(project.hash.short().as_str());
    if !Meta::exists_at(&state_dir) {
        println!("no compose deps recorded for {}", project.container_name);
        return Ok(());
    }
    let meta = Meta::load(&state_dir)?;
    let Some(compose) = meta.compose else {
        println!("no compose deps recorded for {}", project.container_name);
        return Ok(());
    };

    sandbox_docker::compose_down(&compose.project_name).await?;
    println!("compose deps stopped: project={}", compose.project_name);

    // The `--internal` network we created for safe-mode rewire is ours to
    // remove; `compose down` doesn't know about it. The compose-default
    // bridge (used in `--network` mode) is cleaned up by `compose down`
    // itself — leave it alone.
    let expected_internal = sandbox_docker::compose_internal_name(project.hash.short().as_str());
    if compose.network == expected_internal {
        // Best-effort: rm fails if other containers still attach, which
        // would be a bug — we just disconnected everything. Surface the
        // error so the user knows the network leaked.
        if let Err(e) = sandbox_docker::network_rm(&compose.network).await {
            tracing::warn!(network = %compose.network, error = %e, "could not remove compose-internal network");
        }
    }
    Ok(())
}

fn resolve_path(arg: Option<&str>) -> PathBuf {
    match arg {
        None | Some(".") => PathBuf::from("."),
        Some(p) => PathBuf::from(p),
    }
}
