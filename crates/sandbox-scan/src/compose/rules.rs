//! Compose security rules.
//!
//! Each rule inspects one parsed `Service` and emits zero or more `Finding`s.
//! Rules are intentionally non-clever: they check exact flags or path
//! prefixes. Anything that requires semantic reasoning about the compose
//! file (e.g. derived networks) is out of scope until we have a real
//! incident motivating it.

use std::path::{Path, PathBuf};

use super::parse::{LongVolume, Service, VolumeRef};
use crate::findings::{Finding, Severity};

/// Host paths whose mount into a container yields container-escape-grade
/// access. `/var/lib/docker` is special — it lets a hostile container
/// manipulate the daemon's own state. The list is conservative; missing a
/// path here is fine (the user audits anyway) but adding too many would
/// fire on benign devcontainer setups.
const DANGEROUS_HOST_PATHS: &[&str] = &[
    "/",
    "/var/lib/docker",
    "/var/run/docker.sock",
    "/etc",
    "/proc",
    "/sys",
    "/dev",
    "/root",
    "/boot",
];

pub(super) fn check_service(file: &Path, name: &str, service: &Service) -> Vec<Finding> {
    let mut out = Vec::new();
    if service.privileged == Some(true) {
        out.push(finding(
            "compose/privileged",
            Severity::Critical,
            file,
            format!("service `{name}` runs with `privileged: true`"),
            "Privileged containers have full host kernel access. Remove the flag \
             and grant the specific capability you need via `cap_add`.",
        ));
    }
    if matches!(service.network_mode.as_deref(), Some("host")) {
        out.push(finding(
            "compose/network_mode_host",
            Severity::Critical,
            file,
            format!("service `{name}` uses `network_mode: host`"),
            "Host networking lets the container reach localhost services and \
             bypass network isolation. Use a bridged network or remove the field.",
        ));
    }
    if matches!(service.pid.as_deref(), Some("host")) {
        out.push(finding(
            "compose/pid_host",
            Severity::Critical,
            file,
            format!("service `{name}` uses `pid: host`"),
            "Sharing the host PID namespace lets the container see and signal \
             every host process. Remove the field.",
        ));
    }
    if matches!(service.userns_mode.as_deref(), Some("host")) {
        out.push(finding(
            "compose/userns_host",
            Severity::Warn,
            file,
            format!("service `{name}` uses `userns_mode: host`"),
            "Disabling the user namespace remap defeats UID isolation. Remove \
             unless you have a documented reason.",
        ));
    }
    if let Some(caps) = &service.cap_add {
        for cap in caps {
            let upper = cap.to_uppercase();
            let severity = if upper == "SYS_ADMIN" || upper == "ALL" {
                Severity::Critical
            } else {
                Severity::High
            };
            out.push(finding(
                "compose/cap_add",
                severity,
                file,
                format!("service `{name}` adds capability `{cap}`"),
                "Granted capabilities punch holes in the kernel sandbox. Confirm \
                 each one is required and document it.",
            ));
        }
    }
    if let Some(opts) = &service.security_opt {
        for opt in opts {
            if is_disabling_opt(opt) {
                out.push(finding(
                    "compose/security_opt_unconfined",
                    Severity::Critical,
                    file,
                    format!(
                        "service `{name}` disables a security backend via `security_opt: {opt}`"
                    ),
                    "Removing seccomp/apparmor lets the container syscall freely. \
                     Restore the default unless an audited workload requires it.",
                ));
            }
        }
    }
    if let Some(vols) = &service.volumes {
        for vol in vols {
            if let Some((host, target, read_only)) = normalize_volume(vol)
                && is_dangerous_host_path(&host)
            {
                let severity = if read_only {
                    // RO mount of /etc, /var/lib/docker, etc. still leaks
                    // information but doesn't grant write access. Drop one
                    // severity tier.
                    Severity::High
                } else {
                    Severity::Critical
                };
                out.push(finding(
                    "compose/dangerous_host_mount",
                    severity,
                    file,
                    format!(
                        "service `{name}` mounts host path `{host}` -> `{target}` ({})",
                        if read_only { "read-only" } else { "read-write" }
                    ),
                    "Mounting host filesystem paths into a container defeats the \
                     point of containerization. Confine the mount to project files.",
                ));
            }
        }
    }
    out
}

fn finding(
    rule_id: &str,
    severity: Severity,
    file: &Path,
    message: String,
    remediation: &str,
) -> Finding {
    Finding {
        rule_id: rule_id.into(),
        severity,
        message,
        path: PathBuf::from(file),
        line: None,
        remediation: Some(remediation.into()),
    }
}

fn is_disabling_opt(opt: &str) -> bool {
    let lower = opt.to_ascii_lowercase();
    lower == "seccomp:unconfined"
        || lower == "apparmor:unconfined"
        || lower == "label:disable"
        || lower == "no-new-privileges:false"
}

