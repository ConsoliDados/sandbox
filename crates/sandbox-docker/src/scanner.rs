//! Adapter for the ephemeral ClamAV scan container (Phase 4b).
//!
//! Per ADR-0008 we shell out to `docker` to run two operations:
//!
//! - `run_clamscan(project_path, db_volume)` — read-only bind of the project
//!   into `/scan`, mount the persistent signature volume, no network.
//! - `run_freshclam(db_volume)` — `--network bridge` (the only outbound moment
//!   in the scanner's life), refreshes signatures into the same volume.
//!
//! [`ensure_image`] builds `sandbox/scanner:latest` from the bundled
//! `crates/sandbox-scan/scanner-image/Dockerfile` if the daemon doesn't
//! already have it. For v0.1 we don't pull from a registry — local build
//! keeps the trust boundary at the user.

use std::path::Path;

use crate::cmd::{run_attached, run_probe};
use crate::{Error, Result};

pub const SCANNER_IMAGE: &str = "sandbox/scanner:latest";
pub const SCANNER_DB_VOLUME: &str = "sandbox-scanner-db";

/// Outcome of `clamscan`. `infected_count` is parsed from the exit code:
///   - 0  → clean
///   - 1  → at least one virus found (caller parses stdout for details)
///   - >1 → real error (network, signature DB missing, etc.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClamavOutcome {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl ClamavOutcome {
    pub fn is_clean(&self) -> bool {
        self.exit_code == 0
    }

    pub fn has_findings(&self) -> bool {
        self.exit_code == 1
    }

    pub fn is_error(&self) -> bool {
        self.exit_code > 1
    }
}

/// `True` if `sandbox/scanner:latest` already exists in the local daemon.
pub async fn image_exists() -> Result<bool> {
    let probe = run_probe(&["image", "inspect", SCANNER_IMAGE]).await?;
    Ok(probe.success)
}

/// Build the scanner image from the bundled Dockerfile. `dockerfile_dir`
/// must be a path the daemon can read (caller resolves it). Attaches stdio
/// so the user sees the build progress; this happens at most once per
/// machine until the Dockerfile or alpine base changes.
pub async fn build_image(dockerfile_dir: &Path) -> Result<()> {
    let dir_str = dockerfile_dir
        .to_str()
        .ok_or_else(|| Error::NonUtf8Output {
            cmd: "docker build (dockerfile path)".into(),
        })?;
    run_attached(&["build", "-t", SCANNER_IMAGE, dir_str]).await
}

/// Build the image only if it isn't already present.
pub async fn ensure_image(dockerfile_dir: &Path) -> Result<()> {
    if image_exists().await? {
        return Ok(());
    }
    build_image(dockerfile_dir).await
}

/// `True` if the `sandbox-scanner-db` named volume exists. Used by the CLI
/// to decide whether the signature DB has been populated at least once
/// (a `clamscan` without DB exits with code 2).
pub async fn db_volume_exists() -> Result<bool> {
    let probe = run_probe(&["volume", "inspect", SCANNER_DB_VOLUME]).await?;
    Ok(probe.success)
}

