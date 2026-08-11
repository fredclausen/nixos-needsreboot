// Original code from https://github.com/thefossguy/nixos-needsreboot
// Forked and updated by Fred Clausen https://github.com/fredclausen/nixos-needsreboot/

//! Determine whether a NixOS machine needs a reboot after a system generation
//! change.
//!
//! The binary is a thin wrapper over [`run::run`], which returns its result as
//! a value rather than calling [`std::process::exit`]. That is deliberate: the
//! exit status is the tool's public contract, and a contract that is only
//! expressible by terminating the process cannot be tested.

#![deny(
    clippy::pedantic,
    //clippy::cargo,
    clippy::nursery,
    clippy::style,
    clippy::correctness,
    clippy::all,
    clippy::unwrap_used,
    clippy::expect_used
)]

#[macro_use]
extern crate log;

pub mod cli;
pub mod compare_nixos_modules;
pub mod error;
pub mod paths;
pub mod reboot;
pub mod run;

pub use cli::{Invocation, Mode, Options, Recompute, Verbosity};
pub use error::Error;
pub use paths::SystemPaths;
pub use reboot::{decide, RebootDecision};
pub use run::{run, Outcome, Privilege};
