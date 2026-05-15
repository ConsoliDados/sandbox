//! Errors surfaced by `sandbox-proxy`.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid regex `{pattern}`: {reason}")]
    InvalidRegex { pattern: String, reason: String },

    #[error("traefik sidecar: {0}")]
    TraefikLifecycle(String),

    #[error("port {port} requested by `{project}` is already bound by `{owner}`")]
    PortConflict {
        port: u16,
        project: String,
        owner: String,
    },
}

pub type Result<T> = std::result::Result<T, Error>;
