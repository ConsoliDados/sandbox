//! `sandbox down [PROJECT]` — stop the container, keep state and volumes.

use std::path::PathBuf;

use sandbox_core::{LanguageRegistry, Project};

use crate::{Error, Result};

#[derive(Debug)]
pub(crate) struct Args {
    pub(crate) project: Option<String>,
    pub(crate) all: bool,
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
        return Ok(());
    }
    if sandbox_docker::is_running(&project.container_name).await? {
        sandbox_docker::stop(&project.container_name).await?;
        println!("stopped {}", project.container_name);
    } else {
        println!("already stopped: {}", project.container_name);
    }
    Ok(())
}

fn resolve_path(arg: Option<&str>) -> PathBuf {
    match arg {
        None | Some(".") => PathBuf::from("."),
        Some(p) => PathBuf::from(p),
    }
}
