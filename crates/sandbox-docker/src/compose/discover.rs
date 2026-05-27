//! Compose file discovery (ADR-0010 § Decision item 3).
//!
//! Default: walk the project root and match basenames against
//! `^(docker-compose|compose).*\.ya?ml$`. Single match wins; zero matches is
//! a soft outcome (project has no compose deps); multi-match is a hard error
//! that requires `--compose-file PATH` to disambiguate.
//!
//! The ADR's literal glob (`**/compose*.y{,a}ml`) wouldn't match
//! `docker-compose.yml` since the basename starts with `docker-`. The regex
//! used here covers both names while staying narrow enough to skip
//! `production-compose.yml`-style false positives.

use std::path::{Path, PathBuf};

use regex::Regex;
use walkdir::WalkDir;

use crate::{Error, Result};

/// Directories that compose discovery never descends into. Each is a
/// well-known cache / build output that would only host compose files
/// belonging to dependencies, not the project itself.
pub const DISCOVER_SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    ".git",
    "dist",
    "build",
    ".next",
    "vendor",
    ".venv",
    "__pycache__",
];

/// How deep into the project tree the walker descends. Realistic compose
/// files live at the root or one or two folders down (`infra/`, `services/`,
/// `docker/`). Capping at 4 keeps the walk fast in large monorepos.
const MAX_DEPTH: usize = 4;

/// Result of a discovery run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// No compose file present. Caller decides whether that is an error
    /// (e.g. `--with-deps` was passed) or fine (the default case).
    None,
    /// Exactly one compose file was found.
    Found(PathBuf),
}

/// Find the project's compose file.
///
/// `project_root` is the absolute, canonical project path (the same path
/// `Project::resolve` produces).
///
/// If `override_path` is `Some`, discovery is skipped and the override is
/// validated (must exist and be a regular file). Override paths are returned
/// canonicalized so downstream `docker compose -f <path>` invocations are
/// stable.
///
/// Multi-match returns [`Error::ComposeMultipleMatches`] with the candidate
/// list — the user disambiguates with `--compose-file PATH`.
pub fn discover(project_root: &Path, override_path: Option<&Path>) -> Result<Outcome> {
    if let Some(forced) = override_path {
        return resolve_override(forced);
    }
    let mut matches = scan(project_root);
    matches.sort();

    match matches.as_slice() {
        [] => Ok(Outcome::None),
        [_only] => {
            // Unwrap is sound: the slice match guarantees exactly one element.
            // `into_iter().next()` returning None here would be impossible.
            let path = matches.into_iter().next().ok_or(Error::ComposeIo {
                path: project_root.to_path_buf(),
                source: std::io::Error::other("compose discovery invariant violated"),
            })?;
            Ok(Outcome::Found(path))
        }
        _ => Err(Error::ComposeMultipleMatches {
            candidates: matches,
        }),
    }
}

fn resolve_override(path: &Path) -> Result<Outcome> {
    if !path.exists() {
        return Err(Error::ComposeOverrideMissing {
            path: path.to_path_buf(),
        });
    }
    if !path.is_file() {
        return Err(Error::ComposeOverrideNotFile {
            path: path.to_path_buf(),
        });
    }
    let canonical = std::fs::canonicalize(path).map_err(|source| Error::ComposeIo {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(Outcome::Found(canonical))
}

fn scan(project_root: &Path) -> Vec<PathBuf> {
    // Compiled once per call. `Regex::new` on a literal is fast enough that
    // a `OnceLock` would be premature optimization for a one-shot CLI.
    let pattern = match Regex::new(r"^(docker-compose|compose).*\.ya?ml$") {
        Ok(re) => re,
        Err(_) => return Vec::new(),
    };

    WalkDir::new(project_root)
        .max_depth(MAX_DEPTH)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !is_skipped(entry.file_name().to_str()))
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| pattern.is_match(name))
        })
        .map(|entry| entry.into_path())
        .collect()
}

