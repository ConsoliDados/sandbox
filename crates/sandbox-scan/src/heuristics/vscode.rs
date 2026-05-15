//! VSCode `tasks.json` autorun heuristic.
//!
//! The YARA rule `contagious_interview_vscode_autorun` matches the exact
//! shape from incident-2026-05-06: `runOn=folderOpen` + `hide=true` +
//! `reveal=never` + `node .vscode/<payload>`. This heuristic catches the
//! looser shape: ANY task with `runOn=folderOpen` *and* an executable
//! command. Folder-open autorun is rare-to-nonexistent in legitimate dev
//! workflows; flagging it forces the user to look at it before opening the
//! workspace in VSCode (which would otherwise run it silently).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

use super::regex_util::compile;
use crate::findings::{Finding, Severity};

const RULE_ID: &str = "heuristics/vscode_tasks_autorun";

fn run_on_folder_open() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| compile(r#""runOn"\s*:\s*"folderOpen""#))
}

pub(super) fn check_tasks_json(rel: &Path, body: &str) -> Vec<Finding> {
    if !run_on_folder_open().is_match(body) {
        return Vec::new();
    }
    vec![Finding {
        rule_id: RULE_ID.into(),
        severity: Severity::High,
        message: ".vscode/tasks.json runs a task on `folderOpen` — VSCode \
                  would execute this silently when the workspace is opened"
            .into(),
        path: PathBuf::from(rel),
        line: line_of(body, run_on_folder_open()),
        remediation: Some(
            "Inspect the task `command`. If it's not something you authored, \
             remove `tasks.json` (or change `runOn` to a manual trigger) \
             before opening this folder in VSCode."
                .into(),
        ),
    }]
}

fn line_of(body: &str, re: &Regex) -> Option<u32> {
    let m = re.find(body)?;
    Some(1 + body[..m.start()].bytes().filter(|b| *b == b'\n').count() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_on_folder_open() {
        let body = r#"{"tasks": [{"label":"x","runOn":"folderOpen"}]}"#;
        let out = check_tasks_json(Path::new(".vscode/tasks.json"), body);
        let summary: Vec<_> = out
            .iter()
            .map(|f| (f.rule_id.as_str(), f.severity))
            .collect();
        assert_eq!(summary, vec![(RULE_ID, Severity::High)]);
    }

    #[test]
    fn ignores_manual_trigger() {
        let body = r#"{"tasks": [{"label":"x","runOn":"manual"}]}"#;
        assert!(check_tasks_json(Path::new(".vscode/tasks.json"), body).is_empty());
    }

    #[test]
    fn detects_line_number() {
        let body = "{\n  \"tasks\": [{\n    \"runOn\": \"folderOpen\"\n  }]\n}";
        let out = check_tasks_json(Path::new(".vscode/tasks.json"), body);
        assert_eq!(out.first().and_then(|f| f.line), Some(3));
    }
}
