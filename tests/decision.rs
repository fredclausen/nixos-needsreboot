//! Which generation differences do and do not require a reboot.

#![allow(clippy::unwrap_used)]

mod common;

use common::FakeNixos;
use nixos_needsreboot::compare_nixos_modules::upgrades_available;
use nixos_needsreboot::{decide, RebootDecision};
use std::fs;

/// Regression test for the bug that made reboot detection a permanent no-op:
/// generation identity was read from `nixos-version`, which is just
/// `config.system.nixos.label`. On a flake that does not embed a revision in
/// the label every generation carries the same string, so the check always
/// concluded "latest generation" and never compared the kernel.
#[test]
fn kernel_upgrade_is_detected_when_nixos_version_labels_match() {
    let nixos = FakeNixos::new();
    let label = "26.05pre-git";

    let old = nixos.system("old", label, "6.18.33", "260.2");
    let new = nixos.system("new", label, "6.18.41", "260.2");

    // Precondition: the labels really are indistinguishable...
    assert_eq!(
        fs::read_to_string(old.join("nixos-version")).unwrap(),
        fs::read_to_string(new.join("nixos-version")).unwrap(),
    );

    // ...but the closures are not, so the kernel bump must be reported.
    assert_eq!(
        decide(&old, &new).unwrap(),
        RebootDecision::Needed(vec!["Linux Kernel (6.18.33 -> 6.18.41)\n".to_string()]),
    );
}

#[test]
fn booted_closure_equal_to_activated_closure_is_the_same_generation() {
    let nixos = FakeNixos::new();
    let system = nixos.system("only", "26.05pre-git", "6.18.41", "260.2");

    assert_eq!(
        decide(&system, &system).unwrap(),
        RebootDecision::SameGeneration
    );
}

#[test]
fn new_generation_without_kernel_or_systemd_change_needs_no_reboot() {
    let nixos = FakeNixos::new();

    let old = nixos.system("old", "26.05pre-git", "6.18.41", "260.2");
    let new = nixos.system("new", "26.05pre-git", "6.18.41", "260.2");

    assert_ne!(old, new);
    assert_eq!(
        decide(&old, &new).unwrap(),
        RebootDecision::NoRebootWorthyChange
    );
}

#[test]
fn unreadable_system_closure_is_an_error_not_a_silent_pass() {
    let nixos = FakeNixos::new();
    let old = nixos.system("old", "26.05pre-git", "6.18.41", "260.2");

    assert!(decide(&old, &nixos.root().join("nope")).is_err());
}

#[test]
fn a_module_is_reported_at_most_once() {
    let nixos = FakeNixos::new();

    // Two segments move at the same time; that is still one upgrade.
    let old = nixos.system("old", "26.05pre-git", "6.18.5", "260.2");
    let new = nixos.system("new", "26.05pre-git", "6.19.41", "260.2");

    assert_eq!(
        upgrades_available(&old, &new).unwrap(),
        vec!["Linux Kernel (6.18.5 -> 6.19.41)\n"]
    );
}

#[test]
fn a_kernel_downgrade_is_not_reported_as_an_upgrade() {
    let nixos = FakeNixos::new();

    let old = nixos.system("old", "26.05pre-git", "6.19.3", "260.2");
    let new = nixos.system("new", "26.05pre-git", "6.18.41", "260.2");

    assert!(upgrades_available(&old, &new).unwrap().is_empty());
}

#[test]
fn kernel_upgrade_is_detected() {
    let nixos = FakeNixos::new();

    let old = nixos.system("old", "26.05pre-git", "6.18.33", "260.2");
    let new = nixos.system("new", "26.05pre-git", "6.18.41", "260.2");

    assert_eq!(
        upgrades_available(&old, &new).unwrap(),
        vec!["Linux Kernel (6.18.33 -> 6.18.41)\n"]
    );
}

#[test]
fn identical_kernel_and_systemd_report_nothing() {
    let nixos = FakeNixos::new();

    let old = nixos.system("old", "26.05pre-git", "6.18.41", "260.2");
    let new = nixos.system("new", "26.05pre-git", "6.18.41", "260.2");

    assert_ne!(old, new);
    assert!(upgrades_available(&old, &new).unwrap().is_empty());
}

#[test]
fn systemd_upgrade_is_detected() {
    let nixos = FakeNixos::new();

    let old = nixos.system("old", "26.05pre-git", "6.18.41", "260.2");
    let new = nixos.system("new", "26.05pre-git", "6.18.41", "261.1");

    assert_eq!(
        upgrades_available(&old, &new).unwrap(),
        vec!["Systemd (260.2 -> 261.1)\n"]
    );
}

/// Both modules moving must produce both reasons, in module iteration order.
#[test]
fn a_kernel_and_systemd_upgrade_are_both_reported() {
    let nixos = FakeNixos::new();

    let old = nixos.system("old", "26.05pre-git", "6.18.41", "260.2");
    let new = nixos.system("new", "26.05pre-git", "6.19.1", "261.1");

    assert_eq!(
        upgrades_available(&old, &new).unwrap(),
        vec![
            "Linux Kernel (6.18.41 -> 6.19.1)\n",
            "Systemd (260.2 -> 261.1)\n"
        ]
    );
}

/// A systemd upgrade alone is reboot-worthy even when the kernel is unchanged,
/// and vice versa; neither module may mask the other.
#[test]
fn either_module_alone_is_enough_to_require_a_reboot() {
    let nixos = FakeNixos::new();

    let kernel_only = nixos.generations(("6.18.41", "260.2"), ("6.19.1", "260.2"));
    assert!(decide(&kernel_only.booted, &kernel_only.activated)
        .unwrap()
        .requires_reboot());

    let systemd = FakeNixos::new();
    let systemd_only = systemd.generations(("6.18.41", "260.2"), ("6.18.41", "261.1"));
    assert!(decide(&systemd_only.booted, &systemd_only.activated)
        .unwrap()
        .requires_reboot());
}

#[test]
fn missing_symlink_is_an_error_not_a_silent_pass() {
    let nixos = FakeNixos::new();
    let old = nixos.system("old", "26.05pre-git", "6.18.41", "260.2");

    assert!(upgrades_available(&old, &nixos.root().join("nope")).is_err());
}

/// A closure whose kernel symlink is a regular file rather than a symlink must
/// fail loudly. `read_link` refuses it, and the tool must not treat that as
/// "nothing changed".
#[test]
fn a_regular_file_where_a_symlink_belongs_is_an_error() {
    let nixos = FakeNixos::new();
    let old = nixos.system("old", "26.05pre-git", "6.18.41", "260.2");

    let broken = nixos.root().join("broken");
    fs::create_dir_all(&broken).unwrap();
    fs::write(broken.join("kernel"), "not a symlink").unwrap();

    assert!(upgrades_available(&old, &broken).is_err());
}
