//! Command implementations. One module per `sandbox` subcommand.
//!
//! Each module exposes an `execute` async function that the dispatcher in
//! `main.rs` calls with the parsed flags. Business logic stays here; argparse
//! lives in `main.rs`.

pub(crate) mod dotfiles;
pub(crate) mod down;
pub(crate) mod nuke;
pub(crate) mod run;
