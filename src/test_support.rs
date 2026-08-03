//! Helpers shared by the crate's unit tests.

#![allow(clippy::unwrap_used)]
// Panicking on a failed setup step is the correct failure mode for a test; the
// crate-level `deny(clippy::unwrap_used)` is aimed at production code.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

/// Build a fake NixOS system closure under `root` whose `kernel` and `systemd`
/// symlinks resolve the way the real ones do: `kernel` points at a file *inside*
/// the kernel package, `systemd` at the systemd package directory.
///
/// `label` becomes the contents of `nixos-version`, mirroring
/// `config.system.nixos.label`.
pub fn fake_system(root: &Path, name: &str, label: &str, kernel: &str, systemd: &str) -> PathBuf {
    let store = root.join("nix/store");
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
