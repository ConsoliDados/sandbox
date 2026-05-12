//! `sandbox nuke [PROJECT]` — remove container, named volumes, and state.

use std::path::PathBuf;

use sandbox_core::{LanguageRegistry, Paths, Project};

use crate::{Error, Result};

#[derive(Debug)]
pub(crate) struct Args {
    pub(crate) project: Option<String>,
    pub(crate) all: bool,
    pub(crate) keep_volumes: bool,
    pub(crate) keep_state: bool,
}

pub(crate) async fn execute(args: Args) -> Result<()> {
    if args.all {
        return Err(Error::NotImplemented);
    }
    let path = resolve_path(args.project.as_deref());
    let registry = LanguageRegistry::builtin()?;
    let project = Project::resolve(&path, &registry, None)?;

    sandbox_docker::rm(&project.container_name, true).await?;
    println!("removed container {}", project.container_name);

    if !args.keep_volumes {
        for vol in project.named_volumes() {
            sandbox_docker::remove_volume(vol.as_str()).await?;
        }
        println!("removed named volumes for {}", project.container_name);
    }

    if !args.keep_state {
        let paths = Paths::discover()?;
        let state_dir = paths.container_state_dir(&project.hash.short());
        if state_dir.exists() {
            std::fs::remove_dir_all(&state_dir)?;
            println!("removed state dir {}", state_dir.display());
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
