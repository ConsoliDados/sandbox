//! `docker compose` lifecycle for project deps (ADR-0010 § Decision items 4-7).
//!
//! Three operations:
//! - [`up`] — `docker compose -p NAME -f PATH up -d`.
//! - [`services`] — read back the service names + container IDs that `up`
//!   produced; needed for the post-up network rewire.
//! - [`down`] — `docker compose -p NAME down`; idempotent.
//!
//! The rewire (moving service containers off the compose-created default
//! network onto a `--internal` `sandbox-compose-<hash>` so deps inherit the
//! sandbox's egress policy) lives in [`super::network::rewire_to_internal`].

use crate::cmd::run_capture;
use crate::{Error, Result};

/// Run `docker compose -p <project> -f <file> up -d`.
///
/// `project_name` namespaces the compose project so deps don't collide with
/// whatever the user may be running by hand. Conventional naming:
/// `sandbox-<short_hash>-deps`.
pub async fn up(file_path: &str, project_name: &str) -> Result<()> {
    run_capture(&["compose", "-p", project_name, "-f", file_path, "up", "-d"]).await?;
    Ok(())
}

/// `docker compose -p NAME down` — removes containers and the compose-managed
/// network. Idempotent: if the project isn't running, compose treats it as a
/// no-op.
pub async fn down(project_name: &str) -> Result<()> {
    run_capture(&["compose", "-p", project_name, "down"]).await?;
    Ok(())
}

/// One service container brought up by `docker compose`. The pair is what
/// the post-up rewire needs: `service` becomes the network alias on the
/// rewired internal network so sibling services (and the sandbox container)
/// can still reach `postgres`, `redis`, etc. by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceContainer {
    pub service: String,
    pub container_id: String,
}

/// Enumerate `(service_name, container_id)` for every service in the compose
/// project. Empty when the project has nothing running.
pub async fn services(project_name: &str) -> Result<Vec<ServiceContainer>> {
    let stdout = run_capture(&[
        "compose",
        "-p",
        project_name,
        "ps",
        "--format",
        "{{.Service}}\t{{.ID}}",
    ])
    .await?;
    parse_services_output(project_name, &stdout)
}

fn parse_services_output(project_name: &str, stdout: &str) -> Result<Vec<ServiceContainer>> {
    let mut out = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((service, container_id)) = trimmed.split_once('\t') else {
            return Err(Error::InvalidJson {
                cmd: format!("compose -p {project_name} ps"),
                reason: format!("expected `<service>\\t<id>`, got `{trimmed}`"),
            });
        };
        out.push(ServiceContainer {
            service: service.to_string(),
            container_id: container_id.to_string(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn parses_typical_compose_ps_output() -> TestResult {
        let stdout = "postgres\tabc123\nredis\tdef456\n";
        let parsed = parse_services_output("proj", stdout)?;
        assert_eq!(
            parsed,
            vec![
                ServiceContainer {
                    service: "postgres".into(),
                    container_id: "abc123".into(),
                },
                ServiceContainer {
                    service: "redis".into(),
                    container_id: "def456".into(),
                },
            ]
        );
        Ok(())
    }

    #[test]
    fn empty_output_yields_empty_vec() -> TestResult {
        assert!(parse_services_output("proj", "")?.is_empty());
        Ok(())
    }

    #[test]
    fn ignores_blank_lines() -> TestResult {
        let stdout = "\n\npostgres\tabc123\n\n";
        let parsed = parse_services_output("proj", stdout)?;
        assert_eq!(parsed.len(), 1);
        Ok(())
    }

    #[test]
    fn malformed_line_returns_invalid_json_error() {
        // Missing tab — can't split into service + id pair.
        let stdout = "postgres abc123\n";
        let err = parse_services_output("proj", stdout).err();
        assert!(matches!(err, Some(Error::InvalidJson { .. })));
    }
}
