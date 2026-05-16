//! Traefik reverse proxy adapter: sidecar lifecycle + label generation.
//!
//! See `crates/sandbox-proxy/AGENTS.md` for boundaries and conventions.

pub mod error;
pub mod labels;
pub mod ports;
pub mod traefik;

pub use error::{Error, Result};
pub use labels::{DEFAULT_DOMAIN, for_project as labels_for_project, slug_from_path};
pub use ports::detect as detect_ports;
pub use traefik::{
    COMPOSE_PROJECT, DASHBOARD_PORT, PROXY_NETWORK, ProxyConfig, detect_docker_api_version,
    is_running as proxy_running, logs as proxy_logs, render as render_proxy,
    render_project_dynamic, start as proxy_start, status as proxy_status, stop as proxy_stop,
    write_dynamic_configs,
};
