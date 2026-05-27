//! `sandbox logs PROJECT [--follow] [--tail N] [--since DUR]` — stream
//! container logs to the user's terminal.
//!
//! Default tail is 200 per SRS § `logs`. Logs are available for stopped
//! containers too; only an entirely missing container yields exit 40.

use std::path::PathBuf;

use sandbox_core::{LanguageRegistry, Project};
use sandbox_docker::LogsOpts;

use crate::{Error, Result};

const DEFAULT_TAIL: u32 = 200;

#[derive(Debug)]
pub(crate) struct Args {
    pub(crate) project: Option<String>,
    pub(crate) follow: bool,
    pub(crate) tail: Option<u32>,
    pub(crate) since: Option<String>,
    pub(crate) print_cmd: bool,
}

pub(crate) async fn execute(args: Args) -> Result<()> {
    let path = resolve_path(args.project.as_deref());
    let registry = LanguageRegistry::builtin()?;
    let project = Project::resolve(&path, &registry, None)?;

    let opts = LogsOpts {
        follow: args.follow,
        tail: Some(args.tail.unwrap_or(DEFAULT_TAIL)),
        since: args.since,
    };

    if args.print_cmd {
        let argv = sandbox_docker::logs_args(&project.container_name, &opts);
        println!("docker {}", argv.join(" "));
        return Ok(());
    }

    if !sandbox_docker::exists(&project.container_name).await? {
        return Err(Error::ContainerNotFound {
            name: project.container_name.as_str().to_string(),
        });
    }
    sandbox_docker::logs(&project.container_name, &opts).await?;
    Ok(())
}

fn resolve_path(arg: Option<&str>) -> PathBuf {
    match arg {
        None | Some(".") => PathBuf::from("."),
        Some(p) => PathBuf::from(p),
    }
}
