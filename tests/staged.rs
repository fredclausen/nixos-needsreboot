//! A generation staged for the next boot, as left by `nixos-rebuild boot`.
//!
//! Such a generation needs a reboot even when its kernel and systemd are
//! unchanged, because none of its configuration is live until then. A closure
//! activated by `nixos-rebuild test` is never registered in the profile and
//! must not be mistaken for one.

#![allow(clippy::unwrap_used)]

mod common;

use common::FakeNixos;
use nixos_needsreboot::cli::{Mode, Options, Recompute};
use nixos_needsreboot::{run, Error, Outcome, Privilege};
use std::{fs, os::unix::fs::symlink};

const LABEL: &str = "26.05pre-git";

fn commit() -> Options {
    Options {
        mode: Mode::Commit,
        recompute: Recompute::Always,
    }
}

/// The case this check exists for: the profile moved on, nothing the existing
/// module comparison looks at changed, and the machine is still running the
/// old generation.
#[test]
fn a_staged_generation_without_module_changes_requires_a_reboot() {
    let nixos = FakeNixos::new();
    let old = nixos.system("old", LABEL, "6.18.41", "260.2");
    let new = nixos.system("new", LABEL, "6.18.41", "260.2");
    let paths = nixos.layout(&old, &old, &new, &[&old, &new]);

    let outcome = run(&commit(), &paths, Privilege::Root).unwrap();

    assert_eq!(outcome, Outcome::RebootRequired);
    assert_eq!(
        fs::read_to_string(&paths.flag_file).unwrap(),
        "Staged generation (running 1 -> next boot 2)\n"
    );
}

#[test]
fn a_staged_generation_with_a_kernel_upgrade_reports_both() {
    let nixos = FakeNixos::new();
    let old = nixos.system("old", LABEL, "6.18.41", "260.2");
    let new = nixos.system("new", LABEL, "6.19.1", "260.2");
    let paths = nixos.layout(&old, &old, &new, &[&old, &new]);

    run(&commit(), &paths, Privilege::Root).unwrap();

    assert_eq!(
        fs::read_to_string(&paths.flag_file).unwrap(),
        "Linux Kernel (6.18.41 -> 6.19.1)\nStaged generation (running 1 -> next boot 2)\n"
    );
}

/// `switch` to one generation, then `boot` another: the running generation is
/// no longer the booted one, and it is still not the one that boots next.
#[test]
fn a_generation_staged_after_a_switch_is_detected() {
    let nixos = FakeNixos::new();
    let first = nixos.system("first", LABEL, "6.18.41", "260.2");
    let second = nixos.system("second", LABEL, "6.18.41", "260.2");
    let third = nixos.system("third", LABEL, "6.18.41", "260.2");
    let paths = nixos.layout(&first, &second, &third, &[&first, &second, &third]);

    run(&commit(), &paths, Privilege::Root).unwrap();

    assert_eq!(
        fs::read_to_string(&paths.flag_file).unwrap(),
        "Staged generation (running 2 -> next boot 3)\n"
    );
}

/// `nixos-rebuild boot --rollback` stages an *older* generation; that still
/// takes a reboot to reach.
#[test]
fn a_staged_rollback_is_detected() {
    let nixos = FakeNixos::new();
    let first = nixos.system("first", LABEL, "6.18.41", "260.2");
    let second = nixos.system("second", LABEL, "6.18.41", "260.2");
    let paths = nixos.layout(&first, &second, &first, &[&first, &second]);

    let outcome = run(&commit(), &paths, Privilege::Root).unwrap();

    assert_eq!(outcome, Outcome::RebootRequired);
    assert_eq!(
        fs::read_to_string(&paths.flag_file).unwrap(),
        "Staged generation (running 2 -> next boot 1)\n"
    );
}

