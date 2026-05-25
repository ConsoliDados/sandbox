use std::path::PathBuf;

/// Errors surfaced by the Docker adapter.
///
/// `DaemonUnreachable` is detected by sniffing `docker`'s stderr for the
/// "Cannot connect to the Docker daemon" string — the only operationally
/// useful distinction at the CLI boundary is "is the daemon up?".
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("docker daemon unreachable: {0}")]
    DaemonUnreachable(String),

    #[error("docker `{cmd}` failed (exit {code}): {stderr}")]
    DockerFailed {
        cmd: String,
        code: i32,
        stderr: String,
    },

    #[error("io error invoking docker: {source}")]
    Io {
        #[source]
        source: std::io::Error,
    },

    #[error("output of docker `{cmd}` was not valid utf-8")]
    NonUtf8Output { cmd: String },

    #[error("could not parse json output of docker `{cmd}`: {reason}")]
    InvalidJson { cmd: String, reason: String },

    #[error("could not read host user id via `id -{flag}`: {reason}")]
    UserIdLookup { flag: char, reason: String },

    #[error("dotfile not found at {0}")]
    DotfileMissing(PathBuf),

    #[error(
        "multiple compose files found — pass `--compose-file PATH` to pick one:\n{}",
        format_candidates(.candidates)
    )]
    ComposeMultipleMatches { candidates: Vec<PathBuf> },

    #[error("compose file does not exist: {path}")]
    ComposeOverrideMissing { path: PathBuf },

    #[error("compose path is not a regular file: {path}")]
    ComposeOverrideNotFile { path: PathBuf },

    #[error("compose io: {path}: {source}")]
    ComposeIo {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

fn format_candidates(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|p| format!("  - {}", p.display()))
        .collect::<Vec<_>>()
        .join("\n")
}

pub type Result<T> = std::result::Result<T, Error>;
