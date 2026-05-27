//! Scan result cache, keyed by `(content_hash, ruleset_version)`.
//!
//! Each entry lives at `<scan_cache_dir>/<content_hash>.toml`. A lookup hit
//! only matters if the stored `ruleset_version` equals the current one —
//! bumping rules invalidates every entry without us having to walk the dir.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::findings::Findings;
use crate::{Error, Result};

/// Current ruleset version. Bump whenever the YARA rules, heuristic regexes,
/// or compose checks change in a way that could alter findings. Old cache
/// entries with a lower version are ignored on lookup.
///
/// History:
/// - v1: initial ruleset (Contagious Interview YARA + heuristics + compose).
/// - v2: `heuristics/eval_function_constructor` widened to match
///   `new (Function.constructor)(…)` parenthesized form.
/// - v3: `compose/registry_not_allowed` added (Phase 6, ADR-0010 §
///   registry allowlist). Default-allows `docker.io/library/*` and
///   `ghcr.io/*` only.
pub const RULESET_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    ruleset_version: u32,
    content_hash: String,
    findings: Findings,
}

/// Read the cache entry for `content_hash`. Returns `None` when the file is
/// missing, unreadable, malformed, or stamped with an older ruleset version.
/// Errors are intentionally not propagated — a corrupt cache entry should
/// fall through to a real scan, not abort the run.
pub fn lookup(scan_dir: &Path, content_hash: &str) -> Option<Findings> {
    let path = entry_path(scan_dir, content_hash);
    let raw = std::fs::read_to_string(&path).ok()?;
    let entry: Entry = toml::from_str(&raw).ok()?;
    if entry.ruleset_version != RULESET_VERSION {
        return None;
    }
    if entry.content_hash != content_hash {
        return None;
    }
    Some(entry.findings)
}

/// Persist `findings` under `content_hash`. Creates `scan_dir` if missing.
pub fn store(scan_dir: &Path, content_hash: &str, findings: &Findings) -> Result<()> {
    std::fs::create_dir_all(scan_dir).map_err(|source| Error::Io {
        path: scan_dir.to_path_buf(),
        source,
    })?;
    let entry = Entry {
        ruleset_version: RULESET_VERSION,
        content_hash: content_hash.to_string(),
        findings: findings.clone(),
    };
    let path = entry_path(scan_dir, content_hash);
    let body = toml::to_string_pretty(&entry).map_err(|e| Error::InvalidToml {
        path: path.clone(),
        reason: e.to_string(),
    })?;
    std::fs::write(&path, body).map_err(|source| Error::Io { path, source })
}

fn entry_path(scan_dir: &Path, content_hash: &str) -> PathBuf {
    scan_dir.join(format!("{content_hash}.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::findings::{Finding, Severity};
    use std::path::PathBuf;

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    fn sample_findings() -> Findings {
        let mut f = Findings::new();
        f.push(Finding {
            rule_id: "yara/some_rule".into(),
            severity: Severity::High,
            message: "boom".into(),
            path: PathBuf::from("src/index.js"),
            line: Some(42),
            remediation: Some("delete it".into()),
        });
        f
    }

    #[test]
    fn store_then_lookup_roundtrips() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let hash = "deadbeef";
        store(tmp.path(), hash, &sample_findings())?;
        assert_eq!(lookup(tmp.path(), hash), Some(sample_findings()));
        Ok(())
    }

    #[test]
    fn lookup_misses_when_hash_unknown() -> TestResult {
        let tmp = tempfile::tempdir()?;
        assert!(lookup(tmp.path(), "no-such-hash").is_none());
        Ok(())
    }

    #[test]
    fn lookup_misses_when_ruleset_version_differs() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let path = entry_path(tmp.path(), "abc");
        let stale = Entry {
            ruleset_version: RULESET_VERSION + 1, // pretend we're behind
            content_hash: "abc".into(),
            findings: sample_findings(),
        };
        std::fs::write(&path, toml::to_string_pretty(&stale)?)?;
        // Older binary on a newer cache: we don't trust the entry.
        assert!(lookup(tmp.path(), "abc").is_none());
        Ok(())
    }

    #[test]
    fn lookup_misses_on_corrupt_toml() -> TestResult {
        let tmp = tempfile::tempdir()?;
        std::fs::write(entry_path(tmp.path(), "abc"), b"[[[ not toml")?;
        assert!(lookup(tmp.path(), "abc").is_none());
        Ok(())
    }
}
