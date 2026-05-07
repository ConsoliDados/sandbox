use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// XDG-aware filesystem locations used by `sandbox`.
///
/// On Linux these honor `$XDG_CONFIG_HOME`, `$XDG_DATA_HOME`, `$XDG_CACHE_HOME`
/// (defaulting to `~/.config/sandbox`, `~/.local/share/sandbox`, `~/.cache/sandbox`).
/// See ADR-0007.
#[derive(Debug, Clone)]
pub struct Paths {
    config: PathBuf,
    data: PathBuf,
    cache: PathBuf,
}

impl Paths {
    /// Resolve XDG paths from the current environment. Does **not** create the
    /// directories; call [`Self::ensure_dirs`] for that.
    pub fn discover() -> Result<Self> {
        let dirs = directories::ProjectDirs::from("", "", "sandbox").ok_or(Error::XdgNoHome)?;
        Ok(Self {
            config: dirs.config_dir().to_path_buf(),
            data: dirs.data_dir().to_path_buf(),
            cache: dirs.cache_dir().to_path_buf(),
        })
    }

    /// Construct paths from explicit roots. Useful for tests and for the
    /// `--config` override (where the user points us at a single tree).
    pub fn from_roots(config: PathBuf, data: PathBuf, cache: PathBuf) -> Self {
        Self {
            config,
            data,
            cache,
        }
    }

    /// Create all three top-level dirs if they don't exist.
    pub fn ensure_dirs(&self) -> Result<()> {
        for p in [&self.config, &self.data, &self.cache] {
            std::fs::create_dir_all(p).map_err(|source| Error::Io {
                path: p.clone(),
                source,
            })?;
        }
        Ok(())
    }

    pub fn config(&self) -> &Path {
        &self.config
    }

    pub fn data(&self) -> &Path {
        &self.data
    }

    pub fn cache(&self) -> &Path {
        &self.cache
    }

    pub fn config_file(&self) -> PathBuf {
        self.config.join("config.toml")
    }

    pub fn user_languages_dir(&self) -> PathBuf {
        self.config.join("languages")
    }

    pub fn user_zshrc_sandbox(&self) -> PathBuf {
        self.config.join("zsh").join(".zshrc.sandbox")
    }

    pub fn containers_dir(&self) -> PathBuf {
        self.data.join("containers")
    }

    pub fn container_state_dir(&self, hash_short: &str) -> PathBuf {
        self.containers_dir().join(hash_short)
    }

    pub fn proxy_dir(&self) -> PathBuf {
        self.data.join("proxy")
    }

    pub fn scan_cache_dir(&self) -> PathBuf {
        self.cache.join("scan")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn ensure_dirs_creates_all_three() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let paths = Paths::from_roots(
            tmp.path().join("config"),
            tmp.path().join("data"),
            tmp.path().join("cache"),
        );
        paths.ensure_dirs()?;
        assert!(paths.config().is_dir());
        assert!(paths.data().is_dir());
        assert!(paths.cache().is_dir());
        Ok(())
    }

    #[test]
    fn derived_paths_compose_under_roots() {
        let paths = Paths::from_roots(
            PathBuf::from("/c"),
            PathBuf::from("/d"),
            PathBuf::from("/k"),
        );
        assert_eq!(paths.config_file(), PathBuf::from("/c/config.toml"));
        assert_eq!(paths.user_languages_dir(), PathBuf::from("/c/languages"));
        assert_eq!(
            paths.container_state_dir("abc123"),
            PathBuf::from("/d/containers/abc123")
        );
        assert_eq!(paths.scan_cache_dir(), PathBuf::from("/k/scan"));
    }
}
