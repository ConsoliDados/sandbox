use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// Per-project metadata persisted at
/// `$XDG_DATA_HOME/sandbox/containers/<hash_short>/meta.toml`.
///
/// Schema is intentionally minimal at v0; new fields should be `#[serde(default)]`
/// and additive so old state files keep loading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Meta {
    pub container_name: String,
    pub project_path: PathBuf,
    pub project_hash: String,
    pub language: String,

    #[serde(default)]
    pub created_at: Option<String>,

    #[serde(default)]
    pub last_run_at: Option<String>,

    #[serde(default)]
    pub named_volumes: Vec<String>,

    /// Lockfile names (relative to project workdir) that are bind-mounted from
    /// `Paths::lockfiles_dir(hash)` in `safe`/`paranoid`. Empty when the project
    /// last ran under `unsafe` or has no lockfiles. See ADR-0003.
    #[serde(default)]
    pub lockfiles: Vec<String>,

    /// Host ports the project asked to expose through the reverse proxy
    /// (Phase 5). Source: CLI `--expose` overrides ∪ manifest-driven
    /// auto-detection. Read by `sandbox proxy start` to render Traefik
    /// entryPoints. Empty when the project has never opted in.
    #[serde(default)]
    pub ports: Vec<u16>,

    /// Compose deps brought up by `sandbox run --with-deps` (Phase 6, ADR-0010).
    /// `None` when the project ran without `--with-deps`. Read by
    /// `sandbox down --with-deps` and `sandbox nuke` to know what to tear
    /// down — we only touch what we started.
    #[serde(default)]
    pub compose: Option<ComposeMeta>,
}

/// Subset of compose lifecycle state we persist per project.
///
/// `network` is the actual Docker network the deps live on after the
/// `sandbox run` post-`up` rewire — `sandbox-compose-<short>` in safe mode
/// (created with `--internal`) or `<project_name>_default` in `--network`
/// mode (compose-created bridge). Cleanup operations use it to decide
/// whether to `docker network rm` after `compose down`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComposeMeta {
    pub file: PathBuf,
    pub project_name: String,
    pub services: Vec<String>,
    pub network: String,
}

impl Meta {
    /// Load `meta.toml` from a per-project state directory.
    pub fn load(state_dir: &Path) -> Result<Self> {
        let path = state_dir.join("meta.toml");
        let raw = std::fs::read_to_string(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        toml::from_str(&raw).map_err(|e| Error::InvalidManifest {
            path,
            reason: e.to_string(),
        })
    }

    /// Persist `meta.toml` into a per-project state directory, creating the
    /// directory if it doesn't yet exist.
    pub fn save(&self, state_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(state_dir).map_err(|source| Error::Io {
            path: state_dir.to_path_buf(),
            source,
        })?;
        let path = state_dir.join("meta.toml");
        let raw = toml::to_string_pretty(self).map_err(|e| Error::InvalidManifest {
            path: path.clone(),
            reason: e.to_string(),
        })?;
        std::fs::write(&path, raw).map_err(|source| Error::Io { path, source })
    }

    /// True if a state directory has been initialised for this project.
    pub fn exists_at(state_dir: &Path) -> bool {
        state_dir.join("meta.toml").is_file()
    }

    /// Enumerate every per-project state under `containers_dir` and load each
    /// `meta.toml`. Returns `Vec<Meta>` sorted by `container_name` for stable
    /// output. Missing `containers_dir` yields an empty vec (no projects yet);
    /// subdirs without a readable `meta.toml` are skipped with a warning so a
    /// single corrupt entry doesn't poison `sandbox ps`.
    pub fn load_all(containers_dir: &Path) -> Result<Vec<Self>> {
        let read_dir = match std::fs::read_dir(containers_dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(Error::Io {
                    path: containers_dir.to_path_buf(),
                    source,
                });
            }
        };

        let mut out = Vec::new();
        for entry in read_dir {
            let entry = entry.map_err(|source| Error::Io {
                path: containers_dir.to_path_buf(),
                source,
            })?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if !Self::exists_at(&path) {
                continue;
            }
            match Self::load(&path) {
                Ok(meta) => out.push(meta),
                Err(e) => {
                    tracing::warn!(?path, error = %e, "skipping unreadable state");
                }
            }
        }
        out.sort_by(|a, b| a.container_name.cmp(&b.container_name));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Meta {
        Meta {
            container_name: "sandbox-abcdef012345".to_string(),
            project_path: PathBuf::from("/tmp/some-project"),
            project_hash: "abcdef012345".to_string(),
            language: "rust".to_string(),
            created_at: Some("2026-05-06T20:00:00Z".to_string()),
            last_run_at: None,
            named_volumes: vec!["sandbox-abcdef012345-target".to_string()],
            lockfiles: vec!["Cargo.lock".to_string()],
            ports: vec![],
            compose: None,
        }
    }

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn save_then_load_roundtrip() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let dir = tmp.path().join("containers").join("abcdef012345");
        let m = fixture();
        m.save(&dir)?;
        let loaded = Meta::load(&dir)?;
        assert_eq!(loaded, m);
        Ok(())
    }

