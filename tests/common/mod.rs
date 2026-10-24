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

    /// The state `nixos-rebuild switch` leaves behind: `booted` was booted,
    /// `activated` is both running and in the profile, and both are
    /// registered generations (`1` and `2`).
    ///
    /// The flag file is *not* created; its path merely points into the tree.
    pub fn paths(&self, booted: &Path, activated: &Path) -> SystemPaths {
        self.layout(booted, activated, activated, &[booted, activated])
    }

    /// Lay out the profile symlinks explicitly.
    ///
    /// Each closure in `registered` becomes generation `index + 1`, as a
    /// `system-<N>-link` next to the profile, and the profile points at the
    /// last generation link whose closure is `profile` — exactly the chain of
    /// symlinks `nixos-rebuild` builds. A `profile` that is not registered is
    /// linked directly.
    pub fn layout(
        &self,
        booted: &Path,
        current: &Path,
        profile: &Path,
        registered: &[&Path],
    ) -> SystemPaths {
        let run = self.root().join("run");
        let profiles = self.root().join("profiles");
        fs::create_dir_all(&run).unwrap();
        fs::create_dir_all(&profiles).unwrap();

        let booted_link = run.join("booted-system");
        let current_link = run.join("current-system");
        let profile_link = profiles.join("system");
        symlink(booted, &booted_link).unwrap();
        symlink(current, &current_link).unwrap();

        let mut profile_target = profile.to_path_buf();
        for (index, closure) in registered.iter().enumerate() {
            let generation = profiles.join(format!("system-{}-link", index + 1));
            symlink(closure, &generation).unwrap();
            if *closure == profile {
                profile_target = generation;
            }
        }
        symlink(&profile_target, &profile_link).unwrap();

        SystemPaths {
            booted: booted_link,
            current: current_link,
            activated: profile_link,
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
