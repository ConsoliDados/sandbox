//! `.env` file parser, narrow to the keys we care about for port detection.
//!
//! We don't pull a full dotenv crate — the format we need to parse is tiny
//! (KEY=VALUE per line, optional `#` comment, optional quotes around the
//! value) and the surface stays auditable.

use std::path::Path;

use crate::Result;

/// Scan `<project_root>/.env` for any of `keys` and collect the parsed `u16`
/// port numbers. Missing file returns empty. Non-numeric or out-of-range
/// values are skipped silently — port detection is best-effort.
pub(super) fn parse_env_file(project_root: &Path, keys: &[String]) -> Result<Vec<u16>> {
    let path = project_root.join(".env");
    let body = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(crate::Error::Io { path, source }),
    };
    Ok(parse_body(&body, keys))
}

fn parse_body(body: &str, keys: &[String]) -> Vec<u16> {
    let mut out = Vec::new();
    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if !keys.iter().any(|k| k == key) {
            continue;
        }
        let value = strip_quotes(value.trim());
        let cleaned = value.split('#').next().unwrap_or("").trim();
        if let Ok(port) = cleaned.parse::<u16>() {
            out.push(port);
        }
    }
    out
}

fn strip_quotes(s: &str) -> &str {
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        let mut chars = s.chars();
        chars.next();
        chars.next_back();
        return chars.as_str();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    fn keys(slice: &[&str]) -> Vec<String> {
        slice.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn missing_env_returns_empty() -> TestResult {
        let tmp = tempfile::tempdir()?;
        assert!(parse_env_file(tmp.path(), &keys(&["PORT"]))?.is_empty());
        Ok(())
    }

    #[test]
    fn extracts_bare_port_assignment() {
        assert_eq!(parse_body("PORT=3000\n", &keys(&["PORT"])), vec![3000]);
    }

    #[test]
    fn ignores_comments_and_blank_lines() {
        let body = "\n# comment\nPORT=4000\n#PORT=9999\n";
        assert_eq!(parse_body(body, &keys(&["PORT"])), vec![4000]);
    }

    #[test]
    fn strips_double_and_single_quotes() {
        let body = "PORT=\"3001\"\nAPP_PORT='5007'\n";
        assert_eq!(
            parse_body(body, &keys(&["PORT", "APP_PORT"])),
            vec![3001, 5007]
        );
    }

    #[test]
    fn drops_trailing_inline_comment() {
        let body = "PORT=8080 # primary\n";
        assert_eq!(parse_body(body, &keys(&["PORT"])), vec![8080]);
    }

    #[test]
    fn skips_keys_outside_allowlist() {
        let body = "PORT=3000\nDATABASE_URL=postgres://x\n";
        assert_eq!(parse_body(body, &keys(&["APP_PORT"])), Vec::<u16>::new());
    }

    #[test]
    fn skips_unparseable_values() {
        let body = "PORT=abc\nAPP_PORT=99999\nHTTP_PORT=70000\nGOOD=80\n";
        // 70000 / 99999 don't fit in u16; abc isn't a number; only GOOD passes.
        assert_eq!(
            parse_body(body, &keys(&["PORT", "APP_PORT", "HTTP_PORT", "GOOD"]),),
            vec![80]
        );
    }

    #[test]
    fn reads_real_file() -> TestResult {
        let tmp = tempfile::tempdir()?;
        std::fs::write(tmp.path().join(".env"), "PORT=3000\nAPP_PORT=5007\n")?;
        let ports = parse_env_file(tmp.path(), &keys(&["PORT", "APP_PORT"]))?;
        assert_eq!(ports, vec![3000, 5007]);
        Ok(())
    }
}
