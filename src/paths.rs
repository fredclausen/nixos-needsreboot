//! The filesystem locations a reboot check reads and writes.
//!
//! Production code uses [`SystemPaths::nixos`]. Tests construct the struct
//! directly against a temporary directory, which is the whole reason these are
//! values rather than constants baked into the call sites.

use std::path::PathBuf;

/// The symlink pointing at the currently booted system closure.
pub const BOOTED_SYSTEM: &str = "/run/booted-system";
/// The symlink pointing at the most recently activated system closure.
pub const ACTIVATED_SYSTEM: &str = "/nix/var/nix/profiles/system";
/// The file written to record that a reboot is required, and why.
pub const REBOOT_FLAG_FILE: &str = "/run/reboot-required";

/// Where a reboot check looks for the two system closures and the flag file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemPaths {
    /// Symlink to the booted system closure.
    pub booted: PathBuf,
    /// Symlink to the activated system closure.
    pub activated: PathBuf,
    /// File recording that a reboot is required.
    pub flag_file: PathBuf,
}

impl SystemPaths {
    /// The real locations on a NixOS system.
    #[must_use]
    pub fn nixos() -> Self {
        Self {
            booted: PathBuf::from(BOOTED_SYSTEM),
            activated: PathBuf::from(ACTIVATED_SYSTEM),
            flag_file: PathBuf::from(REBOOT_FLAG_FILE),
        }
    }
}
