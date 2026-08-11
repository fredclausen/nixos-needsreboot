//! The error taxonomy for a reboot check.

use crate::compare_nixos_modules::CompareError;
use std::{error, fmt, io, path::PathBuf};

/// Everything that can stop a reboot check from reaching a decision.
///
/// Every variant is a hard failure: the tool never degrades a failed check
/// into "no reboot needed", because that silently leaves a machine running an
/// old kernel.
#[derive(Debug)]
pub enum Error {
    /// The activated-system profile is absent, so this is not a NixOS system.
    NotNixos {
        /// The profile path that was expected to exist.
        path: PathBuf,
    },
    /// A system profile symlink could not be resolved to a store closure.
    UnresolvableSystem {
        /// The profile symlink that could not be resolved.
        path: PathBuf,
        /// The underlying filesystem error.
        source: io::Error,
    },
    /// `$USER` is unset or is not valid UTF-8.
    UnknownUser,
    /// The reboot flag file would have to be written, but the process is not
    /// running as root.
    NotRoot,
    /// The existing reboot flag file could not be read.
    FlagFileRead {
        /// The flag file that could not be read.
        path: PathBuf,
        /// The underlying filesystem error.
        source: io::Error,
    },
    /// The stale reboot flag file could not be removed.
    FlagFileRemove {
        /// The flag file that could not be removed.
        path: PathBuf,
        /// The underlying filesystem error.
        source: io::Error,
    },
    /// The reboot flag file could not be written.
    FlagFileWrite {
        /// The flag file that could not be written.
        path: PathBuf,
        /// The underlying filesystem error.
        source: io::Error,
    },
    /// A module could not be compared between the two closures.
    Compare(CompareError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotNixos { path } => write!(
                f,
                "This binary is intended to run only on NixOS; {} does not exist",
                path.display()
            ),
            Self::UnresolvableSystem { path, source } => {
                write!(f, "Cannot resolve {}: {source}", path.display())
            }
            Self::UnknownUser => write!(f, "Cannot determine current user from $USER"),
            Self::NotRoot => write!(
                f,
                "Please run this as root\nHINT: use the '--dry-run' option"
            ),
            Self::FlagFileRead { path, source } => {
                write!(f, "Could not read existing {}: {source}", path.display())
            }
            Self::FlagFileRemove { path, source } => {
                write!(f, "Could not remove existing {}: {source}", path.display())
            }
            Self::FlagFileWrite { path, source } => {
                write!(f, "Could not write {}: {source}", path.display())
            }
            Self::Compare(source) => write!(f, "Failed to compute upgrades:\n{source}"),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::UnresolvableSystem { source, .. }
            | Self::FlagFileRead { source, .. }
            | Self::FlagFileRemove { source, .. }
            | Self::FlagFileWrite { source, .. } => Some(source),
            Self::Compare(source) => Some(source),
            Self::NotNixos { .. } | Self::UnknownUser | Self::NotRoot => None,
        }
    }
}

impl From<CompareError> for Error {
    fn from(source: CompareError) -> Self {
        Self::Compare(source)
    }
}
