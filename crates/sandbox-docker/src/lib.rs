//! Docker adapter: shells out to `docker` and `docker compose` (see ADR-0002).
//!
//! Public surface:
//!
//! - [`Plan`] — pure data describing a single `docker run`. Built by the CLI
//!   orchestrator from `core::Project + Profile + LangManifest`. `Display`
//!   renders it as a literal shell command for `--print-cmd`.
//! - [`lifecycle`] — container ops keyed by `ContainerName`.
//! - [`volume`] — idempotent named-volume create/remove.
//! - [`network`] — `--internal` network create + connect/disconnect.

mod cmd;
mod error;
pub mod lifecycle;
pub mod network;
mod plan;
pub mod volume;

pub use error::{Error, Result};
pub use lifecycle::{
    ContainerInfo, ExecOpts, exec, exists, is_running, list_sandboxes, list_sandboxes_args, rm,
    run, start, stop,
};
pub use network::{SANDBOX_INTERNAL, connect, disconnect, ensure_internal};
pub use plan::{Mount, NetworkSpec, Plan, ResourceSpec, SecuritySpec, UserSpec};
pub use volume::{ensure as ensure_volume, exists as volume_exists, remove as remove_volume};
