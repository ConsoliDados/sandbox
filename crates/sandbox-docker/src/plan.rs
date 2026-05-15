//! `Plan` — pure data describing a single `docker run` invocation.
//!
//! Constructed by the CLI orchestrator from `core::Project + Profile + LangManifest`
//! and rendered to argv via [`Plan::to_args`]. `Display` prints the equivalent
//! shell command for `--print-cmd` (per ADR-0002 every Docker action must be
//! representable as a literal command line).

use std::fmt;
use std::path::PathBuf;
use std::process::Command;

use sandbox_core::ContainerName;

use crate::{Error, Result};

/// Numeric uid:gid for `--user`. Per ADR-0009 / OQ-004 we always pass numeric
/// pairs to avoid mismatches with images that lack the host's username.
#[derive(Debug, Clone, Copy)]
pub struct UserSpec {
    pub uid: u32,
    pub gid: u32,
}

impl UserSpec {
    /// Read uid/gid by shelling out to `id -u` / `id -g`. The host process is
    /// the source of truth — we don't trust `$UID` (not exported by every
    /// shell) and avoid `unsafe` libc calls (forbidden at workspace level).
    pub fn current() -> Result<Self> {
        Ok(Self {
            uid: read_id('u')?,
            gid: read_id('g')?,
        })
    }
}

fn read_id(flag: char) -> Result<u32> {
    let arg = format!("-{flag}");
    let out = Command::new("id")
        .arg(&arg)
        .output()
        .map_err(|e| Error::UserIdLookup {
            flag,
            reason: e.to_string(),
        })?;
    if !out.status.success() {
        return Err(Error::UserIdLookup {
            flag,
            reason: format!("id exited with {:?}", out.status.code()),
        });
    }
    let s = String::from_utf8_lossy(&out.stdout);
    s.trim()
        .parse()
        .map_err(|e: std::num::ParseIntError| Error::UserIdLookup {
            flag,
            reason: e.to_string(),
        })
}

/// A single mount declaration. `--volume` for binds and named volumes; `--tmpfs` otherwise.
#[derive(Debug, Clone)]
pub enum Mount {
    Bind {
        src: PathBuf,
        dst: String,
        read_only: bool,
    },
    Volume {
        name: String,
        dst: String,
        read_only: bool,
    },
    Tmpfs {
        dst: String,
    },
}

#[derive(Debug, Clone)]
pub enum NetworkSpec {
    /// Internal docker network — no internet egress.
    Internal(String),
    /// Standard bridge — egress to the host's network.
    Bridge,
    /// `--network none` — no networking at all.
    None,
}

