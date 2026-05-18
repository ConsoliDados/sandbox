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
pub mod compose;
mod error;
pub mod lifecycle;
pub mod network;
mod plan;
pub mod scanner;
pub mod volume;

pub use compose::{
    Outcome as ComposeOutcome, ServiceContainer, discover as discover_compose,
    down as compose_down, lifecycle as compose_lifecycle, services as compose_services,
    up as compose_up,
};
pub use error::{Error, Result};
pub use lifecycle::{
    ContainerInfo, ExecOpts, LogsOpts, exec, exists, is_running, list_sandboxes,
    list_sandboxes_args, logs, logs_args, rm, run, start, stop,
};
pub use network::{
    BRIDGE, SANDBOX_INTERNAL, compose_internal_name, connect, disconnect, ensure_bridge,
    ensure_compose_internal, ensure_internal, inspect_networks, rewire_to_internal,
    rm as network_rm,
};
pub use plan::{Mount, NetworkSpec, Plan, ResourceSpec, SecuritySpec, UserSpec};
pub use scanner::{
    ClamavOutcome, SCANNER_DB_VOLUME, SCANNER_IMAGE, build_image as build_scanner_image,
    clamscan_argv, db_volume_exists, ensure_image as ensure_scanner_image, freshclam_argv,
    image_exists as scanner_image_exists, run_clamscan, run_freshclam,
};
pub use volume::{
    ensure as ensure_volume, ensure_owned as ensure_volume_owned, exists as volume_exists,
    remove as remove_volume,
};
