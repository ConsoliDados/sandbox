//! Project compose deps integration (Phase 6, ADR-0010).
//!
//! For the initial slice this module only contains [`discover`]. Parsing,
//! validation, and lifecycle (`up` / network rewire / `down`) land in later
//! Phase 6 items.

mod discover;

pub use discover::{DISCOVER_SKIP_DIRS, Outcome, discover};
