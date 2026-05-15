//! Minimal `docker-compose.yml` parser.
//!
//! We deliberately model only the fields we audit. Unknown keys are ignored
//! (compose has many; spec changes; we don't want to fight schema drift). A
//! missing `services:` map yields a `ComposeFile` with `services` empty,
//! which the validator then sees as "nothing to flag."

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::{Error, Result};

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ComposeFile {
    #[serde(default)]
    pub services: HashMap<String, Service>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Service {
    #[serde(default)]
    pub privileged: Option<bool>,
    #[serde(default)]
    pub network_mode: Option<String>,
    #[serde(default)]
    pub pid: Option<String>,
    #[serde(default)]
    pub userns_mode: Option<String>,
    #[serde(default)]
    pub cap_add: Option<Vec<String>>,
    #[serde(default)]
    pub security_opt: Option<Vec<String>>,
    #[serde(default)]
    pub volumes: Option<Vec<VolumeRef>>,
}

/// Compose accepts volumes in two shapes:
///   - short: `"./data:/data:ro"`
///   - long:  `{ type: bind, source: ./data, target: /data, read_only: true }`
///
/// We parse both into a normalized struct downstream.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum VolumeRef {
    Short(String),
    Long(LongVolume),
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LongVolume {
    #[serde(rename = "type", default)]
    pub volume_type: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub read_only: Option<bool>,
}

/// Parse a compose YAML body. Path is included in the error for diagnostics.
pub fn parse(path: &Path, body: &str) -> Result<ComposeFile> {
    serde_yml::from_str(body).map_err(|e| Error::ComposeParse {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn parse_empty_yaml_yields_default() -> TestResult {
        let f = parse(Path::new("compose.yml"), "version: '3'\n")?;
        assert!(f.services.is_empty());
        Ok(())
    }

    #[test]
    fn parse_service_with_privileged_and_caps() -> TestResult {
        let body = r#"
services:
  app:
    image: nginx
    privileged: true
    cap_add:
      - SYS_ADMIN
    network_mode: host
"#;
        let f = parse(Path::new("compose.yml"), body)?;
        let summary = f.services.get("app").map(|svc| {
            (
                svc.privileged,
                svc.network_mode.clone(),
                svc.cap_add.clone().unwrap_or_default(),
            )
        });
        assert_eq!(
            summary,
            Some((Some(true), Some("host".into()), vec!["SYS_ADMIN".into()]))
        );
        Ok(())
    }

    #[test]
    fn parse_short_and_long_volumes() -> TestResult {
        let body = r#"
services:
  app:
    image: x
    volumes:
      - "/var/lib/docker:/host-docker:ro"
      - type: bind
        source: /etc
        target: /host-etc
        read_only: true
"#;
        let f = parse(Path::new("compose.yml"), body)?;
        let vols: Vec<_> = f
            .services
            .get("app")
            .and_then(|s| s.volumes.as_ref())
            .map(|v| {
                v.iter()
                    .map(|vol| match vol {
                        VolumeRef::Short(s) => ("short", s.clone(), None, None, None),
                        VolumeRef::Long(l) => (
                            "long",
                            String::new(),
                            l.source.clone(),
                            l.target.clone(),
                            l.read_only,
                        ),
                    })
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(
            vols,
            vec![
                (
                    "short",
                    "/var/lib/docker:/host-docker:ro".into(),
                    None,
                    None,
                    None,
                ),
                (
                    "long",
                    String::new(),
                    Some("/etc".into()),
                    Some("/host-etc".into()),
                    Some(true),
                ),
            ]
        );
        Ok(())
    }

    #[test]
    fn parse_invalid_yaml_errors_with_path() {
        let result = parse(Path::new("compose.yml"), "::: invalid :::");
        let parsed_path = match result {
            Err(Error::ComposeParse { path, .. }) => Some(path.to_string_lossy().into_owned()),
            _ => None,
        };
        assert_eq!(parsed_path, Some("compose.yml".into()));
    }
}
