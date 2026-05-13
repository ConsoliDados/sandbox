//! Content hash of a project tree, used as the scan cache key.
//!
//! Preferred path: `git ls-files` — fast, respects `.gitignore`, ignores
//! untracked junk. Fallback: walkdir + sha256, used for non-git trees.
//! Output is a stable hex string regardless of which path is taken.

use std::path::Path;
use std::process::Command;

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::{Error, Result};

/// Compute the content hash of every file under `project_root`. Returns a
/// 64-char hex SHA-256.
pub fn content_hash(project_root: &Path) -> Result<String> {
    let files = match list_git_files(project_root) {
        Some(files) => files,
        None => list_walkdir_files(project_root)?,
    };

    let mut top = Sha256::new();
    for rel in &files {
        let abs = project_root.join(rel);
        // A path can be listed by git but not present on disk (deleted but
        // un-staged). Skip silently so the hash stays stable.
        let bytes = match std::fs::read(&abs) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => {
                return Err(Error::Io { path: abs, source });
            }
        };
        let mut entry = Sha256::new();
        entry.update(rel.as_bytes());
        entry.update([0u8]);
        entry.update(&bytes);
        top.update(entry.finalize());
    }
    Ok(hex::encode(top.finalize()))
}

/// `git ls-files -z` from `project_root`. `None` means git isn't available
/// or the directory isn't a repo — caller should fall back to walkdir.
fn list_git_files(project_root: &Path) -> Option<Vec<String>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(project_root)
        .args(["ls-files", "-z"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let mut files: Vec<String> = output
        .stdout
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    files.sort();
    Some(files)
}

fn list_walkdir_files(project_root: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(project_root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_ignored(e.file_name().to_string_lossy().as_ref()))
    {
        let entry = entry.map_err(|e| Error::Io {
            path: project_root.to_path_buf(),
            source: e.into(),
        })?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(project_root)
            .map_err(|_| Error::Io {
                path: entry.path().to_path_buf(),
                source: std::io::Error::other("path not under project root"),
            })?;
        files.push(rel.to_string_lossy().into_owned());
    }
    files.sort();
    Ok(files)
}

/// Hard-coded ignore list for the walkdir fallback. Mirrors what `git
/// ls-files` would skip in practice — package dirs and version control. We
/// keep this list short on purpose: scan accuracy matters more than scan
/// speed in the rare non-git case.
fn is_ignored(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | "node_modules"
            | "target"
            | ".pnpm-store"
            | ".yarn"
            | "dist"
            | "build"
            | ".venv"
            | "__pycache__"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn walkdir_fallback_stable_across_runs() -> TestResult {
        let tmp = tempfile::tempdir()?;
        std::fs::write(tmp.path().join("a.txt"), b"hello")?;
        std::fs::write(tmp.path().join("b.txt"), b"world")?;
        let h1 = content_hash(tmp.path())?;
        let h2 = content_hash(tmp.path())?;
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
        Ok(())
    }

    #[test]
    fn content_change_changes_hash() -> TestResult {
        let tmp = tempfile::tempdir()?;
        std::fs::write(tmp.path().join("a.txt"), b"hello")?;
        let h1 = content_hash(tmp.path())?;
        std::fs::write(tmp.path().join("a.txt"), b"hello!")?;
        let h2 = content_hash(tmp.path())?;
        assert_ne!(h1, h2);
        Ok(())
    }

    #[test]
    fn path_change_changes_hash() -> TestResult {
        // Same bytes, different filename → different hash. Catches the
        // "rename a malicious file" case.
        let tmp1 = tempfile::tempdir()?;
        let tmp2 = tempfile::tempdir()?;
        std::fs::write(tmp1.path().join("a.txt"), b"x")?;
        std::fs::write(tmp2.path().join("b.txt"), b"x")?;
        assert_ne!(content_hash(tmp1.path())?, content_hash(tmp2.path())?);
        Ok(())
    }

    #[test]
    fn ignored_dirs_are_skipped() -> TestResult {
        let tmp = tempfile::tempdir()?;
        std::fs::write(tmp.path().join("a.txt"), b"keep")?;
        let h_clean = content_hash(tmp.path())?;

        std::fs::create_dir_all(tmp.path().join("node_modules"))?;
        std::fs::write(tmp.path().join("node_modules").join("big.js"), b"junk")?;
        let h_with_nm = content_hash(tmp.path())?;
        // Hash must be identical: scan ignores node_modules.
        assert_eq!(h_clean, h_with_nm);
        Ok(())
    }
}