fn normalize_volume(v: &VolumeRef) -> Option<(String, String, bool)> {
    match v {
        VolumeRef::Short(s) => parse_short_volume(s),
        VolumeRef::Long(LongVolume {
            volume_type,
            source,
            target,
            read_only,
        }) => {
            // Only bind mounts have a host path. Named volumes are safe by
            // construction (managed by Docker).
            if matches!(volume_type.as_deref(), Some("bind") | None) {
                let src = source.clone()?;
                let tgt = target.clone().unwrap_or_default();
                Some((src, tgt, read_only.unwrap_or(false)))
            } else {
                None
            }
        }
    }
}

/// Parse `"host:container[:opts]"`. Only absolute or relative paths qualify
/// as a bind; named volumes (e.g. `"my_vol:/data"`) are filtered out
/// because the "host" piece doesn't refer to a host path.
fn parse_short_volume(s: &str) -> Option<(String, String, bool)> {
    let parts: Vec<&str> = s.splitn(3, ':').collect();
    let host = (*parts.first()?).to_string();
    if !host.starts_with('/') && !host.starts_with('.') && !host.starts_with('~') {
        // Named volume — host piece is the volume name, not a path.
        return None;
    }
    let target = parts.get(1).copied().unwrap_or_default().to_string();
    let read_only = parts
        .get(2)
        .is_some_and(|opts| opts.split(',').any(|o| o == "ro"));
    Some((host, target, read_only))
}

fn is_dangerous_host_path(host: &str) -> bool {
    DANGEROUS_HOST_PATHS
        .iter()
        .any(|&p| host == p || host.starts_with(&format!("{p}/")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compose::parse::parse;

    fn fixture(body: &str) -> Vec<Finding> {
        let f = parse(Path::new("compose.yml"), body).unwrap_or_default();
        f.services
            .iter()
            .flat_map(|(name, svc)| check_service(Path::new("compose.yml"), name, svc))
            .collect()
    }

    fn rule_ids(findings: &[Finding]) -> Vec<&str> {
        let mut ids: Vec<&str> = findings.iter().map(|f| f.rule_id.as_str()).collect();
        ids.sort();
        ids.dedup();
        ids
    }

    #[test]
    fn clean_service_emits_nothing() {
        let body = r#"
services:
  app:
    image: node:24
    ports:
      - "3000:3000"
"#;
        assert!(fixture(body).is_empty());
    }

    #[test]
    fn privileged_fires_critical() {
        let f = fixture(
            r#"
services:
  app:
    image: x
    privileged: true
"#,
        );
        let summary: Vec<_> = f.iter().map(|x| (x.rule_id.as_str(), x.severity)).collect();
        assert_eq!(summary, vec![("compose/privileged", Severity::Critical)]);
    }

    #[test]
    fn network_mode_host_fires_critical() {
        let f = fixture(
            r#"
services:
  app:
    image: x
    network_mode: host
"#,
        );
        assert!(rule_ids(&f).contains(&"compose/network_mode_host"));
    }

    #[test]
    fn sys_admin_cap_critical_other_caps_high() {
        let f = fixture(
            r#"
services:
  app:
    image: x
    cap_add:
      - SYS_ADMIN
      - NET_ADMIN
"#,
        );
        let by_msg: Vec<_> = f
            .iter()
            .filter(|x| x.rule_id == "compose/cap_add")
            .map(|x| (x.severity, x.message.contains("SYS_ADMIN")))
            .collect();
        // One critical (SYS_ADMIN), one high (NET_ADMIN).
        assert!(by_msg.contains(&(Severity::Critical, true)));
        assert!(by_msg.contains(&(Severity::High, false)));
    }

    #[test]
    fn dangerous_host_mount_short_form_rw_critical() {
        let f = fixture(
            r#"
services:
  app:
    image: x
    volumes:
      - "/var/lib/docker:/host-docker"
"#,
        );
        let summary: Vec<_> = f
            .iter()
            .filter(|x| x.rule_id == "compose/dangerous_host_mount")
            .map(|x| x.severity)
            .collect();
        assert_eq!(summary, vec![Severity::Critical]);
    }

    #[test]
    fn dangerous_host_mount_ro_drops_to_high() {
        let f = fixture(
            r#"
services:
  app:
    image: x
    volumes:
      - "/etc:/host-etc:ro"
"#,
        );
        let summary: Vec<_> = f
            .iter()
            .filter(|x| x.rule_id == "compose/dangerous_host_mount")
            .map(|x| x.severity)
            .collect();
        assert_eq!(summary, vec![Severity::High]);
    }

    #[test]
    fn named_volume_is_safe() {
        let f = fixture(
            r#"
services:
  app:
    image: x
    volumes:
      - "appdata:/data"
"#,
        );
        assert!(f.is_empty(), "named volumes must not fire: {f:?}");
    }

    #[test]
    fn seccomp_unconfined_is_critical() {
        let f = fixture(
            r#"
services:
  app:
    image: x
    security_opt:
      - "seccomp:unconfined"
"#,
        );
        let summary: Vec<_> = f.iter().map(|x| (x.rule_id.as_str(), x.severity)).collect();
        assert_eq!(
            summary,
            vec![("compose/security_opt_unconfined", Severity::Critical)]
        );
    }
}
