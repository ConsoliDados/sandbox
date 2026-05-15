//! User-global scan suppression (`~/.config/sandbox/scan-ignore.toml`).
//!
//! Entries are keyed by `(rule_id, project_hash)`. The hash is the short
//! one shown in `sandbox ps` — the engine receives it from the caller (CLI
//! knows the project hash before invoking the scan).
//!
//! Project-local ignore files are intentionally not supported. See ADR-0008
//! and OQ-007 for the rationale (malware planting a suppression entry to
//! silence its own detection).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::findings::Findings;
use crate::{Error, Result};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct IgnoreFile {
    #[serde(default, rename = "ignore")]
    entries: Vec<Entry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    rule_id: String,
    project_hash: String,
    #[serde(default)]
    note: Option<String>,
}

/// Compiled list of `(rule_id, project_hash)` pairs to drop from findings.
/// Constructed once; cheap to apply per-scan.
#[derive(Debug, Clone, Default)]
pub struct IgnoreList {
    entries: Vec<(String, String)>,
}

impl IgnoreList {
    /// Load from the user's scan-ignore.toml. Missing file = empty list
    /// (the common case for a first run on a new machine).
    pub fn load(path: &Path) -> Result<Self> {
        let raw = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(Error::Io {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };
        let file: IgnoreFile = toml::from_str(&raw).map_err(|e| Error::InvalidToml {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })?;
        Ok(Self {
            entries: file
                .entries
                .into_iter()
                .map(|e| (e.rule_id, e.project_hash))
                .collect(),
        })
    }

    /// Drop every finding whose `(rule_id, project_hash)` matches an entry.
    /// `project_hash` is passed by the caller — the engine knows it; we
    /// don't recompute here.
    pub fn apply(&self, findings: &mut Findings, project_hash: &str) {
        if self.entries.is_empty() {
            return;
        }
        findings.items.retain(|f| {
            !self
                .entries
                .iter()
                .any(|(rid, ph)| f.rule_id == *rid && ph == project_hash)
        });
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
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
            rule_id: "yara/contagious_interview_c2_domain".into(),
            severity: Severity::High,
            message: "x".into(),
            path: PathBuf::from("a.js"),
            line: None,
            remediation: None,
        });
        f.push(Finding {
            rule_id: "heuristics/eval_function_constructor".into(),
            severity: Severity::High,
            message: "x".into(),
            path: PathBuf::from("b.js"),
            line: None,
            remediation: None,
        });
        f
    }

    #[test]
    fn missing_ignore_file_yields_empty_list() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let path = tmp.path().join("does-not-exist.toml");
        let list = IgnoreList::load(&path)?;
        assert!(list.is_empty());
        Ok(())
    }

    #[test]
    fn applying_empty_list_is_noop() -> TestResult {
        let mut f = sample_findings();
        IgnoreList::default().apply(&mut f, "deadbeef");
        assert_eq!(f.len(), 2);
        Ok(())
    }

    #[test]
    fn matches_drop_only_specified_pair() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let path = tmp.path().join("scan-ignore.toml");
        std::fs::write(
            &path,
            r#"
[[ignore]]
rule_id = "yara/contagious_interview_c2_domain"
project_hash = "abcdef123456"
note = "audited, false positive on docs page"
"#,
        )?;
        let list = IgnoreList::load(&path)?;

        // Wrong hash → no drop.
        let mut f1 = sample_findings();
        list.apply(&mut f1, "ffffffffffff");
        assert_eq!(f1.len(), 2);

        // Matching hash → drop only the suppressed rule.
        let mut f2 = sample_findings();
        list.apply(&mut f2, "abcdef123456");
        let remaining: Vec<_> = f2.iter().map(|f| f.rule_id.as_str()).collect();
        assert_eq!(remaining, vec!["heuristics/eval_function_constructor"]);
        Ok(())
    }

    #[test]
    fn malformed_toml_errors() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let path = tmp.path().join("scan-ignore.toml");
        std::fs::write(&path, "[[[ not toml")?;
        let result = IgnoreList::load(&path);
        assert!(matches!(result, Err(Error::InvalidToml { .. })));
        Ok(())
    }
}
