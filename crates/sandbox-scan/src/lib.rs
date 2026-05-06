//! Security scanner: YARA rules + heuristic patterns + compose validation.
//!
//! See `crates/sandbox-scan/AGENTS.md` for boundaries and conventions.

// Phase 0: placeholder. Implementations land in Phase 4.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not implemented yet (Phase 0 skeleton)")]
    NotImplemented,
}

pub type Result<T> = std::result::Result<T, Error>;
