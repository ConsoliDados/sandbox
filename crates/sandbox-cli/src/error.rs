//! Error type for the `sandbox` binary.
//!
//! Composes the four library crates' errors via `#[from]` variants. See
//! ADR-0011 for the rationale (no `anyhow`).

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error(transparent)]
    Core(#[from] sandbox_core::Error),

    #[error(transparent)]
    Docker(#[from] sandbox_docker::Error),

    #[error(transparent)]
    Scan(#[from] sandbox_scan::Error),

    #[error(transparent)]
    Proxy(#[from] sandbox_proxy::Error),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("clap: {0}")]
    Clap(#[from] clap::Error),

    #[error("no sandbox container for `{name}` (run `sandbox run` first)")]
    ContainerNotFound { name: String },

    #[error("sandbox container `{name}` is not running (run `sandbox run` first)")]
    ContainerNotRunning { name: String },

    #[error("not implemented yet (Phase 0 skeleton); see roadmap")]
    NotImplemented,
}

impl Error {
    /// Exit code per `docs/sandbox/srs.md` § Global. Unmapped variants fall
    /// through to the generic `1`.
    pub(crate) fn exit_code(&self) -> i32 {
        match self {
            Error::Clap(_) => 2,
            Error::ContainerNotFound { .. } | Error::ContainerNotRunning { .. } => 40,
            _ => 1,
        }
    }
}

pub(crate) type Result<T> = std::result::Result<T, Error>;
