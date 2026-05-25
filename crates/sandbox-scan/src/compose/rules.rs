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

/// Registries we trust by default (ADR-0010 § Phase 6 registry allowlist).
/// `docker.io/library/*` is the namespace reserved for official Docker
/// images; `ghcr.io` is GitHub's per-user/org registry, where the namespace
/// itself is account-controlled (i.e. typo-squatting requires controlling
/// the github org). Other registries are out of the v0.1 default; users
/// extend the list via config when wired (Phase 6 follow-up).
const ALLOWED_DOCKER_IO_NAMESPACES: &[&str] = &["library"];
const ALLOWED_REGISTRIES_ANY_NAMESPACE: &[&str] = &["ghcr.io"];

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
    if let Some(image) = service.image.as_deref()
        && let Some(reason) = is_registry_disallowed(image)
    {
        out.push(finding(
            "compose/registry_not_allowed",
            Severity::High,
            file,
            format!("service `{name}` uses image `{image}`: {reason}"),
            "Only docker.io/library/* and ghcr.io/* are allowed by default. \
             If this registry/namespace is intentional, extend the allowlist \
             via `[scan.compose] allowed_registries = [...]` in config \
             (planned), or override with `--unsafe` for this run.",
        ));
    }
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

/// Returns a human-readable reason if the image's registry/namespace is
/// outside the default allowlist; `None` if the image is allowed.
///
/// Docker image refs are flexible (`postgres`, `library/postgres`,
/// `ghcr.io/owner/repo`, `gcr.io/proj/repo:tag@sha256:...`). We strip the
/// tag/digest and infer the registry by looking at the first path segment:
/// if it contains `.` or `:` (port) it's a hostname; otherwise the implicit
/// registry is `docker.io`.
fn is_registry_disallowed(image: &str) -> Option<String> {
    let (registry, namespace) = parse_image_ref(image);
    if registry == "docker.io" {
        if ALLOWED_DOCKER_IO_NAMESPACES.contains(&namespace.as_str()) {
            return None;
        }
        return Some(format!(
            "docker.io namespace `{namespace}` is not in the default allowlist \
             (only `library/*` for official images)"
        ));
    }
    if ALLOWED_REGISTRIES_ANY_NAMESPACE.contains(&registry.as_str()) {
        return None;
    }
    Some(format!(
        "registry `{registry}` is not in the default allowlist"
    ))
}

/// Returns `(registry, namespace)` for a docker image reference. Tag and
/// digest are discarded — they don't affect registry policy.
///
/// Compose accepts these shapes for `image:`:
/// - `postgres` / `postgres:15` → docker.io/library/postgres
/// - `library/postgres` → docker.io/library/postgres
/// - `owner/repo[:tag]` → docker.io/owner/repo
/// - `host[:port]/path/repo[:tag][@digest]` → host as registry
fn parse_image_ref(image: &str) -> (String, String) {
    let without_digest = image.split_once('@').map_or(image, |(head, _)| head);
    let parts: Vec<&str> = without_digest.split('/').collect();
    match parts.as_slice() {
        [] | [""] => ("docker.io".into(), "library".into()),
        [_single] => ("docker.io".into(), "library".into()),
        [namespace, _repo] => ("docker.io".into(), (*namespace).to_string()),
        [first, rest @ ..] => {
            // The first segment is a registry only if it looks like a
            // hostname: contains `.` or a port (`:`). Otherwise compose
            // treats the whole path as a docker.io namespace+repo.
            if first.contains('.') || first.contains(':') {
                let namespace = rest
                    .split_last()
                    .map(|(_, ns)| ns.join("/"))
                    .unwrap_or_default();
                ((*first).to_string(), namespace)
            } else {
                ("docker.io".into(), (*first).to_string())
            }
        }
    }
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

    // --- registry allowlist (Phase 6, ADR-0010) -----------------------------

    #[test]
    fn allowlist_passes_official_library_images() {
        // Single-segment and explicit library/ — both resolve to
        // docker.io/library and must pass.
        for img in ["postgres", "postgres:15", "library/postgres", "node:24"] {
            let body = format!("services:\n  s:\n    image: {img}\n");
            let f = fixture(&body);
            assert!(
                !rule_ids(&f).contains(&"compose/registry_not_allowed"),
                "{img} should be allowed but fired the rule: {f:?}"
            );
        }
    }

    #[test]
    fn allowlist_passes_ghcr_images_in_any_namespace() {
        for img in ["ghcr.io/owner/repo", "ghcr.io/owner/repo:tag"] {
            let body = format!("services:\n  s:\n    image: {img}\n");
            let f = fixture(&body);
            assert!(
                !rule_ids(&f).contains(&"compose/registry_not_allowed"),
                "{img} should be allowed but fired the rule: {f:?}"
            );
        }
    }

    #[test]
    fn allowlist_flags_unknown_docker_io_namespace() {
        // docker.io is allowed but only the `library` namespace; an arbitrary
        // owner namespace is the typo-squat / impersonation surface.
        let f = fixture("services:\n  s:\n    image: attacker/postgres:15\n");
        let summary: Vec<_> = f
            .iter()
            .filter(|x| x.rule_id == "compose/registry_not_allowed")
            .map(|x| (x.severity, x.message.clone()))
            .collect();
        let first = summary.first().cloned();
        assert_eq!(summary.len(), 1, "expected exactly one finding: {f:?}");
        let (sev, msg) = first.unwrap_or((Severity::Info, String::new()));
        assert_eq!(sev, Severity::High);
        assert!(msg.contains("attacker"));
    }

    #[test]
    fn allowlist_flags_third_party_registry() {
        let f = fixture("services:\n  s:\n    image: gcr.io/some-project/image:tag\n");
        assert!(
            rule_ids(&f).contains(&"compose/registry_not_allowed"),
            "gcr.io should fire the rule: {f:?}"
        );
    }

    #[test]
    fn allowlist_ignores_tag_and_digest_when_classifying() {
        // The digest must not be treated as part of the path; we should still
        // classify the registry correctly and allow this image.
        let f = fixture(
            "services:\n  s:\n    image: library/postgres:15@sha256:0000000000000000000000000000000000000000000000000000000000000000\n",
        );
        assert!(
            !rule_ids(&f).contains(&"compose/registry_not_allowed"),
            "digest should not affect registry classification: {f:?}"
        );
    }
}
