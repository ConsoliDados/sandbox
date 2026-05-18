//! `sandbox net on|off|status PROJECT` — runtime egress toggle.
//!
//! ADR-0004 pins the mechanism: every sandbox container's primary network is
//! `sandbox-internal` (`--internal`, no egress). Egress is granted at runtime
//! by additionally attaching Docker's default `bridge` network via
//! `docker network connect`. `off` disconnects it.
//!
//! v0.1 semantics: **ephemeral**. The toggle does not persist across container
//! recreation — a fresh `sandbox run` always starts back on `sandbox-internal`
//! only. Persisting would mean either re-attaching automatically on every
//! `run` (surprising) or storing intent in `Meta` and surfacing it in `ps`
//! (more bookkeeping). Revisit if real workflows ask for it.

use std::path::PathBuf;

use sandbox_core::{ContainerName, LanguageRegistry, Project};
use sandbox_docker::BRIDGE;
use serde::Serialize;

use crate::{Error, Result};

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub(crate) enum Format {
    Table,
    Json,
}

#[derive(Debug)]
pub(crate) enum Args {
    On { project: String },
    Off { project: String },
    Status { project: String, format: Format },
}

pub(crate) async fn execute(args: Args) -> Result<()> {
    match args {
        Args::On { project } => on(&project).await,
        Args::Off { project } => off(&project).await,
        Args::Status { project, format } => status(&project, format).await,
    }
}

async fn on(project_arg: &str) -> Result<()> {
    let project = resolve_project(project_arg)?;
    let name = project.container_name.as_str();
    ensure_running(&project.container_name).await?;

    // Idempotent: if bridge is already attached we report the no-op instead
    // of letting `docker network connect` error with "endpoint already
    // exists". The check is one round-trip against `docker inspect` which is
    // cheap.
    let nets = sandbox_docker::inspect_networks(name).await?;
    if nets.iter().any(|n| n == BRIDGE) {
        println!("{name}: egress already on (bridge already attached)");
        return Ok(());
    }

    sandbox_docker::connect(BRIDGE, name).await?;
    println!("{name}: egress on (attached bridge)");
    Ok(())
}

async fn off(project_arg: &str) -> Result<()> {
    let project = resolve_project(project_arg)?;
    let name = project.container_name.as_str();
    ensure_running(&project.container_name).await?;

    // Refuse if bridge is the container's *only* network — that would be an
    // `--unsafe` container whose primary is bridge. Disconnecting would
    // strand the container with no networking, which is almost certainly
    // not what the user wants. They should `sandbox down` instead.
    let nets = sandbox_docker::inspect_networks(name).await?;
    if !nets.iter().any(|n| n == BRIDGE) {
        println!("{name}: egress already off (bridge not attached)");
        return Ok(());
    }
    if matches!(nets.as_slice(), [only] if only == BRIDGE) {
        return Err(Error::NetOffWouldStrand {
            name: name.to_string(),
        });
    }

    sandbox_docker::disconnect(BRIDGE, name).await?;
    println!("{name}: egress off (detached bridge)");
    Ok(())
}

async fn status(project_arg: &str, format: Format) -> Result<()> {
    let project = resolve_project(project_arg)?;
    let name = project.container_name.as_str();
    if !sandbox_docker::exists(&project.container_name).await? {
        return Err(Error::ContainerNotFound {
            name: name.to_string(),
        });
    }

    let mut nets = sandbox_docker::inspect_networks(name).await?;
    nets.sort();
    let report = Report {
        container: name.to_string(),
        egress: nets.iter().any(|n| n == BRIDGE),
        networks: nets,
    };

    match format {
        Format::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        Format::Table => print!("{}", render_table(&report)),
    }
    Ok(())
}

fn resolve_project(arg: &str) -> Result<Project> {
    // For now PROJECT is interpreted as a path (same path-only resolution
    // `down` uses). Hash-prefix / alias lookup is a Phase 6+ enhancement
    // tracked under SRS § Project resolution rules.
    let path = match arg {
        "." | "" => PathBuf::from("."),
        other => PathBuf::from(other),
    };
    let registry = LanguageRegistry::builtin()?;
    Ok(Project::resolve(&path, &registry, None)?)
}