/// `nixos-rebuild test` activates a closure that never enters the profile.
/// Rebooting would discard it rather than complete anything, so it is not a
/// staged generation.
#[test]
fn a_test_activation_is_not_a_staged_generation() {
    let nixos = FakeNixos::new();
    let booted = nixos.system("booted", LABEL, "6.18.41", "260.2");
    let tested = nixos.system("tested", LABEL, "6.18.41", "260.2");
    let paths = nixos.layout(&booted, &tested, &booted, &[&booted]);

    let outcome = run(&commit(), &paths, Privilege::Root).unwrap();

    assert_eq!(outcome, Outcome::NoRebootRequired);
    assert!(!paths.flag_file.exists());
}

/// A test activation must not hide a module upgrade that is already in the
/// profile.
#[test]
fn a_test_activation_does_not_mask_a_kernel_upgrade() {
    let nixos = FakeNixos::new();
    let booted = nixos.system("booted", LABEL, "6.18.41", "260.2");
    let profile = nixos.system("profile", LABEL, "6.19.1", "260.2");
    let tested = nixos.system("tested", LABEL, "6.19.1", "260.2");
    let paths = nixos.layout(&booted, &tested, &profile, &[&booted, &profile]);

    run(&commit(), &paths, Privilege::Root).unwrap();

    assert_eq!(
        fs::read_to_string(&paths.flag_file).unwrap(),
        "Linux Kernel (6.18.41 -> 6.19.1)\n"
    );
}

/// Once the staged generation has been switched to, nothing is pending; only
/// the module comparison against the booted closure remains.
#[test]
fn switching_to_the_staged_generation_clears_it() {
    let nixos = FakeNixos::new();
    let old = nixos.system("old", LABEL, "6.18.41", "260.2");
    let new = nixos.system("new", LABEL, "6.18.41", "260.2");
    let paths = nixos.layout(&old, &new, &new, &[&old, &new]);

    let outcome = run(&commit(), &paths, Privilege::Root).unwrap();

    assert_eq!(outcome, Outcome::NoRebootRequired);
}

/// A dry run reports a staged generation with the same status as a real run.
#[test]
fn a_dry_run_reports_a_staged_generation() {
    let nixos = FakeNixos::new();
    let old = nixos.system("old", LABEL, "6.18.41", "260.2");
    let new = nixos.system("new", LABEL, "6.18.41", "260.2");
    let paths = nixos.layout(&old, &old, &new, &[&old, &new]);

    let options = Options {
        mode: Mode::DryRun,
        recompute: Recompute::Always,
    };
    let outcome = run(&options, &paths, Privilege::Unprivileged).unwrap();

    assert_eq!(outcome, Outcome::RebootRequired);
    assert!(!paths.flag_file.exists());
}

/// A generation link that cannot be resolved is a broken profile, not a
/// reason to skip the staged-generation check.
#[test]
fn a_dangling_generation_link_is_an_error() {
    let nixos = FakeNixos::new();
    let system = nixos.system("only", LABEL, "6.18.41", "260.2");
    let paths = nixos.paths(&system, &system);
    symlink(
        nixos.root().join("gone"),
        paths.activated.with_file_name("system-9-link"),
    )
    .unwrap();

    let error = run(&commit(), &paths, Privilege::Root).unwrap_err();

    assert!(matches!(error, Error::UnresolvableSystem { .. }));
}

/// The running system must resolve; a dangling `/run/current-system` is a
/// broken machine, not a quiet "nothing staged".
#[test]
fn an_unresolvable_running_system_is_an_error() {
    let nixos = FakeNixos::new();
    let system = nixos.system("only", LABEL, "6.18.41", "260.2");
    let mut paths = nixos.paths(&system, &system);
    paths.current = nixos.root().join("run/missing-current-system");
    symlink(nixos.root().join("gone"), &paths.current).unwrap();

    let error = run(&commit(), &paths, Privilege::Root).unwrap_err();

    assert!(matches!(error, Error::UnresolvableSystem { .. }));
}
