//! Generic JS eval-shape heuristics.
//!
//! Targets the shapes malware uses to defeat static analysis: dynamic
//! `Function` construction (`Function.constructor`, `new Function(...)`),
//! `eval(atob(...))` decoding-then-evaling base64. The Contagious Interview
//! YARA rule is stricter (requires the chainlink C2 + endpoint); this fires
//! on the eval shape alone because there's almost no legitimate reason to
//! build a function from a string at runtime in modern Node code.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

use super::regex_util::compile;
use crate::findings::{Finding, Severity};

const RULE_FN_CONSTRUCTOR: &str = "heuristics/eval_function_constructor";
const RULE_NEW_FUNCTION: &str = "heuristics/eval_new_function_string";
const RULE_EVAL_ATOB: &str = "heuristics/eval_atob";

fn fn_constructor() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    // Covers both `Function.constructor(...)` and the parenthesized form
    // `new (Function.constructor)(...)` that the Lazarus profile.js used.
    R.get_or_init(|| compile(r"Function\s*\.\s*constructor\s*\)?\s*\("))
}

fn new_function_string() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    // `new Function('a','b','return a+b')` — at least one quoted arg
    // distinguishes this from `new Function()` with no body.
    R.get_or_init(|| compile(r#"new\s+Function\s*\(\s*['"`]"#))
}

fn eval_atob() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| compile(r"\beval\s*\(\s*atob\s*\("))
}

pub(super) fn check(rel: &Path, body: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    if let Some(line) = first_line_match(body, fn_constructor()) {
        out.push(Finding {
            rule_id: RULE_FN_CONSTRUCTOR.into(),
            severity: Severity::High,
            message: "`Function.constructor(...)` — building a function from a string at runtime"
                .into(),
            path: PathBuf::from(rel),
            line: Some(line),
            remediation: Some(
                "This is an eval-equivalent that bypasses static analysis. \
                 Audit the surrounding code; legitimate uses are vanishingly rare."
                    .into(),
            ),
        });
    }
    if let Some(line) = first_line_match(body, new_function_string()) {
        out.push(Finding {
            rule_id: RULE_NEW_FUNCTION.into(),
            severity: Severity::Warn,
            message: "`new Function(<string>, ...)` — dynamic function with string body".into(),
            path: PathBuf::from(rel),
            line: Some(line),
            remediation: Some(
                "Same family as Function.constructor. Convert to a regular function \
                 unless this is a known template engine or VM."
                    .into(),
            ),
        });
    }
    if let Some(line) = first_line_match(body, eval_atob()) {
        out.push(Finding {
            rule_id: RULE_EVAL_ATOB.into(),
            severity: Severity::Critical,
            message: "`eval(atob(...))` — evaluating base64-decoded source at runtime".into(),
            path: PathBuf::from(rel),
            line: Some(line),
            remediation: Some(
                "Decode the base64 yourself and inspect the payload. There is no \
                 legitimate developer reason for this construct."
                    .into(),
            ),
        });
    }
    out
}

fn first_line_match(body: &str, re: &Regex) -> Option<u32> {
    let m = re.find(body)?;
    Some(1 + body[..m.start()].bytes().filter(|b| *b == b'\n').count() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_on_function_constructor() {
        let body = "const f = Function.constructor('return 1+1');";
        let f = check(Path::new("a.js"), body);
        assert!(f.iter().any(|x| x.rule_id == RULE_FN_CONSTRUCTOR));
    }

    #[test]
    fn fires_on_parenthesized_function_constructor() {
        // The exact shape from the Lazarus profile.js backdoor.
        let body = "const x = new (Function.constructor)('require','m','...');";
        let f = check(Path::new("a.js"), body);
        assert!(f.iter().any(|x| x.rule_id == RULE_FN_CONSTRUCTOR));
    }

    #[test]
    fn fires_on_new_function_with_string_body() {
        let body = "const f = new Function('a','b','return a+b');";
        assert!(
            check(Path::new("a.js"), body)
                .iter()
                .any(|x| x.rule_id == RULE_NEW_FUNCTION)
        );
    }

    #[test]
    fn does_not_fire_on_bare_new_function() {
        let body = "const f = new Function();";
        assert!(check(Path::new("a.js"), body).is_empty());
    }

    #[test]
    fn fires_critical_on_eval_atob() {
        let body = "eval(atob('Y29uc29sZS5sb2coMSk='));";
        let out = check(Path::new("a.js"), body);
        assert!(
            out.iter()
                .any(|x| x.rule_id == RULE_EVAL_ATOB && x.severity == Severity::Critical)
        );
    }

    #[test]
    fn clean_js_passes() {
        let body = "function add(a, b) { return a + b; } console.log(add(1, 2));";
        assert!(check(Path::new("a.js"), body).is_empty());
    }
}
