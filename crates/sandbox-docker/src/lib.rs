//! Docker adapter: shells out to `docker` and `docker compose` (see ADR-0002).
//!
//! See `crates/sandbox-docker/AGENTS.md` for boundaries and conventions.

// Phase 0: placeholder. Implementations land in Phase 1.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not implemented yet (Phase 0 skeleton)")]
    NotImplemented,
}

pub type Result<T> = std::result::Result<T, Error>;
