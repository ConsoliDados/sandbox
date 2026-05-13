//! Security scanner: YARA rules + heuristic patterns + compose validation.
//!
//! See `crates/sandbox-scan/AGENTS.md` for boundaries and conventions.

pub mod cache;
pub mod error;
pub mod findings;
pub mod project_hash;
pub mod yara;

pub use cache::RULESET_VERSION;
pub use error::{Error, Result};
pub use findings::{Finding, Findings, Severity};
pub use project_hash::content_hash;
pub use yara::YaraEngine;
