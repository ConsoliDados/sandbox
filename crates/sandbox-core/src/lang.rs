use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{Error, Result};

const BUILTIN_NODE: &str = include_str!("../../../languages/node.toml");
const BUILTIN_BUN: &str = include_str!("../../../languages/bun.toml");
const BUILTIN_RUST: &str = include_str!("../../../languages/rust.toml");

/// Identifier for a language manifest. Equal to the manifest's `name` field.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LanguageId(String);

impl LanguageId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for LanguageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Language manifest schema. See `languages/README.md`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LangManifest {
    pub name: String,
    pub display_name: String,
    pub image: String,
    pub detect: Vec<String>,

    #[serde(default)]
    pub priority: u32,

    #[serde(default)]
    pub package_dirs: Vec<String>,

    /// Lockfiles handled with the same isolation strategy as `package_dirs`
    /// (named volume in safe/paranoid; bind mount in unsafe). See ADR-0003.
    #[serde(default)]
    pub lock_files: Vec<String>,

    #[serde(default)]
    pub default_port: Option<u16>,

    #[serde(default)]
    pub extra_packages: Vec<String>,

    #[serde(default = "default_shell")]
    pub shell: String,

    #[serde(default = "default_workdir")]
    pub workdir: String,

    #[serde(default)]
    pub port_detection: Option<PortDetection>,
}

fn default_shell() -> String {
    // Conservative default: every supported base image (debian-slim, alpine,
    // rust:slim, etc.) ships `/bin/bash` or symlinks `sh` → `bash`. When the
    // custom-image pipeline lands (extra_packages → built image with zsh,
    // starship, etc.), language manifests can opt into `/bin/zsh`.
    "/bin/bash".to_string()
}

