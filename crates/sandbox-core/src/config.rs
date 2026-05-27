use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Profile, Result};

/// Top-level configuration loaded from `~/.config/sandbox/config.toml`.
///
/// Missing fields fall back to defaults. Built-in profiles (`default`,
/// `unsafe`, `paranoid`) are merged in unless the user overrode them by name.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub defaults: Defaults,

    #[serde(default)]
    pub scan: ScanConfig,

    #[serde(default)]
    pub proxy: ProxyConfig,

    #[serde(default)]
    pub profile: HashMap<String, Profile>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    #[serde(default = "default_shell")]
    pub shell: String,

    #[serde(default)]
    pub language_dirs: Vec<PathBuf>,

    #[serde(default = "default_profile_name")]
    pub profile: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScanConfig {
    #[serde(default = "default_true")]
    pub cache: bool,

    #[serde(default = "default_severity")]
    pub severity_threshold: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyConfig {
    #[serde(default = "default_proxy_domain")]
    pub domain: String,

    #[serde(default)]
    pub auto_start: bool,
}

fn default_shell() -> String {
    "zsh".to_string()
}

fn default_profile_name() -> String {
    "default".to_string()
}

fn default_severity() -> String {
    "warn".to_string()
}

fn default_proxy_domain() -> String {
    "sandbox.localhost".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            shell: default_shell(),
            language_dirs: Vec::new(),
            profile: default_profile_name(),
        }
    }
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            cache: true,
            severity_threshold: default_severity(),
        }
    }
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            domain: default_proxy_domain(),
            auto_start: false,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        let mut profile = HashMap::new();
        profile.insert("default".to_string(), Profile::default_profile());
        profile.insert("unsafe".to_string(), Profile::unsafe_profile());
        profile.insert("paranoid".to_string(), Profile::paranoid_profile());
        Self {
            defaults: Defaults::default(),
            scan: ScanConfig::default(),
            proxy: ProxyConfig::default(),
            profile,
        }
    }
}

impl Config {
    /// Load from disk, falling back to defaults if the file does not exist.
    /// Built-in profiles are always merged in (user values win).
    pub fn load_or_default(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let mut config: Config = toml::from_str(&raw).map_err(|e| Error::InvalidManifest {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })?;
        config.merge_builtin_profiles();
        Ok(config)
    }

    fn merge_builtin_profiles(&mut self) {
        for (name, factory) in [
            ("default", Profile::default_profile as fn() -> Profile),
            ("unsafe", Profile::unsafe_profile),
            ("paranoid", Profile::paranoid_profile),
        ] {
            self.profile.entry(name.to_string()).or_insert_with(factory);
        }
    }

    /// Look up a named profile, returning [`Error::LanguageNotFound`]'s
    /// nearest equivalent for missing profiles. Profile lookup uses the same
    /// "named lookup" path as language manifests.
    pub fn profile(&self, name: &str) -> Result<&Profile> {
        self.profile
            .get(name)
            .ok_or_else(|| Error::ProfileNotFound(name.to_string()))
    }

    pub fn profiles(&self) -> &HashMap<String, Profile> {
        &self.profile
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn missing_file_returns_defaults() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let missing = tmp.path().join("does-not-exist.toml");
        let cfg = Config::load_or_default(&missing)?;
        assert_eq!(cfg.defaults.shell, "zsh");
        assert_eq!(cfg.defaults.profile, "default");
        assert!(cfg.profile.contains_key("default"));
        assert!(cfg.profile.contains_key("unsafe"));
        assert!(cfg.profile.contains_key("paranoid"));
        Ok(())
    }

    #[test]
    fn user_values_override_builtin() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[defaults]
shell = "bash"
profile = "paranoid"

[scan]
cache = false
severity_threshold = "high"

[proxy]
domain = "dev.test"
auto_start = true

[profile.default]
name = "default"
unsafe = false
network = false
ephemeral_home = true
cap_drop = "ALL"
no_new_privileges = true
cpu = 8.0
memory_mb = 16384
no_compose_deps = false
"#,
        )?;

        let cfg = Config::load_or_default(&path)?;
        assert_eq!(cfg.defaults.shell, "bash");
        assert_eq!(cfg.defaults.profile, "paranoid");
        assert!(!cfg.scan.cache);
        assert_eq!(cfg.scan.severity_threshold, "high");
        assert_eq!(cfg.proxy.domain, "dev.test");

        let default_profile = cfg.profile("default")?;
        assert_eq!(default_profile.cpu, Some(8.0));
        assert_eq!(default_profile.memory_mb, Some(16384));
        // Builtin profiles still injected for the others
        assert!(cfg.profile.contains_key("unsafe"));
        assert!(cfg.profile.contains_key("paranoid"));
        Ok(())
    }

    #[test]
    fn missing_profile_yields_typed_error() {
        let cfg = Config::default();
        let result = cfg.profile("nope");
        assert!(matches!(result, Err(Error::ProfileNotFound(_))));
    }

    #[test]
    fn invalid_toml_surfaces_invalid_manifest() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "not = valid = toml")?;
        let result = Config::load_or_default(&path);
        assert!(matches!(result, Err(Error::InvalidManifest { .. })));
        Ok(())
    }
}
