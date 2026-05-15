//! Traefik reverse proxy adapter: sidecar lifecycle + label generation.
//!
//! See `crates/sandbox-proxy/AGENTS.md` for boundaries and conventions.

pub mod error;
pub mod labels;
pub mod ports;

pub use error::{Error, Result};
pub use labels::{DEFAULT_DOMAIN, for_project as labels_for_project, slug_from_path};
pub use ports::detect as detect_ports;
