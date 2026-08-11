//! Orchestration: turn a parsed invocation into a decision and its side
//! effects.
//!
//! Nothing in here calls [`std::process::exit`]. The exit status is a value
//! ([`Outcome`]) returned up to `main`, which is what makes the tool's actual
//! contract — its exit codes — testable.

use crate::cli::{Mode, Options, Recompute};
use crate::error::Error;
use crate::paths::SystemPaths;
use crate::reboot::{decide, RebootDecision};
use std::{env, fs, path::Path, path::PathBuf};

/// Whether the process may write to the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Privilege {
    /// Running as `root`.
    Root,
    /// Running as some other user.
    Unprivileged,
}

impl Privilege {
    /// Determine the privilege level from `$USER`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnknownUser`] if `$USER` is unset or not valid UTF-8.
    pub fn from_env() -> Result<Self, Error> {
        let user = env::var_os("USER").ok_or(Error::UnknownUser)?;
        let user = user.into_string().map_err(|_| Error::UnknownUser)?;

        Ok(if user == "root" {
            Self::Root
        } else {
            Self::Unprivileged
        })
    }
}

/// The result of a completed check, and the process exit status it maps to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The machine is running everything the activated generation provides.
    NoRebootRequired,
    /// The activated generation ships a newer kernel or systemd.
    RebootRequired,
}

impl Outcome {
    /// The process exit status for this outcome.
    ///
    /// `0` means no reboot is required and `2` means one is. A failed check is
    /// `1` and is never produced here, because a failure is an [`Error`], not
    /// an `Outcome`.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::NoRebootRequired => 0,
            Self::RebootRequired => 2,
        }
    }
}

/// Perform a reboot check.
///
/// # Errors
///
/// Returns an error if the flag file would have to be written without root, if
/// this is not a NixOS system, if either system closure cannot be resolved or
/// inspected, or if the flag file cannot be read, removed or written.
pub fn run(options: &Options, paths: &SystemPaths, privilege: Privilege) -> Result<Outcome, Error> {
    if options.mode == Mode::Commit && privilege == Privilege::Unprivileged {
        return Err(Error::NotRoot);
    }

    if !paths.activated.exists() {
        return Err(Error::NotNixos {
            path: paths.activated.clone(),
        });
    }

    let booted = resolve_system(&paths.booted)?;
    let activated = resolve_system(&paths.activated)?;

    if let Some(outcome) = report_existing_flag_file(options.recompute, &paths.flag_file)? {
        return Ok(outcome);
    }

    remove_stale_flag_file(options.mode, &paths.flag_file)?;

    match decide(&booted, &activated)? {
        RebootDecision::SameGeneration => {
            info!("You are using the latest NixOS generation, no need to reboot");
            Ok(Outcome::NoRebootRequired)
        }
        RebootDecision::NoRebootWorthyChange => {
            info!("No updates available, moar uptime!!!");
            Ok(Outcome::NoRebootRequired)
        }
        RebootDecision::Needed(reasons) => {
            match options.mode {
                Mode::DryRun => print_reasons(&reasons),
                Mode::Commit => write_flag_file(&paths.flag_file, &reasons)?,
            }
            Ok(Outcome::RebootRequired)
        }
    }
}

/// Resolve a system profile symlink to the concrete `/nix/store` closure it
/// points at.
fn resolve_system(path: &Path) -> Result<PathBuf, Error> {
    let resolved = fs::canonicalize(path).map_err(|source| Error::UnresolvableSystem {
        path: path.to_path_buf(),
        source,
    })?;

    debug!("{} resolves to {}", path.display(), resolved.display());

    Ok(resolved)
}

/// With `--no-force-recompute`, an existing flag file is authoritative: report
/// its contents and skip the comparison entirely.
fn report_existing_flag_file(
    recompute: Recompute,
    flag_file: &Path,
) -> Result<Option<Outcome>, Error> {
    if recompute == Recompute::Always || !flag_file.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(flag_file).map_err(|source| Error::FlagFileRead {
        path: flag_file.to_path_buf(),
        source,
    })?;

    info!("Reboot needed: {}", contents.trim());

    Ok(Some(Outcome::RebootRequired))
}

/// A recomputed check must not leave a flag file from a previous generation
/// behind: it would outlive the reason it was written for.
fn remove_stale_flag_file(mode: Mode, flag_file: &Path) -> Result<(), Error> {
    if mode == Mode::DryRun || !flag_file.exists() {
        return Ok(());
    }

    fs::remove_file(flag_file).map_err(|source| Error::FlagFileRemove {
        path: flag_file.to_path_buf(),
        source,
    })
}

fn print_reasons(reasons: &[String]) {
    for reason in reasons {
        info!("Upgrade available: {}", reason.trim());
    }
}

fn write_flag_file(flag_file: &Path, reasons: &[String]) -> Result<(), Error> {
    fs::write(flag_file, reasons.concat()).map_err(|source| Error::FlagFileWrite {
        path: flag_file.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::Outcome;

    /// Consumers script on these numbers; they are the tool's public contract.
    #[test]
    fn the_exit_codes_are_zero_and_two() {
        assert_eq!(Outcome::NoRebootRequired.code(), 0);
        assert_eq!(Outcome::RebootRequired.code(), 2);
    }
}
