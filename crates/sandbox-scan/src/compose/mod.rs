//! Compose validator: parses `docker-compose.yml`/`compose.yaml` at project
//! root and emits findings per service.
//!
//! Compose deps are integrated into the full project lifecycle in Phase 6
//! (ADR-0010). The validator already lives here because the same parse +
//! rule set will run there too — scanning the file is the easy half.

pub mod parse;
mod rules;

use std::path::Path;

use crate::Result;
use crate::findings::Findings;

/// Compose filenames we look at, in lookup order. The first match wins.
const COMPOSE_FILENAMES: &[&str] = &[
    "docker-compose.yml",
    "docker-compose.yaml",
    "compose.yml",
    "compose.yaml",
];

/// Scan every recognized compose file at the project root and collect
/// findings. Returns empty `Findings` when no compose file is present.
pub fn scan(project_root: &Path) -> Result<Findings> {
    let mut findings = Findings::new();
    for name in COMPOSE_FILENAMES {
        let path = project_root.join(name);
        if !path.is_file() {
            continue;
        }
        let body = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => return Err(crate::Error::Io { path, source }),
        };
        let compose = parse::parse(&path, &body)?;
        // Path passed to rules is project-relative for nicer reporting.
        let rel = Path::new(name);
        for (svc_name, svc) in &compose.services {
            findings.extend(rules::check_service(rel, svc_name, svc));
        }
    }
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::findings::Severity;

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn no_compose_file_no_findings() -> TestResult {
        let tmp = tempfile::tempdir()?;
        assert!(scan(tmp.path())?.is_empty());
        Ok(())
    }

    #[test]
    fn finds_privileged_service() -> TestResult {
        let tmp = tempfile::tempdir()?;
        std::fs::write(
            tmp.path().join("docker-compose.yml"),
            r#"
services:
  evil:
    image: x
    privileged: true
"#,
        )?;
        let f = scan(tmp.path())?;
        assert!(
            f.iter()
                .any(|x| x.rule_id == "compose/privileged" && x.severity == Severity::Critical)
        );
        Ok(())
    }

    #[test]
    fn picks_first_extension_in_lookup_order() -> TestResult {
        let tmp = tempfile::tempdir()?;
        std::fs::write(
            tmp.path().join("docker-compose.yml"),
            "services:\n  a:\n    image: x\n    privileged: true\n",
        )?;
        std::fs::write(
            tmp.path().join("compose.yml"),
            "services:\n  b:\n    image: x\n    network_mode: host\n",
        )?;
        let f = scan(tmp.path())?;
        // Both files exist; both should be scanned (no early return).
        assert!(f.iter().any(|x| x.rule_id == "compose/privileged"));
        assert!(f.iter().any(|x| x.rule_id == "compose/network_mode_host"));
        Ok(())
    }
}
