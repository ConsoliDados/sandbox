//! Project compose deps integration (Phase 6, ADR-0010).
//!
//! - [`discover`] finds the project's compose file.
//! - [`lifecycle`] wraps `docker compose up/down` and reads back the running
//!   service containers.
//! - The post-`up` network rewire (services moved off the compose-default
//!   bridge onto `sandbox-compose-<hash>` `--internal`) lives in
//!   [`super::network::rewire_to_internal`] / [`super::network::ensure_compose_internal`]
//!   so all network ops stay in one module.

mod discover;
pub mod lifecycle;

pub use discover::{DISCOVER_SKIP_DIRS, Outcome, discover};
pub use lifecycle::{ServiceContainer, down, services, up};
