//! Tiny helper: compile a regex literal whose only failure mode is "the
//! developer typoed the pattern at build time."
//!
//! Lifting `Regex::new(...).expect(...)` into a dedicated function lets us
//! attach a single `#[allow(clippy::expect_used)]` here instead of one per
//! static. Returning the compiled regex (vs. Result) keeps the call sites
//! readable, and the test suite catches any pattern typo because every
//! caller is exercised by at least one unit test.

use regex::Regex;

#[allow(clippy::expect_used)]
pub(super) fn compile(pattern: &'static str) -> Regex {
    Regex::new(pattern).expect("static regex compiles at build time")
}
