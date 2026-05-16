//! `sandbox proxy start|stop|status|logs` — Traefik sidecar control.
//!
//! `start` is the only flag-bearing variant: it reads `Meta::load_all()` to
//! collect the union of ports across active projects, renders the proxy
//! compose + static config, and brings the sidecar up. `stop` brings it
//! down, `status` prints `docker compose ps`, `logs` streams.

use sandbox_core::{Meta, Paths};
use sandbox_proxy::{ProxyConfig, render_proxy};

use crate::Result;

#[derive(Debug, Clone)]
pub(crate) enum Args {
    Start { dashboard: bool },
    Stop,
    Status,
    Logs { follow: bool },
}

impl From<crate::ProxyOp> for Args {
    fn from(op: crate::ProxyOp) -> Self {
        match op {
            crate::ProxyOp::Start { dashboard } => Args::Start { dashboard },
            crate::ProxyOp::Stop => Args::Stop,
            crate::ProxyOp::Status => Args::Status,
            crate::ProxyOp::Logs { follow } => Args::Logs { follow },
        }
    }
}

pub(crate) async fn execute(args: Args) -> Result<()> {
    let paths = Paths::discover()?;
    paths.ensure_dirs()?;
    let proxy_dir = paths.proxy_dir();

    match args {
        Args::Start { dashboard } => start(&paths, &proxy_dir, dashboard).await,
        Args::Stop => stop(&proxy_dir).await,
        Args::Status => status(&proxy_dir).await,
        Args::Logs { follow } => logs(&proxy_dir, follow).await,
    }
}

async fn start(paths: &Paths, proxy_dir: &std::path::Path, dashboard: bool) -> Result<()> {
    let metas = Meta::load_all(&paths.containers_dir())?;
    // Collect (slug, container_name, ports) per project so the dynamic
    // config can point each port at the right backend container.
    let projects: Vec<(String, String, Vec<u16>)> = metas
        .iter()
        .filter(|m| !m.ports.is_empty())
        .map(|m| {
            let slug = sandbox_proxy::slug_from_path(&m.project_path);
            (slug, m.container_name.clone(), m.ports.clone())
        })
        .collect();
    let mut ports: Vec<u16> = projects
        .iter()
        .flat_map(|(_, _, p)| p.iter())
        .copied()
        .collect();
    ports.sort_unstable();
    ports.dedup();

    if projects.is_empty() {
        eprintln!(
            "no project ports registered yet — bringing Traefik up bare.\n\
             run `sandbox run --expose PORT .` first to register ports, then re-run \
             `sandbox proxy start` to refresh."
        );
    } else {
        let summary: Vec<String> = projects
            .iter()
            .map(|(slug, _, p)| format!("{slug} → {p:?}"))
            .collect();
        eprintln!(
            "registering {} project(s): {}",
            projects.len(),
            summary.join(", ")
        );
    }

    sandbox_docker::ensure_bridge(sandbox_proxy::PROXY_NETWORK).await?;
    let cfg = ProxyConfig {
        ports,
        dashboard,
        docker_api_version: None,
    };
    let compose = render_proxy(proxy_dir, &cfg)?;
    // Materialize per-project dynamic configs from the metas. File provider
    // picks them up on next watch tick (or at startup if proxy was down).
    sandbox_proxy::write_dynamic_configs(proxy_dir, projects, sandbox_proxy::DEFAULT_DOMAIN)?;
    sandbox_proxy::proxy_start(&compose).await?;
    eprintln!("traefik up. routing: <slug>.sandbox.local:<PORT>");
    if dashboard {
        eprintln!(
            "dashboard at http://localhost:{port}/dashboard/ \
             (api at /api/version, insecure mode — local only).",
            port = sandbox_proxy::DASHBOARD_PORT,
        );
    }
    eprintln!(
        "tip: add `127.0.0.1 *.sandbox.local` to /etc/hosts (or configure dnsmasq) \
         so the slug resolves."
    );
    Ok(())
}

async fn stop(proxy_dir: &std::path::Path) -> Result<()> {
    let compose = proxy_dir.join("docker-compose.yml");
    if !compose.is_file() {
        eprintln!(
            "no proxy compose file at {} — nothing to stop",
            compose.display()
        );
        return Ok(());
    }
    sandbox_proxy::proxy_stop(&compose).await?;
    eprintln!("traefik down.");
    Ok(())
}

async fn status(proxy_dir: &std::path::Path) -> Result<()> {
    let compose = proxy_dir.join("docker-compose.yml");
    if !compose.is_file() {
        println!("not configured (run `sandbox proxy start` to bootstrap)");
        return Ok(());
    }
    let running = sandbox_proxy::proxy_running().await?;
    let out = sandbox_proxy::proxy_status(&compose).await?;
    if !out.trim().is_empty() {
        print!("{out}");
    }
    println!("state: {}", if running { "running" } else { "stopped" });
    Ok(())
}

async fn logs(proxy_dir: &std::path::Path, follow: bool) -> Result<()> {
    let compose = proxy_dir.join("docker-compose.yml");
    if !compose.is_file() {
        return Err(crate::Error::ProxyNotConfigured);
    }
    sandbox_proxy::proxy_logs(&compose, follow).await?;
    Ok(())
}
