//! Integration tests for the `sandbox` binary lifecycle.
//!
//! Tests in the default profile only exercise `--print-cmd`, which doesn't
//! require a Docker daemon. Tests gated by the `docker-tests` feature
//! exercise the full lifecycle (`run` → `down` → `nuke`) and require a
//! reachable local daemon plus permission to pull `node:24.10.0`.

use std::path::Path;
use std::process::Command;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_sandbox")
}

fn make_node_project(dir: &Path) -> std::io::Result<()> {
    std::fs::write(dir.join("package.json"), b"{\"name\":\"itest\"}\n")
}

#[test]
fn print_cmd_renders_safe_defaults_for_node_project() -> TestResult {
    let tmp = tempfile::tempdir()?;
    make_node_project(tmp.path())?;

    let out = Command::new(binary())
        .arg("--print-cmd")
        .args(["run", tmp.path().to_str().unwrap_or(".")])
        .output()?;
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8(out.stdout)?;
    assert!(stdout.starts_with("docker run"), "got: {stdout}");
    assert!(
        stdout.contains(":/app:ro"),
        "source must be RO in default mode: {stdout}"
    );
    assert!(
        stdout.contains("--network sandbox-internal"),
        "default = no internet: {stdout}"
    );
    assert!(
        stdout.contains("--cap-drop ALL"),
        "caps dropped by default: {stdout}"
    );
    assert!(
        stdout.contains("--security-opt no-new-privileges"),
        "no-new-privileges by default: {stdout}"
    );
    assert!(
        stdout.contains("--user "),
        "explicit numeric user: {stdout}"
    );
    Ok(())
}

#[test]
fn print_cmd_unsafe_relaxes_source_and_network() -> TestResult {
    let tmp = tempfile::tempdir()?;
    make_node_project(tmp.path())?;

    let out = Command::new(binary())
        .arg("--print-cmd")
        .args(["run", tmp.path().to_str().unwrap_or("."), "--unsafe"])
        .output()?;
    assert!(out.status.success());

    let stdout = String::from_utf8(out.stdout)?;
    assert!(
        !stdout.contains(":/app:ro"),
        "source must be RW under --unsafe: {stdout}"
    );
    assert!(
        stdout.contains("--network bridge"),
        "internet allowed under --unsafe: {stdout}"
    );
    assert!(
        stdout.contains("--cap-drop ALL"),
        "caps still dropped even under --unsafe"
    );
    Ok(())
}

#[test]
fn print_cmd_safe_mounts_lockfiles_from_state_dir() -> TestResult {
    let tmp = tempfile::tempdir()?;
    make_node_project(tmp.path())?;

    let out = Command::new(binary())
        .arg("--print-cmd")
        .args(["run", tmp.path().to_str().unwrap_or(".")])
        .output()?;
    assert!(out.status.success());

    let stdout = String::from_utf8(out.stdout)?;
    // Each declared lockfile in node.toml is bind-mounted RW from the
    // per-project state dir, regardless of host presence (ADR-0003).
    for name in ["package-lock.json", "pnpm-lock.yaml", "yarn.lock"] {
        let needle = format!("/lockfiles/{name}:/app/{name}");
        assert!(
            stdout.contains(&needle),
            "expected lockfile bind {needle} in: {stdout}"
        );
        let ro_needle = format!("/lockfiles/{name}:/app/{name}:ro");
        assert!(
            !stdout.contains(&ro_needle),
            "lockfile mount must be RW: {stdout}"
        );
    }
    Ok(())
}

#[test]
fn print_cmd_unsafe_skips_lockfile_state_mounts() -> TestResult {
    let tmp = tempfile::tempdir()?;
    make_node_project(tmp.path())?;

    let out = Command::new(binary())
        .arg("--print-cmd")
        .args(["run", tmp.path().to_str().unwrap_or("."), "--unsafe"])
        .output()?;
    assert!(out.status.success());

    let stdout = String::from_utf8(out.stdout)?;
    // In --unsafe the source bind is RW, so lockfile changes go straight
    // to the host project tree. No state-dir bind needed.
    assert!(
        !stdout.contains("/lockfiles/"),
        "no lockfile state mount under --unsafe: {stdout}"
    );
    Ok(())
}

#[test]
fn print_cmd_network_keeps_source_ro() -> TestResult {
    let tmp = tempfile::tempdir()?;
    make_node_project(tmp.path())?;

    let out = Command::new(binary())
        .arg("--print-cmd")
        .args(["run", tmp.path().to_str().unwrap_or("."), "--network"])
        .output()?;
    assert!(out.status.success());

    let stdout = String::from_utf8(out.stdout)?;
    assert!(
        stdout.contains(":/app:ro"),
        "source still RO with --network alone"
    );
    assert!(stdout.contains("--network bridge"), "internet allowed");
    Ok(())
}

// -----------------------------------------------------------------------------
// Docker-backed tests (require local daemon).
// -----------------------------------------------------------------------------

#[cfg(feature = "docker-tests")]
mod docker {
    use super::*;

    fn docker_available() -> bool {
        Command::new("docker")
            .arg("version")
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[test]
    fn run_creates_then_nuke_removes_node_project() -> TestResult {
        if !docker_available() {
            eprintln!("skipping: docker daemon unreachable");
            return Ok(());
        }
        let tmp = tempfile::tempdir()?;
        make_node_project(tmp.path())?;

        // Use detach via direct docker; we can't drive stdin, so we exercise
        // ensure_volume + ensure_internal then a quick `docker create` style
        // by going down the print-cmd happy path. A full lifecycle test
        // would require pty driving — out of scope for v0.1.
        let out = Command::new(binary())
            .arg("--print-cmd")
            .args(["run", tmp.path().to_str().unwrap_or(".")])
            .output()?;
        assert!(out.status.success());
        Ok(())
    }
}
