//! ClamAV motor: parses `clamscan --no-summary --infected` output into
//! `Findings`.
//!
//! The actual scan happens in an ephemeral docker container driven by
//! `sandbox-docker::scanner`; this module is pure so it stays testable and
//! `sandbox-scan` keeps its no-docker invariant.
//!
//! Output format we consume (one line per infection):
//!   `/scan/<relpath>: <signature> FOUND`
//!
//! `/scan` is the bind-mount destination inside the scanner container — we
//! strip it so findings render with project-relative paths matching the
//! YARA/heuristic motors.

use std::path::{Path, PathBuf};

use crate::findings::{Finding, Findings, Severity};
use crate::{Error, Result};

const RULE_PREFIX: &str = "clamav";
const SCAN_PREFIX: &str = "/scan/";

/// The scanner Dockerfile bundled at build time. The CLI materializes this
/// into a host directory so `docker build` can read it.
pub const SCANNER_DOCKERFILE: &str = include_str!("../../scanner-image/Dockerfile");

/// Write the bundled `Dockerfile` into `target_dir` (creating it if missing)
/// and return the directory path for `docker build`. Idempotent: if a
/// Dockerfile already exists with the same content, this is a no-op write.
pub fn materialize_scanner_dockerfile(target_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(target_dir).map_err(|source| Error::Io {
        path: target_dir.to_path_buf(),
        source,
    })?;
    let dockerfile = target_dir.join("Dockerfile");
    std::fs::write(&dockerfile, SCANNER_DOCKERFILE).map_err(|source| Error::Io {
        path: dockerfile,
        source,
    })?;
    Ok(target_dir.to_path_buf())
}

/// Parse a clamscan stdout buffer. Returns a `Findings` with one entry per
/// infected file. Lines that don't match the `<path>: <signature> FOUND`
/// shape are ignored silently — clamscan can emit progress noise even with
/// `--no-summary` (e.g. "LibClamAV Warning: ..."), and dropping them is
/// safer than over-flagging.
pub fn parse_output(stdout: &str) -> Findings {
    let mut out = Findings::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        let Some((left, signature)) = trimmed.rsplit_once(" FOUND") else {
            continue;
        };
        if !signature.is_empty() {
            // " FOUND" must be a suffix, not a substring — otherwise the
            // signature would have trailing text.
            continue;
        }
        let Some((raw_path, sig)) = left.rsplit_once(": ") else {
            continue;
        };
        let sig = sig.trim();
        if sig.is_empty() {
            continue;
        }
        let path = raw_path
            .strip_prefix(SCAN_PREFIX)
            .unwrap_or(raw_path)
            .to_string();
        out.push(Finding {
            rule_id: format!("{RULE_PREFIX}/{sig}"),
            severity: Severity::Critical,
            message: format!("ClamAV signature `{sig}` matched"),
            path: PathBuf::from(path),
            line: None,
            remediation: Some(
                "Verify against current AV reports. If it's a known false positive, \
                 suppress via ~/.config/sandbox/scan-ignore.toml; otherwise \
                 discard the project."
                    .into(),
            ),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_output_yields_no_findings() {
        assert!(parse_output("").is_empty());
        assert!(parse_output("   \n  \n").is_empty());
    }

    #[test]
    fn single_infection_parses() {
        let raw = "/scan/server.js: Js.Trojan.BeaverTail-1 FOUND\n";
        let f = parse_output(raw);
        let summary: Vec<_> = f
            .iter()
            .map(|x| (x.rule_id.clone(), x.severity, x.path.clone()))
            .collect();
        assert_eq!(
            summary,
            vec![(
                "clamav/Js.Trojan.BeaverTail-1".into(),
                Severity::Critical,
                PathBuf::from("server.js"),
            )]
        );
    }

    #[test]
    fn multiple_infections_parse_in_order() {
        let raw = "\
/scan/a.js: Win.Eicar-Test FOUND
/scan/sub/b.js: Js.Webshell.X FOUND
";
        let paths: Vec<_> = parse_output(raw)
            .iter()
            .map(|f| f.path.to_string_lossy().into_owned())
            .collect();
        assert_eq!(paths, vec!["a.js", "sub/b.js"]);
    }

    #[test]
    fn libclamav_warning_lines_are_ignored() {
        let raw = "\
LibClamAV Warning: cli_pdf: cannot extract embedded file
/scan/payload.bin: Win.Trojan.Generic FOUND
";
        let ids: Vec<_> = parse_output(raw)
            .iter()
            .map(|f| f.rule_id.clone())
            .collect();
        assert_eq!(ids, vec!["clamav/Win.Trojan.Generic"]);
    }

    #[test]
    fn paths_outside_scan_prefix_are_preserved_verbatim() {
        // Defensive: if the bind path changes, we still report something.
        let raw = "/elsewhere/x.js: Sig FOUND\n";
        let paths: Vec<_> = parse_output(raw)
            .iter()
            .map(|f| f.path.to_string_lossy().into_owned())
            .collect();
        assert_eq!(paths, vec!["/elsewhere/x.js"]);
    }

    #[test]
    fn lines_without_found_suffix_are_ignored() {
        let raw = "/scan/x.js: Not a real entry\n";
        assert!(parse_output(raw).is_empty());
    }
}
