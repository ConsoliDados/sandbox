//! Source code scanning for port literals using regex patterns from the
//! language manifest's `port_detection.patterns` array.
//!
//! Each pattern should have a single capture group around the port number.
//! Matches deeper in the project (any file extension) are accepted — we
//! don't try to be language-aware here because the *patterns themselves*
//! are language-aware (they live in the manifest).

use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;
use walkdir::WalkDir;

use crate::{Error, Result};

/// Walk `project_root` and collect every `u16` captured by any of `patterns`.
/// Skips common large directories (`node_modules`, `target`, `.git`, etc.)
/// and binary files. Order of the returned vec is the encounter order —
/// caller dedupes.
pub(super) fn scan_sources(project_root: &Path, patterns: &[String]) -> Result<Vec<u16>> {
    let compiled = compile_patterns(patterns)?;
    if compiled.is_empty() {
        return Ok(Vec::new());
    }
    let mut found = Vec::new();
    for entry in WalkDir::new(project_root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_ignored_dir(e.file_name().to_string_lossy().as_ref()))
    {
        let entry = entry.map_err(|e| Error::Io {
            path: project_root.to_path_buf(),
            source: e.into(),
        })?;
        if !entry.file_type().is_file() {
            continue;
        }
        if file_is_too_big(entry.path()) {
            continue;
        }
        let bytes = match std::fs::read(entry.path()) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => continue,
        };
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue;
        };
        for re in &compiled {
            for cap in re.captures_iter(text) {
                if let Some(m) = cap.get(1)
                    && let Ok(port) = m.as_str().parse::<u16>()
                {
                    found.push(port);
                }
            }
        }
    }
    Ok(found)
}

fn compile_patterns(patterns: &[String]) -> Result<Vec<Regex>> {
    let mut out = Vec::with_capacity(patterns.len());
    for p in patterns {
        let re = Regex::new(p).map_err(|e| Error::InvalidRegex {
            pattern: p.clone(),
            reason: e.to_string(),
        })?;
        out.push(re);
    }
    Ok(out)
}

/// Cap individual file size at ~1 MiB. The patterns we run target source
/// code; minified bundles and lockfiles aren't useful and would dominate
/// scan time on large repos.
fn file_is_too_big(path: &Path) -> bool {
    static LIMIT: OnceLock<u64> = OnceLock::new();
    let limit = *LIMIT.get_or_init(|| 1 << 20);
    std::fs::metadata(path)
        .map(|m| m.len() > limit)
        .unwrap_or(false)
}

fn is_ignored_dir(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | "node_modules"
            | "target"
            | ".pnpm-store"
            | ".yarn"
            | "dist"
            | "build"
            | ".venv"
            | "__pycache__"
            | "vendor"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    fn ps(slice: &[&str]) -> Vec<String> {
        slice.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn captures_node_app_listen() -> TestResult {
        let tmp = tempfile::tempdir()?;
        std::fs::write(
            tmp.path().join("server.js"),
            b"const port = 3000; app.listen(3000, () => {})\n",
        )?;
        let ports = scan_sources(tmp.path(), &ps(&[r"app\.listen\((\d+)"]))?;
        assert_eq!(ports, vec![3000]);
        Ok(())
    }

    #[test]
    fn captures_multiple_matches_across_files() -> TestResult {
        let tmp = tempfile::tempdir()?;
        std::fs::write(tmp.path().join("a.js"), b"server.listen(3000)\n")?;
        std::fs::create_dir_all(tmp.path().join("api"))?;
        std::fs::write(tmp.path().join("api/b.js"), b"app.listen(5007)\n")?;
        let mut ports = scan_sources(
            tmp.path(),
            &ps(&[r"server\.listen\((\d+)", r"app\.listen\((\d+)"]),
        )?;
        ports.sort();
        assert_eq!(ports, vec![3000, 5007]);
        Ok(())
    }

    #[test]
    fn ignores_node_modules() -> TestResult {
        let tmp = tempfile::tempdir()?;
        std::fs::create_dir_all(tmp.path().join("node_modules"))?;
        std::fs::write(
            tmp.path().join("node_modules/lib.js"),
            b"app.listen(9999)\n",
        )?;
        let ports = scan_sources(tmp.path(), &ps(&[r"app\.listen\((\d+)"]))?;
        assert!(ports.is_empty());
        Ok(())
    }

    #[test]
    fn rejects_invalid_pattern_with_specific_error() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let result = scan_sources(tmp.path(), &ps(&["[unclosed"]));
        assert!(matches!(result, Err(Error::InvalidRegex { .. })));
        Ok(())
    }

    #[test]
    fn skips_binary_files() -> TestResult {
        let tmp = tempfile::tempdir()?;
        std::fs::write(tmp.path().join("blob.bin"), [0xff, 0xfe, 0xfd])?;
        let ports = scan_sources(tmp.path(), &ps(&[r"\b(\d+)\b"]))?;
        assert!(ports.is_empty());
        Ok(())
    }
}
