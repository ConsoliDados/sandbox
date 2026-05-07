use std::path::Path;

use sha2::{Digest, Sha256};

use crate::{Error, Result};

/// Stable identifier for a project's filesystem location.
///
/// Computed as `sha256(canonical_path_bytes)`. Workspace-stable, not
/// content-sensitive — see ADR-0009. The scanner uses a separate content hash
/// for cache invalidation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProjectHash([u8; 32]);

impl ProjectHash {
    pub fn from_bytes(b: [u8; 32]) -> Self {
        Self(b)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Full 64-char hex string.
    pub fn hex(&self) -> String {
        hex::encode(self.0)
    }

    /// First 12 hex chars; used in container names (`sandbox-<short>`).
    pub fn short(&self) -> String {
        // Indexing is bounded by the type: ProjectHash is always 32 bytes.
        hex::encode(&self.0[..6])
    }
}

impl std::fmt::Display for ProjectHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.short())
    }
}

/// Compute the project hash from a directory path.
///
/// The path must exist and be a directory; otherwise [`Error::ProjectPathInvalid`].
/// Symbolic links are resolved before hashing so two paths that point at the
/// same target produce the same hash.
pub fn project_hash(path: &Path) -> Result<ProjectHash> {
    if !path.is_dir() {
        return Err(Error::ProjectPathInvalid(path.to_path_buf()));
    }
    let canonical = std::fs::canonicalize(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;

    let mut hasher = Sha256::new();
    hasher.update(canonical.as_os_str().as_encoded_bytes());
    let bytes: [u8; 32] = hasher.finalize().into();
    Ok(ProjectHash::from_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn hash_is_deterministic() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let h1 = project_hash(tmp.path())?;
        let h2 = project_hash(tmp.path())?;
        assert_eq!(h1, h2);
        Ok(())
    }

    #[test]
    fn hash_differs_for_different_paths() -> TestResult {
        let a = tempfile::tempdir()?;
        let b = tempfile::tempdir()?;
        let ha = project_hash(a.path())?;
        let hb = project_hash(b.path())?;
        assert_ne!(ha, hb);
        Ok(())
    }

    #[test]
    fn hash_resolves_symlinks() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let target = tmp.path().join("target");
        std::fs::create_dir(&target)?;
        let link = tmp.path().join("link");
        std::os::unix::fs::symlink(&target, &link)?;

        let ht = project_hash(&target)?;
        let hl = project_hash(&link)?;
        assert_eq!(ht, hl, "symlink and target should hash equal");
        Ok(())
    }

    #[test]
    fn short_is_12_hex_chars() -> TestResult {
        let tmp = tempfile::tempdir()?;
        let h = project_hash(tmp.path())?;
        let short = h.short();
        assert_eq!(short.len(), 12);
        assert!(short.chars().all(|c| c.is_ascii_hexdigit()));
        Ok(())
    }

    #[test]
    fn rejects_non_directory() -> TestResult {
        let tmp = tempfile::NamedTempFile::new()?;
        let result = project_hash(tmp.path());
        assert!(matches!(result, Err(Error::ProjectPathInvalid(_))));
        Ok(())
    }
}
