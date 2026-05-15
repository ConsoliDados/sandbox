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
    let mut ports: Vec<u16> = metas.into_iter().flat_map(|m| m.ports).collect();
    ports.sort_unstable();
    ports.dedup();

    if ports.is_empty() {
        eprintln!(
            "no project ports registered yet — bringing Traefik up with the dashboard only.\n\
             run `sandbox run --expose PORT .` first to register ports, then re-run \
             `sandbox proxy start` to refresh."
        );
    } else {
        eprintln!("registering {} port(s): {:?}", ports.len(), ports);
    }

    sandbox_docker::ensure_bridge(sandbox_proxy::PROXY_NETWORK).await?;
    let cfg = ProxyConfig { ports, dashboard };
    let compose = render_proxy(proxy_dir, &cfg)?;
    sandbox_proxy::proxy_start(&compose).await?;
    eprintln!("traefik up. routing: <slug>.sandbox.local:<PORT>");
    if dashboard {
        eprintln!(
            "dashboard at http://localhost:{} (insecure mode — local only).",
            sandbox_proxy::DASHBOARD_PORT
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
