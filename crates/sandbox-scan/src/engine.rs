//! Orchestrator for the scan pipeline: cache → YARA → heuristics → compose
//! → suppressions.
//!
//! Caches *pre*-suppression findings so adjusting the user's ignore file
//! takes effect without re-running expensive motors. Suppression matching
//! is keyed by `(rule_id, project_hash)` per OQ-007; callers pass the short
//! project hash because the CLI already knows it.

use std::path::{Path, PathBuf};

use crate::findings::Findings;
use crate::{
    Result, cache, compose, heuristics, project_hash, suppress::IgnoreList, yara::YaraEngine,
};

/// Optional knobs for `scan`. Defaults: cache enabled, no suppression file,
/// no project hash (so suppressions don't fire even if a file is provided).
#[derive(Debug, Clone, Default)]
pub struct ScanOpts {
    pub no_cache: bool,
    pub cache_dir: Option<PathBuf>,
    pub ignore_file: Option<PathBuf>,
    /// Short project hash (the one shown in `sandbox ps`). Required for
    /// suppression matching; without it, ignore entries can't be keyed.
    pub project_hash: Option<String>,
}

/// Result of a scan plus the inputs that produced it. `from_cache=true`
/// means the motors didn't actually run this invocation.
#[derive(Debug, Clone)]
pub struct ScanReport {
    pub content_hash: String,
    pub findings: Findings,
    pub from_cache: bool,
}

pub fn scan(project_root: &Path, opts: &ScanOpts) -> Result<ScanReport> {
    let files = project_hash::list_files(project_root)?;
    let content_hash = project_hash::hash_files(project_root, &files)?;

    // Cache lookup is best-effort and pre-suppression — see module doc.
    if !opts.no_cache
        && let Some(cache_dir) = opts.cache_dir.as_deref()
        && let Some(mut cached) = cache::lookup(cache_dir, &content_hash)
    {
        apply_suppressions(&mut cached, opts)?;
        return Ok(ScanReport {
            content_hash,
            findings: cached,
            from_cache: true,
        });
    }

    let mut findings = Findings::new();

    let yara_engine = YaraEngine::builtin()?;
    let yara_findings = yara_engine.scan_files(project_root, &files)?;
    findings.extend(yara_findings.items);

    let heuristic_findings = heuristics::scan_files(project_root, &files)?;
    findings.extend(heuristic_findings.items);

    let compose_findings = compose::scan(project_root)?;
    findings.extend(compose_findings.items);

    findings.sort_canonical();

    // Persist the pre-suppression view so the cache stays useful across
    // changes to the user's ignore file.
    if let Some(cache_dir) = opts.cache_dir.as_deref() {
        cache::store(cache_dir, &content_hash, &findings)?;
    }

    apply_suppressions(&mut findings, opts)?;

    Ok(ScanReport {
        content_hash,
        findings,
        from_cache: false,
    })
}

fn apply_suppressions(findings: &mut Findings, opts: &ScanOpts) -> Result<()> {
    let Some(ignore_path) = opts.ignore_file.as_deref() else {
        return Ok(());
    };
    let Some(hash) = opts.project_hash.as_deref() else {
        return Ok(());
    };
    let list = IgnoreList::load(ignore_path)?;
    list.apply(findings, hash);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::findings::Severity;

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    fn write_evil_project(root: &Path) -> std::io::Result<()> {
        std::fs::write(
            root.join("server.js"),
            "const _ = new (Function.constructor)('require','m','...');\n\
             const c2 = 'Y2hhaW5saW5rLWFwaS12My5saXY=';\n\
             const endpoint = '/api/service/token/abc';\n",
        )
    }

    #[test]
    fn end_to_end_flags_yara_critical_on_known_pattern() -> TestResult {
        let tmp = tempfile::tempdir()?;
        write_evil_project(tmp.path())?;
        let report = scan(tmp.path(), &ScanOpts::default())?;
        assert!(!report.from_cache);
        assert_eq!(report.findings.worst_severity(), Some(Severity::Critical));
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "yara/contagious_interview_profile_js")
        );
        Ok(())
    }

    #[test]
    fn second_run_hits_cache() -> TestResult {
        // Cache dir must live OUTSIDE the project root — otherwise the
        // listing walkdir picks it up and changes the content hash on the
        // second pass, defeating the cache.
        let project = tempfile::tempdir()?;
        let cache_home = tempfile::tempdir()?;
        write_evil_project(project.path())?;

        let opts = ScanOpts {
            no_cache: false,
            cache_dir: Some(cache_home.path().to_path_buf()),
            ..ScanOpts::default()
        };
        let first = scan(project.path(), &opts)?;
        assert!(!first.from_cache);

        let second = scan(project.path(), &opts)?;
        assert!(second.from_cache);
        assert_eq!(first.findings, second.findings);
        Ok(())
    }

    #[test]
    fn no_cache_skips_lookup() -> TestResult {
        let project = tempfile::tempdir()?;
        let cache_home = tempfile::tempdir()?;
        write_evil_project(project.path())?;
        let opts = ScanOpts {
            no_cache: false,
            cache_dir: Some(cache_home.path().to_path_buf()),
            ..ScanOpts::default()
        };
        scan(project.path(), &opts)?; // populate

        let opts_no_cache = ScanOpts {
            no_cache: true,
            ..opts.clone()
        };
        let report = scan(project.path(), &opts_no_cache)?;
        assert!(!report.from_cache);
        Ok(())
    }

    #[test]
    fn suppression_drops_only_matching_pair() -> TestResult {
        let tmp = tempfile::tempdir()?;
        write_evil_project(tmp.path())?;
        let ignore = tmp.path().join("ignore.toml");
        std::fs::write(
            &ignore,
            "[[ignore]]\n\
             rule_id = \"yara/contagious_interview_c2_domain\"\n\
             project_hash = \"deadbeef\"\n",
        )?;
        let opts = ScanOpts {
            ignore_file: Some(ignore),
            project_hash: Some("deadbeef".into()),
            ..ScanOpts::default()
        };
        let report = scan(tmp.path(), &opts)?;
        // The C2-domain rule is suppressed; the strict profile_js rule is not.
        assert!(
            report
                .findings
                .iter()
                .all(|f| f.rule_id != "yara/contagious_interview_c2_domain")
        );
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "yara/contagious_interview_profile_js")
        );
        Ok(())
    }

    #[test]
    fn clean_project_yields_empty_findings() -> TestResult {
        let tmp = tempfile::tempdir()?;
        std::fs::write(tmp.path().join("index.js"), b"console.log('hi');\n")?;
        let report = scan(tmp.path(), &ScanOpts::default())?;
        assert!(report.findings.is_empty(), "got {:?}", report.findings);
        Ok(())
    }
}
