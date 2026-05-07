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
        }
    }

    #[test]
    fn save_then_load_roundtrip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("containers").join("abcdef012345");
        let m = fixture();
        m.save(&dir).expect("save");
        let loaded = Meta::load(&dir).expect("load");
        assert_eq!(loaded, m);
    }

    #[test]
    fn save_creates_missing_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("a").join("b").join("c");
        assert!(!dir.exists());
        fixture().save(&dir).expect("save");
        assert!(dir.is_dir());
        assert!(dir.join("meta.toml").is_file());
    }

    #[test]
    fn exists_at_reports_correctly() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("nope");
        assert!(!Meta::exists_at(&dir));
        fixture().save(&dir).expect("save");
        assert!(Meta::exists_at(&dir));
    }

    #[test]
    fn load_missing_meta_yields_io_error() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let err = Meta::load(tmp.path()).expect_err("should fail");
        assert!(matches!(err, Error::Io { .. }));
    }

    #[test]
    fn load_garbage_yields_invalid_manifest() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join("meta.toml"), "[[[ not toml").expect("write");
        let err = Meta::load(tmp.path()).expect_err("should fail");
        assert!(matches!(err, Error::InvalidManifest { .. }));
    }

    #[test]
    fn old_state_without_optional_fields_still_loads() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            tmp.path().join("meta.toml"),
            r#"
container_name = "sandbox-xyz"
project_path = "/tmp/x"
project_hash = "xyz"
language = "node"
"#,
        )
        .expect("write");
        let m = Meta::load(tmp.path()).expect("load");
        assert_eq!(m.language, "node");
        assert!(m.created_at.is_none());
        assert!(m.named_volumes.is_empty());
    }
}
