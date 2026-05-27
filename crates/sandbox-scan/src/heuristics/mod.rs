//! Heuristic regex/shape checks complementing the signature-based YARA motor.
//!
//! Each submodule owns a domain (VSCode autorun, npm pre/post hooks, eval
//! shapes, base64+network). The orchestrator dispatches by file path so we
//! only run JS checks on JS files, JSON checks on JSON files, etc. — keeps
//! the runtime predictable on large repos.

mod eval_patterns;
mod network;
mod package_json;
mod regex_util;
mod vscode;

use std::path::Path;

use crate::Result;
use crate::findings::Findings;

/// Run every applicable heuristic over each file. Reads file bytes once and
/// reuses the buffer across checks.
pub fn scan_files(project_root: &Path, files: &[String]) -> Result<Findings> {
    let mut findings = Findings::new();
    for rel in files {
        let path = std::path::PathBuf::from(rel);
        let abs = project_root.join(&path);
        let bytes = match std::fs::read(&abs) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => return Err(crate::Error::Io { path: abs, source }),
        };
        let body = match std::str::from_utf8(&bytes) {
            Ok(s) => s,
            // Binary or non-UTF8 file — heuristics are text-shape checks
            // so we skip cleanly. YARA already saw the bytes upstream.
            Err(_) => continue,
        };
        findings.extend(checks_for(&path, body));
    }
    Ok(findings)
}

fn checks_for(path: &Path, body: &str) -> Vec<crate::findings::Finding> {
    let mut out = Vec::new();
    let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");

    if file_name == "tasks.json" && contains_segment(path, ".vscode") {
        out.extend(vscode::check_tasks_json(path, body));
    }
    if file_name == "package.json" {
        out.extend(package_json::check(path, body));
    }
    if is_jsish(file_name) {
        out.extend(eval_patterns::check(path, body));
        out.extend(network::check(path, body));
    }
    out
}

fn is_jsish(file_name: &str) -> bool {
    matches!(
        std::path::Path::new(file_name)
            .extension()
            .and_then(|e| e.to_str()),
        Some("js") | Some("mjs") | Some("cjs") | Some("ts") | Some("tsx") | Some("jsx")
    )
}

fn contains_segment(path: &Path, needle: &str) -> bool {
    path.components()
        .any(|c| c.as_os_str().to_string_lossy() == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn checks_for_dispatches_only_to_applicable_modules() {
        let body = "console.log(1)";
        // .ts file — js heuristics only, no package_json / vscode dispatch.
        let out = checks_for(Path::new("src/index.ts"), body);
        // Empty is fine; the assertion is that we don't panic and we don't
        // try to parse this as package.json.
        assert!(
            out.iter()
                .all(|f| !f.rule_id.starts_with("heuristics/package_json"))
        );
    }

    #[test]
    fn scan_files_skips_binary() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let path = tmp.path().join("blob.js");
        std::fs::write(&path, [0xff, 0xfe, 0xfd, 0xfc])?;
        let f = scan_files(tmp.path(), &["blob.js".into()])?;
        assert!(f.is_empty());
        Ok(())
    }

    #[test]
    fn jsish_extensions_recognized() {
        assert!(is_jsish("a.js"));
        assert!(is_jsish("a.mjs"));
        assert!(is_jsish("a.cjs"));
        assert!(is_jsish("a.ts"));
        assert!(is_jsish("a.tsx"));
        assert!(is_jsish("a.jsx"));
        assert!(!is_jsish("a.json"));
        assert!(!is_jsish("a.txt"));
    }
}
