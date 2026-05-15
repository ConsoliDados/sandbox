//! Errors surfaced by `sandbox-scan`.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("compile yara rules: {0}")]
    YaraCompile(String),

    #[error("yara scan: {0}")]
    YaraScan(String),

    #[error("parse toml at {path}: {reason}")]
    InvalidToml { path: PathBuf, reason: String },

    #[error("compose parse at {path}: {reason}")]
    ComposeParse { path: PathBuf, reason: String },

    #[error("invalid regex `{pattern}`: {reason}")]
    InvalidRegex { pattern: String, reason: String },
}

pub type Result<T> = std::result::Result<T, Error>;
