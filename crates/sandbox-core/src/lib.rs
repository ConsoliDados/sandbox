//! Foundational domain types for the sandbox tool.
//!
//! See `crates/sandbox-core/AGENTS.md` for boundaries and conventions.

// Phase 0: module placeholders. Implementations land in Phase 1+.
// See docs/roadmap.md.

pub mod error {
    /// Error type for `sandbox-core`. Phase 0 placeholder.
    #[derive(Debug, thiserror::Error)]
    pub enum Error {
        #[error("not implemented yet (Phase 0 skeleton)")]
        NotImplemented,
    }

    pub type Result<T> = std::result::Result<T, Error>;
}

pub use error::{Error, Result};
