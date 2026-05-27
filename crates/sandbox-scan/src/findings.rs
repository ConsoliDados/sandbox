//! Output of a scan: a sequence of `Finding`s plus severity helpers.
//!
//! Findings are deterministic given `(content_hash, ruleset_version)` — the
//! same project sources and the same rules produce the same vec, ordered by
//! `(severity desc, path, line)`. Determinism matters for the cache.

use std::cmp::Reverse;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warn,
    High,
    Critical,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warn => "warn",
            Severity::High => "high",
            Severity::Critical => "critical",
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A single security finding produced by a motor (YARA, heuristic, compose).
///
/// `rule_id` is the user-facing stable identifier — namespaced as
/// `<motor>/<rule_name>` (e.g. `contagious_interview/function_constructor_eval`).
/// Suppression entries match by this id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub rule_id: String,
    pub severity: Severity,
    pub message: String,
    /// Path relative to the project root.
    pub path: PathBuf,
    pub line: Option<u32>,
    pub remediation: Option<String>,
}

/// Collected findings from one scan. Empty `Findings` == clean.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Findings {
    #[serde(default)]
    pub items: Vec<Finding>,
}

impl Findings {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, f: Finding) {
        self.items.push(f);
    }

    pub fn extend<I: IntoIterator<Item = Finding>>(&mut self, iter: I) {
        self.items.extend(iter);
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Finding> {
        self.items.iter()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Highest severity across all findings, or `None` if empty.
    pub fn worst_severity(&self) -> Option<Severity> {
        self.items.iter().map(|f| f.severity).max()
    }

    /// `true` when at least one finding meets or exceeds `threshold`.
    pub fn blocks_at(&self, threshold: Severity) -> bool {
        self.items.iter().any(|f| f.severity >= threshold)
    }

    /// Sort in-place by `(severity desc, path, line)`. Idempotent.
    pub fn sort_canonical(&mut self) {
        self.items.sort_by(|a, b| {
            Reverse(a.severity)
                .cmp(&Reverse(b.severity))
                .then_with(|| a.path.cmp(&b.path))
                .then_with(|| a.line.cmp(&b.line))
                .then_with(|| a.rule_id.cmp(&b.rule_id))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(id: &str, sev: Severity, path: &str, line: Option<u32>) -> Finding {
        Finding {
            rule_id: id.into(),
            severity: sev,
            message: "x".into(),
            path: PathBuf::from(path),
            line,
            remediation: None,
        }
    }

    #[test]
    fn severity_orders_least_to_greatest() {
        assert!(Severity::Info < Severity::Warn);
        assert!(Severity::Warn < Severity::High);
        assert!(Severity::High < Severity::Critical);
    }

    #[test]
    fn worst_severity_returns_max() {
        let mut f = Findings::new();
        f.push(finding("a", Severity::Info, "a.js", None));
        f.push(finding("b", Severity::High, "b.js", None));
        f.push(finding("c", Severity::Warn, "c.js", None));
        assert_eq!(f.worst_severity(), Some(Severity::High));
    }

    #[test]
    fn worst_severity_on_empty_is_none() {
        assert_eq!(Findings::new().worst_severity(), None);
    }

    #[test]
    fn blocks_at_returns_true_for_equal_severity() {
        let mut f = Findings::new();
        f.push(finding("a", Severity::High, "x", None));
        assert!(f.blocks_at(Severity::High));
        assert!(!f.blocks_at(Severity::Critical));
    }

    #[test]
    fn sort_canonical_orders_by_severity_then_path_then_line() {
        let mut f = Findings::new();
        f.push(finding("z", Severity::Warn, "b.js", Some(1)));
        f.push(finding("a", Severity::Critical, "x.js", Some(10)));
        f.push(finding("m", Severity::Critical, "a.js", Some(5)));
        f.sort_canonical();
        let ordered: Vec<_> = f
            .iter()
            .map(|i| (i.severity, i.path.to_string_lossy().into_owned()))
            .collect();
        assert_eq!(
            ordered,
            vec![
                (Severity::Critical, "a.js".into()),
                (Severity::Critical, "x.js".into()),
                (Severity::Warn, "b.js".into()),
            ]
        );
    }
}
