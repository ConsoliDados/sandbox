//! Traefik reverse proxy adapter: sidecar lifecycle + label generation.
//!
//! See `crates/sandbox-proxy/AGENTS.md` for boundaries and conventions.

pub mod error;
pub mod ports;

pub use error::{Error, Result};
pub use ports::detect as detect_ports;
