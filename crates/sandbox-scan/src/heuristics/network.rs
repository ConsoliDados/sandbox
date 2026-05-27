//! Base64-decoded blob followed by a network call within a few lines.
//!
//! The Contagious Interview backdoor uses this shape:
//!   const c2 = Buffer.from('Y2hhaW5...', 'base64').toString() + '...';
//!   ...
//!   require('https').get(c2, ...);
//!
//! A `Buffer.from(..., 'base64')` decode by itself is benign (parsing PEM
//! certs, image bytes, etc.). The signal is the proximity to a network
//! call. We slide a window over lines: any `Buffer.from(*, 'base64')`
//! followed by `https.get/request`, `http.get/request`, `fetch(`,
//! `axios.*`, or `require('https')`/`require('http')` within
//! `PROXIMITY_LINES` becomes a finding.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

use super::regex_util::compile;
use crate::findings::{Finding, Severity};

const RULE_ID: &str = "heuristics/base64_then_network";

/// How many subsequent lines after a base64 decode we still treat as
/// "proximate" to a network call. Captures the typical 3–10 line gap seen
/// in real backdoors without firing on `Buffer.from('...', 'base64')` at
/// the top of a 500-line file that happens to use fetch much later.
const PROXIMITY_LINES: usize = 12;

fn base64_decode() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| compile(r#"Buffer\s*\.\s*from\s*\([^)]*,\s*['"`]base64['"`]"#))
}

fn network_call() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        compile(
            r#"(?x)
            (?:
                require\s*\(\s*['"`]https?['"`]\s*\)
              | https?\s*\.\s*(?:get|request)\s*\(
              | \bfetch\s*\(
              | \baxios\s*\.\s*(?:get|post|put|delete|request)\s*\(
            )
            "#,
        )
    })
}

pub(super) fn check(rel: &Path, body: &str) -> Vec<Finding> {
    let lines: Vec<&str> = body.lines().collect();
    let mut out = Vec::new();

    // Find all base64 lines first, then for each, peek ahead PROXIMITY_LINES.
    let decode_lines: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| base64_decode().is_match(l))
        .map(|(i, _)| i)
        .collect();

    let mut emitted = std::collections::HashSet::new();
    for &i in &decode_lines {
        let end = (i + 1 + PROXIMITY_LINES).min(lines.len());
        if let Some(j_rel) = lines
            .get(i + 1..end)
            .and_then(|window| window.iter().position(|l| network_call().is_match(l)))
        {
            let j = i + 1 + j_rel;
            // Dedup by decode line: one finding per base64 site even if
            // multiple network calls follow.
            if emitted.insert(i) {
                out.push(Finding {
                    rule_id: RULE_ID.into(),
                    severity: Severity::High,
                    message: format!(
                        "Base64 decode at line {} is followed by a network call at line {} (within {} lines)",
                        i + 1,
                        j + 1,
                        PROXIMITY_LINES
                    ),
                    path: PathBuf::from(rel),
                    line: Some((i + 1) as u32),
                    remediation: Some(
                        "Decode the base64 manually and inspect what URL or payload it produces. \
                         The combination is rare in non-malicious code."
                            .into(),
                    ),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_when_base64_then_fetch_close() {
        let body = r#"
const blob = Buffer.from('aGVsbG8=', 'base64').toString();
const url = blob + '/x';
fetch(url);
"#;
        let f = check(Path::new("a.js"), body);
        let summary: Vec<_> = f.iter().map(|x| (x.rule_id.as_str(), x.severity)).collect();
        assert_eq!(summary, vec![(RULE_ID, Severity::High)]);
    }

    #[test]
    fn fires_when_base64_then_https_require() {
        let body = r#"
const c2 = Buffer.from('Y2hhaW5saW5rLWFwaS12My5saXY=', 'base64').toString() + 'e';
const https = require('https');
https.get(c2, () => {});
"#;
        assert!(!check(Path::new("a.js"), body).is_empty());
    }

    #[test]
    fn does_not_fire_when_far_apart() {
        let mut body = String::from("const data = Buffer.from('aGVsbG8=', 'base64').toString();\n");
        for _ in 0..20 {
            body.push_str("// padding\n");
        }
        body.push_str("fetch('/x');\n");
        assert!(check(Path::new("a.js"), &body).is_empty());
    }

    #[test]
    fn does_not_fire_on_decode_alone() {
        let body = "const pem = Buffer.from(raw, 'base64');";
        assert!(check(Path::new("a.js"), body).is_empty());
    }
}
