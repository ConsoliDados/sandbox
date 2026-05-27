use serde::{Deserialize, Serialize};

/// A named bundle of safety / runtime flags applied to a `sandbox run`.
///
/// Three profiles are built-in: `default`, `unsafe`, `paranoid`. Users can
/// override any of these or define new ones in `~/.config/sandbox/config.toml`.
/// CLI flags compose on top of the resolved profile (CLI wins on conflict).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    #[serde(default)]
    pub name: String,

    /// `--unsafe`: relax volume / network / scan defaults.
    /// (`unsafe` is a Rust keyword, hence the rename.)
    #[serde(default, rename = "unsafe")]
    pub unsafe_mode: bool,

    /// Allow internet egress at boot. Independently togglable at runtime.
    #[serde(default)]
    pub network: bool,

    /// Mount `$HOME` as a `tmpfs` so host secrets cannot leak into the container.
    #[serde(default = "default_true")]
    pub ephemeral_home: bool,

    /// Linux capabilities to drop. `"ALL"` is the strongest setting.
    #[serde(default = "default_cap_drop")]
    pub cap_drop: String,

    /// Set `--security-opt=no-new-privileges`.
    #[serde(default = "default_true")]
    pub no_new_privileges: bool,

    /// CPU limit (`--cpus=N`). `None` = no limit.
    #[serde(default)]
    pub cpu: Option<f32>,

    /// Memory limit in megabytes (`--memory=Nm`). `None` = no limit.
    #[serde(default)]
    pub memory_mb: Option<u32>,

    /// If true, do not auto-start the project's `docker-compose` deps.
    #[serde(default)]
    pub no_compose_deps: bool,
}

fn default_true() -> bool {
    true
}

fn default_cap_drop() -> String {
    "ALL".to_string()
}

impl Profile {
    /// The paranoid-by-default baseline. Used when no profile is selected.
    pub fn default_profile() -> Self {
        Self {
            name: "default".to_string(),
            unsafe_mode: false,
            network: false,
            ephemeral_home: true,
            cap_drop: "ALL".to_string(),
            no_new_privileges: true,
            cpu: Some(2.0),
            memory_mb: Some(4096),
            no_compose_deps: false,
        }
    }

    /// Trusted-project profile. Read/write source, full network, no scan block.
    pub fn unsafe_profile() -> Self {
        Self {
            name: "unsafe".to_string(),
            unsafe_mode: true,
            network: true,
            ephemeral_home: true,
            cap_drop: "ALL".to_string(),
            no_new_privileges: true,
            cpu: Some(2.0),
            memory_mb: Some(4096),
            no_compose_deps: false,
        }
    }

    /// Maximum lockdown: tighter resources, no compose deps auto-started.
    pub fn paranoid_profile() -> Self {
        Self {
            name: "paranoid".to_string(),
            unsafe_mode: false,
            network: false,
            ephemeral_home: true,
            cap_drop: "ALL".to_string(),
            no_new_privileges: true,
            cpu: Some(1.0),
            memory_mb: Some(2048),
            no_compose_deps: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_paranoid() {
        let p = Profile::default_profile();
        assert!(!p.unsafe_mode);
        assert!(!p.network);
        assert!(p.ephemeral_home);
        assert_eq!(p.cap_drop, "ALL");
        assert!(p.no_new_privileges);
    }

    #[test]
    fn unsafe_unlocks_volume_and_network() {
        let p = Profile::unsafe_profile();
        assert!(p.unsafe_mode);
        assert!(p.network);
    }

    #[test]
    fn paranoid_disables_compose_deps() {
        let p = Profile::paranoid_profile();
        assert!(p.no_compose_deps);
        assert_eq!(p.cpu, Some(1.0));
    }

    #[test]
    fn deserializes_with_unsafe_keyword() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let toml_in = r#"
name = "test"
unsafe = true
network = true
"#;
        let p: Profile = toml::from_str(toml_in)?;
        assert!(p.unsafe_mode);
        assert!(p.network);
        Ok(())
    }

    #[test]
    fn round_trip_keeps_unsafe_field_name() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let p = Profile::unsafe_profile();
        let s = toml::to_string(&p)?;
        assert!(s.contains("unsafe = true"));
        let back: Profile = toml::from_str(&s)?;
        assert_eq!(back.unsafe_mode, p.unsafe_mode);
        Ok(())
    }
}