fn default_workdir() -> String {
    "/app".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortDetection {
    #[serde(default)]
    pub patterns: Vec<String>,
    #[serde(default)]
    pub env_keys: Vec<String>,
}

impl LangManifest {
    pub fn id(&self) -> LanguageId {
        LanguageId::new(&self.name)
    }

    /// Number of `detect` files present in `project_path`. Zero means no match.
    pub fn match_count(&self, project_path: &Path) -> usize {
        self.detect
            .iter()
            .filter(|f| project_path.join(f).exists())
            .count()
    }
}

/// In-memory registry of language manifests, with priority-based detection.
#[derive(Debug, Default, Clone)]
pub struct LanguageRegistry {
    manifests: Vec<LangManifest>,
}

impl LanguageRegistry {
    /// Built-in manifests bundled into the binary at compile time (node, bun, rust).
    pub fn builtin() -> Result<Self> {
        let raws = [
            (BUILTIN_NODE, "<builtin:node>"),
            (BUILTIN_BUN, "<builtin:bun>"),
            (BUILTIN_RUST, "<builtin:rust>"),
        ];
        let mut manifests = Vec::with_capacity(raws.len());
        for (raw, label) in raws {
            let m: LangManifest = toml::from_str(raw).map_err(|e| Error::InvalidManifest {
                path: PathBuf::from(label),
                reason: e.to_string(),
            })?;
            manifests.push(m);
        }
        Ok(Self { manifests })
    }

    /// Load `*.toml` files from a directory. Existing entries with the same
    /// `name` are replaced — user overrides win over built-ins.
    pub fn load_from_dir(&mut self, dir: &Path) -> Result<()> {
        if !dir.exists() {
            return Ok(());
        }
        let entries = std::fs::read_dir(dir).map_err(|source| Error::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| Error::Io {
                path: dir.to_path_buf(),
                source,
            })?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("toml") {
                continue;
            }
            let raw = std::fs::read_to_string(&path).map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;
            let manifest: LangManifest =
                toml::from_str(&raw).map_err(|e| Error::InvalidManifest {
                    path: path.clone(),
                    reason: e.to_string(),
                })?;
            self.replace(manifest);
        }
        Ok(())
    }

    fn replace(&mut self, m: LangManifest) {
        match self.manifests.iter_mut().find(|x| x.name == m.name) {
            Some(slot) => *slot = m,
            None => self.manifests.push(m),
        }
    }

    pub fn get(&self, name: &str) -> Option<&LangManifest> {
        self.manifests.iter().find(|m| m.name == name)
    }

    pub fn require(&self, name: &str) -> Result<&LangManifest> {
        self.get(name)
            .ok_or_else(|| Error::LanguageNotFound(name.to_string()))
    }

    pub fn all(&self) -> &[LangManifest] {
        &self.manifests
    }

    /// Detect the language for a project path.
    ///
    /// Resolution per ADR-0006 / OQ-005:
    /// 1. Filter to manifests with `match_count > 0`.
    /// 2. If empty → [`Error::LanguageNotDetected`].
    /// 3. Among matches, keep the highest `priority`.
    /// 4. Among those, keep the highest `match_count`.
    /// 5. If still tied → [`Error::AmbiguousLanguageMatch`].
    pub fn detect(&self, project_path: &Path) -> Result<&LangManifest> {
        let mut matches: Vec<(&LangManifest, usize)> = self
            .manifests
            .iter()
            .map(|m| (m, m.match_count(project_path)))
            .filter(|(_, n)| *n > 0)
            .collect();

        if matches.is_empty() {
            return Err(Error::LanguageNotDetected(project_path.to_path_buf()));
        }

        let max_priority = matches.iter().map(|(m, _)| m.priority).max().unwrap_or(0);
        matches.retain(|(m, _)| m.priority == max_priority);

        let max_count = matches.iter().map(|(_, n)| *n).max().unwrap_or(0);
        matches.retain(|(_, n)| *n == max_count);

        if matches.len() == 1 {
            // Length is exactly 1; first() is safe.
            return matches
                .first()
                .map(|(m, _)| *m)
                .ok_or(Error::LanguageNotDetected(project_path.to_path_buf()));
        }

        let candidates = matches.iter().map(|(m, _)| m.name.clone()).collect();
        Err(Error::AmbiguousLanguageMatch {
            path: project_path.to_path_buf(),
            candidates,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    fn touch(dir: &Path, name: &str) -> std::io::Result<()> {
        std::fs::write(dir.join(name), b"")
    }

    #[test]
    fn builtin_loads_three_manifests() -> TestResult {
        let reg = LanguageRegistry::builtin()?;
        let names: Vec<&str> = reg.all().iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"node"));
        assert!(names.contains(&"bun"));
        assert!(names.contains(&"rust"));
        Ok(())
    }

    #[test]
    fn builtin_node_carries_lock_files() -> TestResult {
        let reg = LanguageRegistry::builtin()?;
        let node = reg.require("node")?;
        assert!(node.lock_files.iter().any(|f| f == "package-lock.json"));
        Ok(())
    }

    #[test]
    fn lock_files_default_to_empty() -> TestResult {
        let m: LangManifest = toml::from_str(
            r#"
name = "x"
display_name = "X"
image = "x:1"
detect = ["x"]
"#,
        )?;
        assert!(m.lock_files.is_empty());
        Ok(())
    }

    #[test]
    fn detect_picks_rust_over_node_via_priority() -> TestResult {
        let tmp = tempfile::tempdir()?;
        touch(tmp.path(), "package.json")?;
        touch(tmp.path(), "Cargo.toml")?;

        let reg = LanguageRegistry::builtin()?;
        let m = reg.detect(tmp.path())?;
        assert_eq!(m.name, "rust");
        Ok(())
    }

    #[test]
    fn detect_picks_bun_over_node_via_priority() -> TestResult {
        let tmp = tempfile::tempdir()?;
        touch(tmp.path(), "package.json")?;
        touch(tmp.path(), "bun.lock")?;

        let reg = LanguageRegistry::builtin()?;
        let m = reg.detect(tmp.path())?;
        assert_eq!(m.name, "bun");
        Ok(())
    }

    #[test]
    fn detect_returns_node_when_only_package_json() -> TestResult {
        let tmp = tempfile::tempdir()?;
        touch(tmp.path(), "package.json")?;

        let reg = LanguageRegistry::builtin()?;
        let m = reg.detect(tmp.path())?;
        assert_eq!(m.name, "node");
        Ok(())
    }

    #[test]
    fn detect_errors_when_no_match() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let reg = LanguageRegistry::builtin()?;
        let result = reg.detect(tmp.path());
        assert!(matches!(result, Err(Error::LanguageNotDetected(_))));
        Ok(())
    }

    #[test]
    fn user_dir_overrides_builtin_by_name() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let user_manifest = r#"
name = "node"
display_name = "Node (user override)"
image = "node:99"
detect = ["package.json"]
priority = 999
"#;
        std::fs::write(tmp.path().join("node.toml"), user_manifest)?;

        let mut reg = LanguageRegistry::builtin()?;
        reg.load_from_dir(tmp.path())?;

        let node = reg.require("node")?;
        assert_eq!(node.image, "node:99");
        assert_eq!(node.priority, 999);
        Ok(())
    }

    #[test]
    fn load_from_missing_dir_is_noop() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let missing = tmp.path().join("does-not-exist");
        let mut reg = LanguageRegistry::default();
        reg.load_from_dir(&missing)?;
        assert!(reg.all().is_empty());
        Ok(())
    }

    #[test]
    fn deny_unknown_fields_rejects_typos() {
        let bad = r#"
name = "x"
display_name = "X"
image = "x:1"
detect = ["x"]
priorty = 5
"#;
        let r: std::result::Result<LangManifest, _> = toml::from_str(bad);
        assert!(r.is_err(), "typo should be rejected by deny_unknown_fields");
    }
}
