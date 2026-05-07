use std::path::{Path, PathBuf};

use crate::{Error, LanguageId, LanguageRegistry, ProjectHash, Result, project_hash};

/// Docker container name derived from the project hash.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContainerName(String);

impl ContainerName {
    pub fn from_hash(hash: &ProjectHash) -> Self {
        Self(format!("sandbox-{}", hash.short()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ContainerName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Named Docker volume scoped to one project + one package directory.
///
/// Naming pattern: `sandbox-<hash.short()>-<sanitized_package_dir>`.
/// Non-alphanumerics in the package dir are replaced with `_`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NamedVolume(String);

impl NamedVolume {
    pub fn for_package_dir(hash: &ProjectHash, package_dir: &str) -> Self {
        let safe: String = package_dir
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        Self(format!("sandbox-{}-{}", hash.short(), safe))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for NamedVolume {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A resolved project: canonical path + identity + language-derived facts.
///
/// Construct via [`Project::resolve`]. Pure data after construction.
#[derive(Debug, Clone)]
pub struct Project {
    pub path: PathBuf,
    pub hash: ProjectHash,
    pub language: LanguageId,
    pub container_name: ContainerName,
    pub package_dirs: Vec<String>,
}

impl Project {
    /// Resolve a project from a (possibly relative) path.
    ///
    /// Steps:
    /// 1. Canonicalize the path. Fails with [`Error::Io`] if it cannot be
    ///    resolved or [`Error::ProjectPathInvalid`] if it is not a directory.
    /// 2. Pick a language: `lang_override` if provided, otherwise
    ///    [`LanguageRegistry::detect`].
    /// 3. Compute the project hash (canonical path bytes; ADR-0009).
    /// 4. Derive the container name and copy the manifest's `package_dirs`.
    pub fn resolve(
        path: &Path,
        registry: &LanguageRegistry,
        lang_override: Option<&str>,
    ) -> Result<Self> {
        let canonical = std::fs::canonicalize(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        if !canonical.is_dir() {
            return Err(Error::ProjectPathInvalid(canonical));
        }

        let manifest = match lang_override {
            Some(name) => registry.require(name)?,
            None => registry.detect(&canonical)?,
        };

        let hash = project_hash(&canonical)?;
        let container_name = ContainerName::from_hash(&hash);
        let language = manifest.id();
        let package_dirs = manifest.package_dirs.clone();

        Ok(Self {
            path: canonical,
            hash,
            language,
            container_name,
            package_dirs,
        })
    }

    /// All named volumes this project allocates, one per `package_dir`.
    pub fn named_volumes(&self) -> Vec<NamedVolume> {
        self.package_dirs
            .iter()
            .map(|d| NamedVolume::for_package_dir(&self.hash, d))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), b"").expect("write fixture");
    }

    #[test]
    fn container_name_starts_with_sandbox() {
        let h = ProjectHash::from_bytes([0xab; 32]);
        let cn = ContainerName::from_hash(&h);
        assert!(cn.as_str().starts_with("sandbox-"));
        assert_eq!(cn.as_str().len(), "sandbox-".len() + 12);
    }

    #[test]
    fn named_volume_sanitizes_special_chars() {
        let h = ProjectHash::from_bytes([0xab; 32]);
        let v = NamedVolume::for_package_dir(&h, ".cargo/registry");
        assert!(!v.as_str().contains('.'));
        assert!(!v.as_str().contains('/'));
        assert!(v.as_str().starts_with("sandbox-"));
    }

    #[test]
    fn resolve_picks_language_via_detect() {
        let tmp = tempfile::tempdir().expect("tempdir");
        touch(tmp.path(), "Cargo.toml");
        let reg = LanguageRegistry::builtin().expect("builtin");
        let p = Project::resolve(tmp.path(), &reg, None).expect("resolve");
        assert_eq!(p.language.as_str(), "rust");
        assert!(p.package_dirs.iter().any(|d| d == "target"));
        assert_eq!(p.named_volumes().len(), p.package_dirs.len());
    }

    #[test]
    fn resolve_honors_lang_override() {
        let tmp = tempfile::tempdir().expect("tempdir");
        touch(tmp.path(), "Cargo.toml");
        let reg = LanguageRegistry::builtin().expect("builtin");
        let p = Project::resolve(tmp.path(), &reg, Some("node")).expect("resolve");
        assert_eq!(p.language.as_str(), "node");
    }

    #[test]
    fn resolve_rejects_nonexistent_path() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let nope = tmp.path().join("does-not-exist");
        let reg = LanguageRegistry::builtin().expect("builtin");
        let err = Project::resolve(&nope, &reg, None).expect_err("should fail");
        assert!(matches!(err, Error::Io { .. }));
    }

    #[test]
    fn resolve_rejects_unknown_lang_override() {
        let tmp = tempfile::tempdir().expect("tempdir");
        touch(tmp.path(), "Cargo.toml");
        let reg = LanguageRegistry::builtin().expect("builtin");
        let err = Project::resolve(tmp.path(), &reg, Some("clojure")).expect_err("should fail");
        assert!(matches!(err, Error::LanguageNotFound(_)));
    }

    #[test]
    fn container_name_stable_across_resolves() {
        let tmp = tempfile::tempdir().expect("tempdir");
        touch(tmp.path(), "Cargo.toml");
        let reg = LanguageRegistry::builtin().expect("builtin");
        let p1 = Project::resolve(tmp.path(), &reg, None).expect("resolve");
        let p2 = Project::resolve(tmp.path(), &reg, None).expect("resolve");
        assert_eq!(p1.container_name, p2.container_name);
    }
}