#[derive(Debug, Clone)]
pub struct SecuritySpec {
    pub cap_drop_all: bool,
    pub no_new_privileges: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ResourceSpec {
    pub cpus: Option<f32>,
    pub memory_mb: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub image: String,
    pub container_name: ContainerName,
    pub user: UserSpec,
    pub workdir: String,
    pub mounts: Vec<Mount>,
    pub env: Vec<(String, String)>,
    pub network: NetworkSpec,
    /// Extra Docker networks to attach the container to after creation.
    /// `docker run --network` accepts only one network, so additional ones
    /// are joined via `docker network connect` between `create` and `start`
    /// — see `lifecycle::run`. Used by the reverse proxy to attach project
    /// containers to `sandbox-proxy` while keeping `sandbox-internal` as
    /// the primary egress-restricted network.
    pub additional_networks: Vec<String>,
    pub security: SecuritySpec,
    pub resources: ResourceSpec,
    /// Docker labels rendered as `--label k=v`. Used by Traefik to register
    /// the container for routing; empty in non-proxy flows.
    pub labels: Vec<(String, String)>,
    /// Override the image's ENTRYPOINT. Set to the shell binary so that
    /// images shipping a non-shell entrypoint (e.g. `node:24` runs `node`
    /// by default) still drop the user into an interactive session.
    pub entrypoint: Option<String>,
    pub command: Vec<String>,
    pub interactive: bool,
    pub tty: bool,
    pub remove_on_exit: bool,
    pub detach: bool,
}

impl Plan {
    /// Render the plan as `docker run …` argv (without the leading `docker`).
    pub fn to_args(&self) -> Vec<String> {
        let mut a: Vec<String> = vec!["run".into()];
        if self.detach {
            a.push("--detach".into());
        }
        if self.interactive {
            a.push("--interactive".into());
        }
        if self.tty {
            a.push("--tty".into());
        }
        if self.remove_on_exit {
            a.push("--rm".into());
        }
        a.push("--name".into());
        a.push(self.container_name.as_str().into());
        a.push("--user".into());
        a.push(format!("{}:{}", self.user.uid, self.user.gid));
        a.push("--workdir".into());
        a.push(self.workdir.clone());

        for m in &self.mounts {
            match m {
                Mount::Bind {
                    src,
                    dst,
                    read_only,
                } => {
                    a.push("--volume".into());
                    a.push(format!(
                        "{}:{}{}",
                        src.display(),
                        dst,
                        if *read_only { ":ro" } else { "" }
                    ));
                }
                Mount::Volume {
                    name,
                    dst,
                    read_only,
                } => {
                    a.push("--volume".into());
                    a.push(format!(
                        "{}:{}{}",
                        name,
                        dst,
                        if *read_only { ":ro" } else { "" }
                    ));
                }
                Mount::Tmpfs { dst } => {
                    a.push("--tmpfs".into());
                    a.push(dst.clone());
                }
            }
        }

        for (k, v) in &self.env {
            a.push("--env".into());
            a.push(format!("{k}={v}"));
        }

        match &self.network {
            NetworkSpec::Internal(name) => {
                a.push("--network".into());
                a.push(name.clone());
            }
            NetworkSpec::Bridge => {
                a.push("--network".into());
                a.push("bridge".into());
            }
            NetworkSpec::None => {
                a.push("--network".into());
                a.push("none".into());
            }
        }

        for (k, v) in &self.labels {
            a.push("--label".into());
            a.push(format!("{k}={v}"));
        }

        if self.security.cap_drop_all {
            a.push("--cap-drop".into());
            a.push("ALL".into());
        }
        if self.security.no_new_privileges {
            a.push("--security-opt".into());
            a.push("no-new-privileges".into());
        }

        if let Some(cpus) = self.resources.cpus {
            a.push("--cpus".into());
            a.push(format!("{cpus}"));
        }
        if let Some(mem) = self.resources.memory_mb {
            a.push("--memory".into());
            a.push(format!("{mem}m"));
        }

        if let Some(ep) = &self.entrypoint {
            a.push("--entrypoint".into());
            a.push(ep.clone());
        }

        a.push(self.image.clone());
        a.extend(self.command.iter().cloned());
        a
    }
}

impl fmt::Display for Plan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("docker")?;
        for a in self.to_args() {
            if needs_quoting(&a) {
                let escaped = a.replace('\'', "'\\''");
                write!(f, " '{escaped}'")?;
            } else {
                write!(f, " {a}")?;
            }
        }
        Ok(())
    }
}

