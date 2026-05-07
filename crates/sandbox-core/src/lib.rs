//! Foundational domain types for the sandbox tool.
//!
//! See `crates/sandbox-core/AGENTS.md` for boundaries and conventions.

pub mod config;
pub mod error;
pub mod hash;
pub mod lang;
pub mod paths;
pub mod profile;
pub mod project;
pub mod state;

pub use config::{Config, Defaults, ProxyConfig, ScanConfig};
pub use error::{Error, Result};
pub use hash::{ProjectHash, project_hash};
pub use lang::{LangManifest, LanguageId, LanguageRegistry, PortDetection};
pub use paths::Paths;
pub use profile::Profile;
pub use project::{ContainerName, NamedVolume, Project};
pub use state::Meta;