async fn ensure_running(name: &ContainerName) -> Result<()> {
    if !sandbox_docker::exists(name).await? {
        return Err(Error::ContainerNotFound {
            name: name.as_str().to_string(),
        });
    }
    if !sandbox_docker::is_running(name).await? {
        return Err(Error::ContainerNotRunning {
            name: name.as_str().to_string(),
        });
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct Report {
    container: String,
    egress: bool,
    networks: Vec<String>,
}

fn render_table(report: &Report) -> String {
    // Three columns: NETWORK | EGRESS | ROLE. ROLE annotates the well-known
    // networks (sandbox-internal = primary, bridge = egress, sandbox-proxy =
    // inbound routing) so the user can read intent off the table without
    // memorizing the names.
    let mut rows: Vec<[String; 3]> = report
        .networks
        .iter()
        .map(|net| {
            let role = role_for(net);
            let egress = if net == BRIDGE { "yes" } else { "no" };
            [net.clone(), egress.into(), role.into()]
        })
        .collect();
    if rows.is_empty() {
        rows.push(["—".into(), "no".into(), "detached".into()]);
    }

    let headers = ["NETWORK", "EGRESS", "ROLE"];
    let mut widths = headers.map(str::len);
    for row in &rows {
        for (slot, cell) in widths.iter_mut().zip(row.iter()) {
            *slot = (*slot).max(cell.chars().count());
        }
    }

    let mut out = String::new();
    write_line(&mut out, headers.iter().copied(), &widths);
    for row in &rows {
        write_line(&mut out, row.iter().map(String::as_str), &widths);
    }
    let summary = if report.egress {
        "egress: ON"
    } else {
        "egress: off"
    };
    out.push_str(&format!("\n{}: {summary}\n", report.container));
    out
}

fn role_for(network: &str) -> &'static str {
    match network {
        sandbox_docker::SANDBOX_INTERNAL => "primary (no egress)",
        BRIDGE => "egress (toggle)",
        "sandbox-proxy" => "inbound (Traefik)",
        _ => "other",
    }
}

fn write_line<'a>(out: &mut String, cells: impl Iterator<Item = &'a str>, widths: &[usize; 3]) {
    for (i, (cell, width)) in cells.zip(widths.iter()).enumerate() {
        if i > 0 {
            out.push_str("  ");
        }
        out.push_str(cell);
        let pad = width.saturating_sub(cell.chars().count());
        for _ in 0..pad {
            out.push(' ');
        }
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_for_known_networks() {
        assert_eq!(role_for("sandbox-internal"), "primary (no egress)");
        assert_eq!(role_for("bridge"), "egress (toggle)");
        assert_eq!(role_for("sandbox-proxy"), "inbound (Traefik)");
        assert_eq!(role_for("custom-net"), "other");
    }

    #[test]
    fn render_table_marks_bridge_as_egress() {
        let report = Report {
            container: "sandbox-abc123".into(),
            egress: true,
            networks: vec!["bridge".into(), "sandbox-internal".into()],
        };
        let out = render_table(&report);
        assert!(out.starts_with("NETWORK"));
        assert!(out.contains("bridge"));
        assert!(out.contains("yes"));
        assert!(out.contains("primary (no egress)"));
        assert!(out.contains("egress: ON"));
    }

    #[test]
    fn render_table_reports_off_when_only_internal() {
        let report = Report {
            container: "sandbox-abc123".into(),
            egress: false,
            networks: vec!["sandbox-internal".into()],
        };
        let out = render_table(&report);
        assert!(out.contains("sandbox-internal"));
        assert!(out.contains("no"));
        assert!(out.contains("egress: off"));
    }

    #[test]
    fn render_table_handles_detached_container() {
        let report = Report {
            container: "sandbox-abc123".into(),
            egress: false,
            networks: vec![],
        };
        let out = render_table(&report);
        assert!(out.contains("detached"));
        assert!(out.contains("egress: off"));
    }
}
