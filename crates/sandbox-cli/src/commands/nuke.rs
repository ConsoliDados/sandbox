//! `sandbox nuke [PROJECT]` — remove container, named volumes, and state.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use sandbox_core::{LanguageRegistry, Paths, Project};

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