/// Run `clamscan --recursive --no-summary --infected /scan` against the
/// project bind. Returns the raw stdout/stderr so the caller can parse the
/// "FOUND" lines (see `sandbox-scan::clamav`). No findings ⇒ exit 0; any
/// finding ⇒ exit 1; daemon/DB errors ⇒ exit ≥ 2.
pub async fn run_clamscan(project_path: &Path) -> Result<ClamavOutcome> {
    let project_str = project_path.to_str().ok_or_else(|| Error::NonUtf8Output {
        cmd: "docker run (project path)".into(),
    })?;
    // We intentionally allow non-zero exit and capture it: `clamscan` uses
    // exit code 1 to mean "virus found", which is data, not failure.
    let output = tokio::process::Command::new("docker")
        .args([
            "run",
            "--rm",
            "--network",
            "none",
            "--read-only",
            "--tmpfs",
            "/tmp",
            "-v",
            &format!("{project_str}:/scan:ro"),
            "-v",
            &format!("{SCANNER_DB_VOLUME}:/var/lib/clamav"),
            SCANNER_IMAGE,
            "--recursive",
            "--no-summary",
            "--infected",
            "/scan",
        ])
        .output()
        .await
        .map_err(|source| Error::Io { source })?;

    Ok(ClamavOutcome {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

/// Run `freshclam` to refresh the signature DB. `--network bridge` is the
/// only outbound network moment in the scanner's lifetime; we attach stdio
/// so the user sees the download progress.
pub async fn run_freshclam() -> Result<()> {
    let argv = [
        "run",
        "--rm",
        "--network",
        "bridge",
        "-v",
        &format!("{SCANNER_DB_VOLUME}:/var/lib/clamav"),
        "--entrypoint",
        "freshclam",
        SCANNER_IMAGE,
    ];
    let argv: Vec<&str> = argv.iter().map(|s| *s as &str).collect();
    run_attached(&argv).await
}

/// Print-cmd helpers: render the docker invocations as strings so
/// `--print-cmd` flows can echo what would run without executing.
pub fn clamscan_argv(project_path: &Path) -> Vec<String> {
    vec![
        "run".into(),
        "--rm".into(),
        "--network".into(),
        "none".into(),
        "--read-only".into(),
        "--tmpfs".into(),
        "/tmp".into(),
        "-v".into(),
        format!("{}:/scan:ro", project_path.display()),
        "-v".into(),
        format!("{SCANNER_DB_VOLUME}:/var/lib/clamav"),
        SCANNER_IMAGE.into(),
        "--recursive".into(),
        "--no-summary".into(),
        "--infected".into(),
        "/scan".into(),
    ]
}

pub fn freshclam_argv() -> Vec<String> {
    vec![
        "run".into(),
        "--rm".into(),
        "--network".into(),
        "bridge".into(),
        "-v".into(),
        format!("{SCANNER_DB_VOLUME}:/var/lib/clamav"),
        "--entrypoint".into(),
        "freshclam".into(),
        SCANNER_IMAGE.into(),
    ]
}

// Suppress dead-code warnings: until the CLI wires these in (next commit)
// they're not referenced. Once `commands/scan.rs` calls into them this
// allow is removed.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamscan_argv_carries_ro_bind_and_no_network() {
        let argv = clamscan_argv(Path::new("/tmp/proj"));
        assert!(argv.contains(&"--read-only".to_string()));
        assert!(argv.windows(2).any(|w| w == ["--network", "none"]));
        assert!(argv.iter().any(|a| a.ends_with(":/scan:ro")));
        assert!(argv.iter().any(|a| a == SCANNER_IMAGE));
        // Scanner flags must come AFTER the image arg (positional).
        let img_idx = argv.iter().position(|a| a == SCANNER_IMAGE);
        let scan_idx = argv.iter().position(|a| a == "/scan");
        assert!(matches!((img_idx, scan_idx), (Some(i), Some(s)) if i < s));
    }

    #[test]
    fn freshclam_argv_overrides_entrypoint_and_allows_network() {
        let argv = freshclam_argv();
        assert!(argv.windows(2).any(|w| w == ["--network", "bridge"]));
        assert!(argv.windows(2).any(|w| w == ["--entrypoint", "freshclam"]));
        assert_eq!(argv.last().map(String::as_str), Some(SCANNER_IMAGE));
    }

    #[test]
    fn clamav_outcome_classifies_exit_codes() {
        let clean = ClamavOutcome {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
        };
        let found = ClamavOutcome {
            exit_code: 1,
            ..clean.clone()
        };
        let err = ClamavOutcome {
            exit_code: 2,
            ..clean.clone()
        };
        assert!(clean.is_clean());
        assert!(found.has_findings());
        assert!(err.is_error());
    }
}
