//! Security scanner: YARA rules + heuristic patterns + compose validation.
//!
//! See `crates/sandbox-scan/AGENTS.md` for boundaries and conventions.

pub mod cache;
pub mod clamav;
pub mod compose;
pub mod engine;
pub mod error;
pub mod findings;
pub mod heuristics;
pub mod project_hash;
pub mod suppress;
pub mod yara;

pub use cache::RULESET_VERSION;
pub use engine::{ScanOpts, ScanReport, scan};
pub use error::{Error, Result};
pub use findings::{Finding, Findings, Severity};
pub use project_hash::content_hash;
pub use suppress::IgnoreList;
pub use yara::YaraEngine;
