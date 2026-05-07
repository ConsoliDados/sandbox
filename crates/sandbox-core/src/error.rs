use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("home directory could not be resolved (XDG)")]
    XdgNoHome,

    #[error("project path does not exist or is not a directory: {0}")]
    ProjectPathInvalid(PathBuf),

    #[error("invalid language manifest at {path}: {reason}")]
    InvalidManifest { path: PathBuf, reason: String },

    #[error("could not detect a language for {0}; specify --lang")]
    LanguageNotDetected(PathBuf),

    #[error("ambiguous language match for {path} (candidates: {candidates:?}); use --lang")]
    AmbiguousLanguageMatch {
        path: PathBuf,
        candidates: Vec<String>,
    },

    #[error("language not found in registry: {0}")]
    LanguageNotFound(String),
}

pub type Result<T> = std::result::Result<T, Error>;