    #[test]
    fn save_creates_missing_dir() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let dir = tmp.path().join("a").join("b").join("c");
        assert!(!dir.exists());
        fixture().save(&dir)?;
        assert!(dir.is_dir());
        assert!(dir.join("meta.toml").is_file());
        Ok(())
    }

    #[test]
    fn exists_at_reports_correctly() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let dir = tmp.path().join("nope");
        assert!(!Meta::exists_at(&dir));
        fixture().save(&dir)?;
        assert!(Meta::exists_at(&dir));
        Ok(())
    }

    #[test]
    fn load_missing_meta_yields_io_error() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let result = Meta::load(tmp.path());
        assert!(matches!(result, Err(Error::Io { .. })));
        Ok(())
    }

    #[test]
    fn load_garbage_yields_invalid_manifest() -> TestResult {
        let tmp = tempfile::tempdir()?;
        std::fs::write(tmp.path().join("meta.toml"), "[[[ not toml")?;
        let result = Meta::load(tmp.path());
        assert!(matches!(result, Err(Error::InvalidManifest { .. })));
        Ok(())
    }

    #[test]
    fn load_all_returns_empty_when_dir_missing() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let containers = tmp.path().join("containers");
        assert!(Meta::load_all(&containers)?.is_empty());
        Ok(())
    }

    #[test]
    fn load_all_collects_and_sorts_metas() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let containers = tmp.path().join("containers");
        let mut second = fixture();
        second.container_name = "sandbox-zzz".into();
        second.save(&containers.join("zzz"))?;
        let mut first = fixture();
        first.container_name = "sandbox-aaa".into();
        first.save(&containers.join("aaa"))?;

        let all = Meta::load_all(&containers)?;
        let names: Vec<_> = all.iter().map(|m| m.container_name.as_str()).collect();
        assert_eq!(names, vec!["sandbox-aaa", "sandbox-zzz"]);
        Ok(())
    }

    #[test]
    fn load_all_skips_dirs_without_meta_and_corrupt_files() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let containers = tmp.path().join("containers");
        std::fs::create_dir_all(containers.join("empty-dir"))?;
        std::fs::create_dir_all(containers.join("corrupt"))?;
        std::fs::write(containers.join("corrupt").join("meta.toml"), "[[[ broken")?;
        fixture().save(&containers.join("good"))?;

        let all = Meta::load_all(&containers)?;
        assert_eq!(
            all.iter().map(|m| m.language.as_str()).collect::<Vec<_>>(),
            vec!["rust"]
        );
        Ok(())
    }

    #[test]
    fn compose_block_roundtrips_when_present() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let mut m = fixture();
        m.compose = Some(ComposeMeta {
            file: PathBuf::from("/tmp/some-project/docker-compose.yml"),
            project_name: "sandbox-abcdef-deps".into(),
            services: vec!["postgres".into(), "redis".into()],
            network: "sandbox-compose-abcdef".into(),
        });
        m.save(tmp.path())?;
        let loaded = Meta::load(tmp.path())?;
        assert_eq!(loaded, m);
        Ok(())
    }

    #[test]
    fn old_state_without_optional_fields_still_loads() -> TestResult {
        let tmp = tempfile::tempdir()?;
        std::fs::write(
            tmp.path().join("meta.toml"),
            r#"
container_name = "sandbox-xyz"
project_path = "/tmp/x"
project_hash = "xyz"
language = "node"
"#,
        )?;
        let m = Meta::load(tmp.path())?;
        assert_eq!(m.language, "node");
        assert!(m.created_at.is_none());
        assert!(m.named_volumes.is_empty());
        assert!(m.lockfiles.is_empty());
        Ok(())
    }
}