fn is_skipped(name: Option<&str>) -> bool {
    name.is_some_and(|n| DISCOVER_SKIP_DIRS.contains(&n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    fn touch(dir: &Path, rel: &str) -> std::io::Result<PathBuf> {
        let full = dir.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&full, "services: {}\n")?;
        Ok(full)
    }

    #[test]
    fn no_compose_file_returns_none() -> TestResult {
        let tmp = TempDir::new()?;
        touch(tmp.path(), "package.json")?;
        assert_eq!(discover(tmp.path(), None)?, Outcome::None);
        Ok(())
    }

    #[test]
    fn single_docker_compose_at_root_wins() -> TestResult {
        let tmp = TempDir::new()?;
        let expected = touch(tmp.path(), "docker-compose.yml")?;
        assert_eq!(discover(tmp.path(), None)?, Outcome::Found(expected));
        Ok(())
    }

    #[test]
    fn compose_yaml_variant_also_matches() -> TestResult {
        let tmp = TempDir::new()?;
        let expected = touch(tmp.path(), "compose.yaml")?;
        assert_eq!(discover(tmp.path(), None)?, Outcome::Found(expected));
        Ok(())
    }

    #[test]
    fn finds_compose_one_level_deep() -> TestResult {
        let tmp = TempDir::new()?;
        let expected = touch(tmp.path(), "infra/docker-compose.yml")?;
        assert_eq!(discover(tmp.path(), None)?, Outcome::Found(expected));
        Ok(())
    }

    #[test]
    fn multi_match_returns_error_with_candidates() -> TestResult {
        let tmp = TempDir::new()?;
        let a = touch(tmp.path(), "docker-compose.yml")?;
        let b = touch(tmp.path(), "compose.dev.yml")?;
        let err = discover(tmp.path(), None).err();
        assert!(matches!(
            err,
            Some(Error::ComposeMultipleMatches { ref candidates })
            if {
                let mut got = candidates.clone();
                got.sort();
                let mut want = vec![a.clone(), b.clone()];
                want.sort();
                got == want
            }
        ));
        Ok(())
    }

    #[test]
    fn skips_node_modules_and_friends() -> TestResult {
        let tmp = TempDir::new()?;
        touch(tmp.path(), "node_modules/some-dep/docker-compose.yml")?;
        touch(tmp.path(), "target/something/compose.yml")?;
        touch(tmp.path(), ".git/hooks/compose.yml")?;
        // Real compose at root — must still be the only finding.
        let expected = touch(tmp.path(), "docker-compose.yml")?;
        assert_eq!(discover(tmp.path(), None)?, Outcome::Found(expected));
        Ok(())
    }

    #[test]
    fn excludes_lookalikes_outside_the_pattern() -> TestResult {
        let tmp = TempDir::new()?;
        // These should NOT be picked up by discovery — the basename has to
        // start with `compose` or `docker-compose`, not contain it.
        touch(tmp.path(), "production-compose.yml")?;
        touch(tmp.path(), "compose-helper.sh")?;
        touch(tmp.path(), "Composefile")?;
        assert_eq!(discover(tmp.path(), None)?, Outcome::None);
        Ok(())
    }

    #[test]
    fn does_not_descend_past_max_depth() -> TestResult {
        let tmp = TempDir::new()?;
        // 5 levels deep — exceeds MAX_DEPTH (4).
        touch(tmp.path(), "a/b/c/d/e/docker-compose.yml")?;
        assert_eq!(discover(tmp.path(), None)?, Outcome::None);
        Ok(())
    }

    #[test]
    fn override_path_short_circuits_discovery() -> TestResult {
        let tmp = TempDir::new()?;
        // Plant a "real" file at root that discovery would otherwise pick.
        touch(tmp.path(), "docker-compose.yml")?;
        // Override points at a sibling — discovery must honor it.
        let forced = touch(tmp.path(), "custom/my-stack.yml")?;
        let canonical = forced.canonicalize()?;
        assert_eq!(
            discover(tmp.path(), Some(&forced))?,
            Outcome::Found(canonical)
        );
        Ok(())
    }

    #[test]
    fn override_missing_path_errors() -> TestResult {
        let tmp = TempDir::new()?;
        let missing = tmp.path().join("nope.yml");
        let err = discover(tmp.path(), Some(&missing)).err();
        assert!(matches!(
            err,
            Some(Error::ComposeOverrideMissing { ref path }) if path == &missing
        ));
        Ok(())
    }

    #[test]
    fn override_pointing_at_directory_errors() -> TestResult {
        let tmp = TempDir::new()?;
        let dir = tmp.path().join("a-dir");
        fs::create_dir(&dir)?;
        let err = discover(tmp.path(), Some(&dir)).err();
        assert!(matches!(
            err,
            Some(Error::ComposeOverrideNotFile { ref path }) if path == &dir
        ));
        Ok(())
    }
}
