//! Port detection — decides which ports a project listens on so the proxy
//! can expose them.
//!
//! Resolution order (per ADR-0005 § Decision):
//!
//! 1. CLI override (`--expose PORT...`) — the caller passes them in, this
//!    module isn't involved.
//! 2. Heuristics: regex over source via `port_detection.patterns` + key
//!    lookup in `.env` via `port_detection.env_keys`.
//! 3. Manifest `default_port` as a last resort.
//!
//! Results are deduplicated and sorted ascending for stable output.

mod env;
mod source;

use std::path::Path;

use sandbox_core::{LangManifest, PortDetection};

use crate::Result;

/// Detect listening ports for `project_root` using the manifest's heuristic
/// patterns + .env key allowlist, falling back to `manifest.default_port`.
/// Pass `overrides` to short-circuit detection entirely (CLI's `--expose`).
pub fn detect(project_root: &Path, manifest: &LangManifest, overrides: &[u16]) -> Result<Vec<u16>> {
    if !overrides.is_empty() {
        return Ok(dedup_sorted(overrides));
    }

    let mut found: Vec<u16> = Vec::new();
    if let Some(pd) = &manifest.port_detection {
        found.extend(env::parse_env_file(project_root, &pd.env_keys)?);
        found.extend(source::scan_sources(project_root, &pd.patterns)?);
    }
    let result = dedup_sorted(&found);
    if !result.is_empty() {
        return Ok(result);
    }
    if let Some(p) = manifest.default_port {
        return Ok(vec![p]);
    }
    Ok(Vec::new())
}

/// Module-level helper exposed for callers that want only the cap-list of
/// supported keys without running detection.
pub fn env_keys(manifest: &LangManifest) -> Vec<String> {
    manifest
        .port_detection
        .as_ref()
        .map(|pd: &PortDetection| pd.env_keys.clone())
        .unwrap_or_default()
}

fn dedup_sorted(input: &[u16]) -> Vec<u16> {
    let mut v: Vec<u16> = input.to_vec();
    v.sort_unstable();
    v.dedup();
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use sandbox_core::{LangManifest, LanguageRegistry};

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    // Pull the real builtin node manifest through the registry instead of
    // include_str!'ing a file outside this crate (which breaks `cargo publish`
    // and the packaged crate). The builtins now live in `sandbox-core`.
    fn parse_node_manifest() -> std::result::Result<LangManifest, sandbox_core::Error> {
        Ok(LanguageRegistry::builtin()?.require("node")?.clone())
    }

    #[test]
    fn cli_overrides_short_circuit_detection() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let manifest = parse_node_manifest()?;
        // No .env, no source — without overrides we'd fall through to
        // default_port (3000 in node.toml).
        let ports = detect(tmp.path(), &manifest, &[8080, 5007, 8080])?;
        assert_eq!(ports, vec![5007, 8080]); // dedup + sort
        Ok(())
    }

    #[test]
    fn falls_back_to_default_port_when_nothing_detected() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let manifest = parse_node_manifest()?;
        // Pure tmpdir → no env, no source matches → fall back to manifest default.
        let ports = detect(tmp.path(), &manifest, &[])?;
        assert_eq!(ports, vec![3000]);
        Ok(())
    }

    #[test]
    fn env_and_source_combine_dedup_sort() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let manifest = parse_node_manifest()?;
        std::fs::write(tmp.path().join(".env"), "PORT=5007\n")?;
        std::fs::write(
            tmp.path().join("server.js"),
            b"app.listen(3000);\nserver.listen(5007);\n",
        )?;
        let ports = detect(tmp.path(), &manifest, &[])?;
        // 5007 appears in both env and source; should dedupe.
        assert_eq!(ports, vec![3000, 5007]);
        Ok(())
    }

    #[test]
    fn manifest_without_port_detection_just_uses_default() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let raw = r#"
name = "minimal"
display_name = "Minimal"
image = "alpine"
detect = ["minimal.toml"]
default_port = 7777
"#;
        let manifest: LangManifest = toml::from_str(raw)?;
        let ports = detect(tmp.path(), &manifest, &[])?;
        assert_eq!(ports, vec![7777]);
        Ok(())
    }
}