fn needs_quoting(s: &str) -> bool {
    s.is_empty()
        || s.chars()
            .any(|c| c.is_whitespace() || matches!(c, '"' | '\'' | '$' | '`' | '\\' | '*' | '?'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sandbox_core::ProjectHash;

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    fn fixture_plan() -> Plan {
        let hash = ProjectHash::from_bytes([0xab; 32]);
        Plan {
            image: "node:24.10.0".into(),
            container_name: ContainerName::from_hash(&hash),
            user: UserSpec {
                uid: 1000,
                gid: 1000,
            },
            workdir: "/app".into(),
            mounts: vec![
                Mount::Bind {
                    src: PathBuf::from("/home/me/proj"),
                    dst: "/app".into(),
                    read_only: true,
                },
                Mount::Volume {
                    name: "sandbox-abababababab-node_modules".into(),
                    dst: "/app/node_modules".into(),
                    read_only: false,
                },
                Mount::Tmpfs {
                    dst: "/home".into(),
                },
            ],
            env: vec![("HOME".into(), "/home".into())],
            network: NetworkSpec::Internal("sandbox-internal".into()),
            security: SecuritySpec {
                cap_drop_all: true,
                no_new_privileges: true,
            },
            additional_networks: vec![],
            resources: ResourceSpec {
                cpus: Some(2.0),
                memory_mb: Some(4096),
            },
            labels: vec![],
            entrypoint: Some("/bin/bash".into()),
            command: vec![],
            interactive: true,
            tty: true,
            remove_on_exit: false,
            detach: false,
        }
    }

    fn has_pair(args: &[String], k: &str, v: &str) -> bool {
        args.windows(2).any(|w| {
            if let [a, b] = w {
                a == k && b == v
            } else {
                false
            }
        })
    }

    fn after(args: &[String], flag: &str) -> Option<String> {
        let idx = args.iter().position(|a| a == flag)?;
        args.get(idx + 1).cloned()
    }

    #[test]
    fn args_open_with_run_and_name() {
        let args = fixture_plan().to_args();
        assert_eq!(args.first().map(String::as_str), Some("run"));
        let name = after(&args, "--name").unwrap_or_default();
        assert!(name.starts_with("sandbox-"));
    }

    #[test]
    fn args_carry_user_workdir_and_mounts() {
        let args = fixture_plan().to_args();
        assert!(has_pair(&args, "--user", "1000:1000"));
        assert!(has_pair(&args, "--workdir", "/app"));
        assert!(args.iter().any(|a| a == "/home/me/proj:/app:ro"));
        assert!(
            args.iter()
                .any(|a| a == "sandbox-abababababab-node_modules:/app/node_modules")
        );
        assert!(has_pair(&args, "--tmpfs", "/home"));
    }

    #[test]
    fn args_carry_security_and_resource_flags() {
        let args = fixture_plan().to_args();
        assert!(has_pair(&args, "--cap-drop", "ALL"));
        assert!(has_pair(&args, "--security-opt", "no-new-privileges"));
        assert!(has_pair(&args, "--cpus", "2"));
        assert!(has_pair(&args, "--memory", "4096m"));
    }

    #[test]
    fn args_end_with_image() {
        let args = fixture_plan().to_args();
        assert_eq!(args.last().map(String::as_str), Some("node:24.10.0"));
    }

    #[test]
    fn labels_render_as_repeated_label_flag() {
        let mut plan = fixture_plan();
        plan.labels.push(("traefik.enable".into(), "true".into()));
        plan.labels.push((
            "traefik.http.routers.sb-myproj-3000.rule".into(),
            "Host(`myproj.sandbox.local`)".into(),
        ));
        let args = plan.to_args();
        assert!(has_pair(&args, "--label", "traefik.enable=true"));
        assert!(has_pair(
            &args,
            "--label",
            "traefik.http.routers.sb-myproj-3000.rule=Host(`myproj.sandbox.local`)",
        ));
    }

    #[test]
    fn entrypoint_override_renders_before_image() {
        let args = fixture_plan().to_args();
        assert!(has_pair(&args, "--entrypoint", "/bin/bash"));
        // entrypoint must come before the image (positional arg)
        let entry_idx = args.iter().position(|a| a == "--entrypoint");
        let image_idx = args.iter().position(|a| a == "node:24.10.0");
        assert!(matches!((entry_idx, image_idx), (Some(e), Some(i)) if e < i));
    }

    #[test]
    fn display_starts_with_docker_run() {
        let s = format!("{}", fixture_plan());
        assert!(s.starts_with("docker run"));
        assert!(s.contains("--user 1000:1000"));
    }

    #[test]
    fn display_quotes_args_with_spaces() {
        let mut plan = fixture_plan();
        plan.env.push(("WITH_SPACE".into(), "hello world".into()));
        let s = format!("{plan}");
        assert!(s.contains("'WITH_SPACE=hello world'"));
    }

    #[test]
    fn network_bridge_renders_as_bridge() -> TestResult {
        let mut plan = fixture_plan();
        plan.network = NetworkSpec::Bridge;
        let args = plan.to_args();
        assert!(has_pair(&args, "--network", "bridge"));
        Ok(())
    }
}
