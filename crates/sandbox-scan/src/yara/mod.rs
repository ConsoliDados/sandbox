//! YARA motor for `sandbox-scan`.
//!
//! Wraps `yara-x` (pure Rust). Rules ship bundled via `include_str!` and
//! compile at engine construction time. Each matching rule becomes a
//! `Finding` with severity / message / remediation read from the rule's
//! `meta:` block.

use std::path::{Path, PathBuf};

use yara_x::MetaValue;

use crate::findings::{Finding, Findings, Severity};
use crate::{Error, Result};

const RULES_CONTAGIOUS_INTERVIEW: &str = include_str!("rules/contagious_interview.yar");

/// Compiled, ready-to-scan rule set.
pub struct YaraEngine {
    rules: yara_x::Rules,
}

impl YaraEngine {
    /// Compile every bundled rule file. Call once at engine startup —
    /// compilation is comparatively expensive (~ms per rule), scanning is
    /// cheap.
    pub fn builtin() -> Result<Self> {
        let mut compiler = yara_x::Compiler::new();
        compiler
            .add_source(RULES_CONTAGIOUS_INTERVIEW)
            .map_err(|e| Error::YaraCompile(e.to_string()))?;
        let rules = compiler.build();
        Ok(Self { rules })
    }

    /// Scan a flat list of project-relative file paths and return the
    /// collected findings. The same project_root is joined with each rel
    /// path. Files that are missing on disk are skipped silently — the
    /// caller should have produced the list from the project hash, which is
    /// already a snapshot of what exists.
    pub fn scan_files(&self, project_root: &Path, files: &[String]) -> Result<Findings> {
        let mut scanner = yara_x::Scanner::new(&self.rules);
        let mut findings = Findings::new();

        for rel in files {
            let abs = project_root.join(rel);
            let bytes = match std::fs::read(&abs) {
                Ok(b) => b,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(source) => return Err(Error::Io { path: abs, source }),
            };
            let results = scanner
                .scan(&bytes)
                .map_err(|e| Error::YaraScan(e.to_string()))?;
            for matching in results.matching_rules() {
                let (severity, description, remediation) = read_meta(&matching);
                let id = matching.identifier();
                findings.push(Finding {
                    rule_id: format!("yara/{id}"),
                    severity,
                    message: description.unwrap_or_else(|| format!("YARA rule `{id}` matched")),
                    path: PathBuf::from(rel),
                    line: None,
                    remediation,
                });
            }
        }

        Ok(findings)
    }
}

fn read_meta(rule: &yara_x::Rule<'_, '_>) -> (Severity, Option<String>, Option<String>) {
    let mut severity = Severity::High;
    let mut description = None;
    let mut remediation = None;
    for (key, value) in rule.metadata() {
        match (key, value) {
            ("severity", MetaValue::String(s)) => {
                severity = parse_severity(s).unwrap_or(severity);
            }
            ("description", MetaValue::String(s)) => description = Some(s.to_string()),
            ("remediation", MetaValue::String(s)) => remediation = Some(s.to_string()),
            _ => {}
        }
    }
    (severity, description, remediation)
}

fn parse_severity(s: &str) -> Option<Severity> {
    match s.to_ascii_lowercase().as_str() {
        "info" => Some(Severity::Info),
        "warn" | "warning" => Some(Severity::Warn),
        "high" => Some(Severity::High),
        "critical" => Some(Severity::Critical),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn builtin_rules_compile() -> TestResult {
        let _ = YaraEngine::builtin()?;
        Ok(())
    }

    #[test]
    fn clean_project_has_no_findings() -> TestResult {
        let tmp = tempfile::tempdir()?;
        std::fs::write(
            tmp.path().join("index.js"),
            b"console.log('hello world');\n",
        )?;
        std::fs::write(tmp.path().join("README.md"), b"# clean repo\n")?;
        let engine = YaraEngine::builtin()?;
        let findings = engine.scan_files(tmp.path(), &["index.js".into(), "README.md".into()])?;
        assert!(
            findings.is_empty(),
            "expected no findings, got {findings:?}"
        );
        Ok(())
    }

    #[test]
    fn profile_js_backdoor_pattern_fires_critical() -> TestResult {
        let tmp = tempfile::tempdir()?;
        // Synthetic file with all three needles required by the rule.
        let body = r#"
const _ = (() => {
    return new (Function.constructor)('require','m','...');
})();
const c2 = 'Y2hhaW5saW5rLWFwaS12My5saXY=';
const endpoint = '/api/service/token/abc';
"#;
        std::fs::write(tmp.path().join("server.js"), body)?;
        let engine = YaraEngine::builtin()?;
        let findings = engine.scan_files(tmp.path(), &["server.js".into()])?;
        assert!(findings.iter().any(|f| {
            f.rule_id == "yara/contagious_interview_profile_js" && f.severity == Severity::Critical
        }));
        Ok(())
    }

    #[test]
    fn vscode_autorun_pattern_fires_critical() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let body = r#"{
  "tasks": [{
    "label": "post",
    "type": "shell",
    "command": "node .vscode/cancel",
    "runOn": "folderOpen",
    "presentation": { "hide": true, "reveal": "never" }
  }]
}"#;
        std::fs::create_dir_all(tmp.path().join(".vscode"))?;
        std::fs::write(tmp.path().join(".vscode/tasks.json"), body)?;
        let engine = YaraEngine::builtin()?;
        let findings = engine.scan_files(tmp.path(), &[".vscode/tasks.json".to_string()])?;
        assert!(findings.iter().any(|f| {
            f.rule_id == "yara/contagious_interview_vscode_autorun"
                && f.severity == Severity::Critical
        }));
        Ok(())
    }

    #[test]
    fn c2_domain_alone_fires_high() -> TestResult {
        let tmp = tempfile::tempdir()?;
        std::fs::write(
            tmp.path().join("notes.md"),
            b"see: https://chainlink-api-v3.live/foo",
        )?;
        let engine = YaraEngine::builtin()?;
        let findings = engine.scan_files(tmp.path(), &["notes.md".into()])?;
        let severities: Vec<_> = findings
            .iter()
            .filter(|f| f.rule_id == "yara/contagious_interview_c2_domain")
            .map(|f| f.severity)
            .collect();
        assert_eq!(severities, vec![Severity::High]);
        Ok(())
    }
}
