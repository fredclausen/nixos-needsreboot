//! A throwaway NixOS-shaped directory tree for the integration tests.

// Panicking on a failed setup step is the correct failure mode for a test, and
// not every test file uses every helper.
#![allow(clippy::unwrap_used, dead_code)]

use nixos_needsreboot::SystemPaths;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// A temporary directory laid out like the parts of NixOS this tool reads.
pub struct FakeNixos {
    tmp: TempDir,
}

impl FakeNixos {
    pub fn new() -> Self {
        Self {
            tmp: TempDir::new().unwrap(),
        }
    }

    pub fn root(&self) -> &Path {
        self.tmp.path()
    }

    /// Build a system closure whose `kernel` and `systemd` symlinks resolve the
    /// way the real ones do: `kernel` points at a file *inside* the kernel
    /// package, `systemd` at the systemd package directory.
    ///
    /// The two symlinks are given absolute `/nix/store/...` targets that do not
    /// exist on disk. That is deliberate and load-bearing: the production code
    /// reads the link text with `read_link` and never follows it, so a refactor
    /// to `canonicalize` would break here rather than in production.
    ///
    /// `label` becomes the contents of `nixos-version`, mirroring
    /// `config.system.nixos.label`.
    pub fn system(&self, name: &str, label: &str, kernel: &str, systemd: &str) -> PathBuf {
        let store = self.root().join("nix/store");
        fs::create_dir_all(&store).unwrap();

        let kernel_pkg = store.join(format!("{name}hash-linux-{kernel}"));
        fs::create_dir_all(&kernel_pkg).unwrap();
        fs::write(kernel_pkg.join("bzImage"), "kernel").unwrap();

        let systemd_pkg = store.join(format!("{name}hash-systemd-{systemd}"));
        fs::create_dir_all(&systemd_pkg).unwrap();

        let system = store.join(format!("{name}hash-nixos-system-testhost-{label}"));
        fs::create_dir_all(&system).unwrap();
        fs::write(system.join("nixos-version"), label).unwrap();

        symlink(
            format!("/nix/store/{name}hash-linux-{kernel}/bzImage"),
            system.join("kernel"),
        )
        .unwrap();
        symlink(
            format!("/nix/store/{name}hash-systemd-{systemd}"),
            system.join("systemd"),
        )
        .unwrap();

        system
    }

    /// Point the booted and activated profile symlinks at `booted` and
    /// `activated`, and return the [`SystemPaths`] describing this tree.
    ///
    /// The flag file is *not* created; its path merely points into the tree.
    pub fn paths(&self, booted: &Path, activated: &Path) -> SystemPaths {
        let profiles = self.root().join("profiles");
        fs::create_dir_all(&profiles).unwrap();

        let booted_link = profiles.join("booted-system");
        let activated_link = profiles.join("system");
        symlink(booted, &booted_link).unwrap();
        symlink(activated, &activated_link).unwrap();

        SystemPaths {
            booted: booted_link,
            activated: activated_link,
            flag_file: self.root().join("reboot-required"),
        }
    }

    /// The common case: a booted closure and an activated closure differing
    /// only in the versions given.
    pub fn generations(&self, booted: (&str, &str), activated: (&str, &str)) -> SystemPaths {
        let label = "26.05pre-git";
        let old = self.system("old", label, booted.0, booted.1);
        let new = self.system("new", label, activated.0, activated.1);
        self.paths(&old, &new)
    }

    /// A machine already running its activated generation.
    pub fn same_generation(&self, kernel: &str, systemd: &str) -> SystemPaths {
        let system = self.system("only", "26.05pre-git", kernel, systemd);
        self.paths(&system, &system)
    }
}

impl Default for FakeNixos {
    fn default() -> Self {
        Self::new()
    }
}
