//! Traefik docker-label generation for project containers.
//!
//! Per ADR-0005 each detected port becomes a Traefik entryPoint named
//! `port-<port>`, and each port routes to the same container via
//! `Host(<proj>.<domain>)`. The static entryPoint config is rendered in
//! `traefik.rs`; here we emit the per-container labels Docker reads.
//!
//! Naming rules:
//!
//! - `proj_name` is the user-friendly slug (typically `path.file_name()`)
//!   sanitized to `[a-z0-9-]` so it's a valid Host header component.
//! - Router/service identifiers combine the slug with the port so the same
//!   container can serve multiple ports (`<slug>-3000`, `<slug>-5007`).
//! - Output is alphabetically stable for deterministic Plan rendering.

use std::path::Path;

/// Default Traefik proxy domain. Override via config (Phase 7).
pub const DEFAULT_DOMAIN: &str = "sandbox.local";

/// Generate `--label k=v` pairs for a project container with the given
/// detected ports. Returns an empty vec when `ports` is empty (no labels =
/// Traefik ignores the container, which is the correct behavior for
/// projects that don't expose anything).
pub fn for_project(proj_name: &str, ports: &[u16], domain: &str) -> Vec<(String, String)> {
    let slug = sanitize_slug(proj_name);
    if ports.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<(String, String)> = Vec::with_capacity(1 + ports.len() * 4);
    out.push(("traefik.enable".into(), "true".into()));
    for port in ports {
        let id = format!("sb-{slug}-{port}");
        out.push((
            format!("traefik.http.routers.{id}.rule"),
            format!("Host(`{slug}.{domain}`)"),
        ));
        out.push((
            format!("traefik.http.routers.{id}.entrypoints"),
            format!("port-{port}"),
        ));
        out.push((format!("traefik.http.routers.{id}.service"), id.clone()));
        out.push((
            format!("traefik.http.services.{id}.loadbalancer.server.port"),
            port.to_string(),
        ));
    }
    out.sort();
    out
}

/// Derive a slug suitable for Host headers from a project path. Uses the
/// path's final component (e.g. `/home/me/my-project` → `my-project`),
/// lowercased and stripped to `[a-z0-9-]`. Anything that doesn't reduce to
/// at least one valid char falls back to `"project"` — better to render a
/// generic Host than fail the run on a weird path.
pub fn slug_from_path(path: &Path) -> String {
    let raw = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".to_string());
    let slug = sanitize_slug(&raw);
    if slug.is_empty() {
        "project".to_string()
    } else {
        slug
    }
}

fn sanitize_slug(input: &str) -> String {
    let lowered: String = input
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    // Collapse runs of `-`, trim leading/trailing `-`.
    let mut out = String::with_capacity(lowered.len());
    let mut prev_dash = true;
    for c in lowered.chars() {
        if c == '-' {
            if !prev_dash {
                out.push('-');
                prev_dash = true;
            }
        } else {
            out.push(c);
            prev_dash = false;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn no_ports_emits_no_labels() {
        assert!(for_project("myproj", &[], DEFAULT_DOMAIN).is_empty());
    }

    #[test]
    fn single_port_emits_enable_plus_four_router_service_labels() {
        let labels = for_project("myproj", &[3000], DEFAULT_DOMAIN);
        let keys: Vec<_> = labels.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(labels.len(), 5);
        assert!(keys.contains(&"traefik.enable"));
        assert!(
            keys.iter()
                .any(|k| k.starts_with("traefik.http.routers.sb-myproj-3000.rule"))
        );
        assert!(
            keys.iter()
                .any(|k| k.starts_with("traefik.http.routers.sb-myproj-3000.entrypoints"))
        );
        assert!(
            keys.iter()
                .any(|k| k.starts_with("traefik.http.routers.sb-myproj-3000.service"))
        );
        assert!(keys.iter().any(|k| {
            k.starts_with("traefik.http.services.sb-myproj-3000.loadbalancer.server.port")
        }));
    }

    #[test]
    fn multi_port_renders_one_router_per_port() {
        let labels = for_project("myproj", &[3000, 5007], DEFAULT_DOMAIN);
        // 1 enable + (4 per port * 2) = 9
        assert_eq!(labels.len(), 9);
        let host_rules: Vec<_> = labels
            .iter()
            .filter(|(k, _)| k.ends_with(".rule"))
            .map(|(_, v)| v.as_str())
            .collect();
        // Both routers point at the SAME host — the port distinguishes them.
        assert_eq!(
            host_rules,
            vec![
                "Host(`myproj.sandbox.local`)",
                "Host(`myproj.sandbox.local`)",
            ]
        );
    }

    #[test]
    fn labels_are_sorted_deterministically() {
        let labels = for_project("myproj", &[5007, 3000], DEFAULT_DOMAIN);
        let keys: Vec<_> = labels.iter().map(|(k, _)| k.clone()).collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
    }

    #[test]
    fn sanitize_slug_handles_dots_and_underscores() {
        assert_eq!(sanitize_slug("My.Project_Name"), "my-project-name");
    }

    #[test]
    fn sanitize_slug_collapses_consecutive_separators() {
        assert_eq!(sanitize_slug("foo!!bar___baz"), "foo-bar-baz");
    }

    #[test]
    fn sanitize_slug_trims_trailing_dash() {
        assert_eq!(sanitize_slug("---foo---"), "foo");
    }

    #[test]
    fn slug_from_path_uses_final_component() {
        assert_eq!(slug_from_path(&PathBuf::from("/home/me/MyProj")), "myproj");
    }

    #[test]
    fn slug_from_path_falls_back_when_unparseable() {
        assert_eq!(slug_from_path(&PathBuf::from("/")), "project");
        assert_eq!(slug_from_path(&PathBuf::from("---")), "project");
    }

    #[test]
    fn custom_domain_appears_in_host_rule() {
        let labels = for_project("foo", &[8080], "dev.local");
        let host = labels
            .iter()
            .find(|(k, _)| k.ends_with(".rule"))
            .map(|(_, v)| v.as_str());
        assert_eq!(host, Some("Host(`foo.dev.local`)"));
    }
}
