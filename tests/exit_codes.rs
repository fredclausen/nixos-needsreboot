//! The exit-status contract.
//!
//! Consumers script on these numbers, so they are the tool's real public API:
//! `0` no reboot required, `1` the check failed, `2` a reboot is required.

#![allow(clippy::unwrap_used)]

mod common;

use common::FakeNixos;
use nixos_needsreboot::cli::{Mode, Options, Recompute};
use nixos_needsreboot::{run, Error, Outcome, Privilege};

fn options(mode: Mode) -> Options {
    Options {
        mode,
        recompute: Recompute::Always,
    }
}

#[test]
fn an_unchanged_generation_exits_zero() {
    let nixos = FakeNixos::new();
    let paths = nixos.same_generation("6.18.41", "260.2");

    let outcome = run(&options(Mode::DryRun), &paths, Privilege::Root).unwrap();

    assert_eq!(outcome, Outcome::NoRebootRequired);
    assert_eq!(outcome.code(), 0);
}

#[test]
fn a_new_generation_without_reboot_worthy_changes_exits_zero() {
    let nixos = FakeNixos::new();
    let paths = nixos.generations(("6.18.41", "260.2"), ("6.18.41", "260.2"));

    let outcome = run(&options(Mode::DryRun), &paths, Privilege::Root).unwrap();

    assert_eq!(outcome, Outcome::NoRebootRequired);
    assert_eq!(outcome.code(), 0);
}

#[test]
fn a_kernel_upgrade_exits_two() {
    let nixos = FakeNixos::new();
    let paths = nixos.generations(("6.18.41", "260.2"), ("6.19.1", "260.2"));

    let outcome = run(&options(Mode::Commit), &paths, Privilege::Root).unwrap();

    assert_eq!(outcome, Outcome::RebootRequired);
    assert_eq!(outcome.code(), 2);
}

/// `--dry-run` used to exit `0` even when a reboot was required, which made the
/// documented `if nixos-needsreboot --dry-run; then ...` idiom always take the
/// "reboot required" branch. A dry run must report the same status as a real
/// run; only the side effects differ.
#[test]
fn a_dry_run_reports_the_same_status_as_a_real_run() {
    for (booted, activated, expected) in [
        (
            ("6.18.41", "260.2"),
            ("6.19.1", "260.2"),
            Outcome::RebootRequired,
        ),
        (
            ("6.18.41", "260.2"),
            ("6.18.41", "261.1"),
            Outcome::RebootRequired,
        ),
        (
            ("6.18.41", "260.2"),
            ("6.18.41", "260.2"),
            Outcome::NoRebootRequired,
        ),
    ] {
        let dry = FakeNixos::new();
        let dry_outcome = run(
            &options(Mode::DryRun),
            &dry.generations(booted, activated),
            Privilege::Root,
        )
        .unwrap();

        let wet = FakeNixos::new();
        let wet_outcome = run(
            &options(Mode::Commit),
            &wet.generations(booted, activated),
            Privilege::Root,
        )
        .unwrap();

        assert_eq!(
            dry_outcome, expected,
            "dry run of {booted:?} -> {activated:?}"
        );
        assert_eq!(dry_outcome, wet_outcome, "{booted:?} -> {activated:?}");
    }
}

/// A dry run needs no privileges; that is the entire point of the flag.
#[test]
fn a_dry_run_does_not_require_root() {
    let nixos = FakeNixos::new();
    let paths = nixos.generations(("6.18.41", "260.2"), ("6.19.1", "260.2"));

    let outcome = run(&options(Mode::DryRun), &paths, Privilege::Unprivileged).unwrap();

    assert_eq!(outcome, Outcome::RebootRequired);
}

#[test]
fn a_real_run_without_root_is_an_error() {
    let nixos = FakeNixos::new();
    let paths = nixos.generations(("6.18.41", "260.2"), ("6.19.1", "260.2"));

    let error = run(&options(Mode::Commit), &paths, Privilege::Unprivileged).unwrap_err();

    assert!(matches!(error, Error::NotRoot));
    // The privilege check must come before anything touches the filesystem.
    assert!(!paths.flag_file.exists());
}

#[test]
fn a_machine_without_the_system_profile_is_not_nixos() {
    let nixos = FakeNixos::new();
    let mut paths = nixos.same_generation("6.18.41", "260.2");
    paths.activated = nixos.root().join("definitely-not-here");

    let error = run(&options(Mode::DryRun), &paths, Privilege::Root).unwrap_err();

    assert!(matches!(error, Error::NotNixos { .. }));
}

/// A dangling profile symlink is a broken system, not a quiet "no reboot".
#[test]
fn an_unresolvable_system_profile_is_an_error() {
    let nixos = FakeNixos::new();
    let missing = nixos.root().join("gone");
    let system = nixos.system("only", "26.05pre-git", "6.18.41", "260.2");
    let mut paths = nixos.paths(&system, &system);
    paths.booted = {
        let link = nixos.root().join("dangling");
        std::os::unix::fs::symlink(&missing, &link).unwrap();
        link
    };

    let error = run(&options(Mode::DryRun), &paths, Privilege::Root).unwrap_err();

    assert!(matches!(error, Error::UnresolvableSystem { .. }));
}

/// An unreadable closure must surface as an error rather than being reported
/// as "no reboot needed", which would leave a machine on an old kernel.
#[test]
fn a_failed_comparison_is_never_reported_as_no_reboot() {
    let nixos = FakeNixos::new();
    let good = nixos.system("old", "26.05pre-git", "6.18.41", "260.2");
    let bare = nixos.root().join("bare");
    std::fs::create_dir_all(&bare).unwrap();

    let paths = nixos.paths(&good, &bare);
    let error = run(&options(Mode::DryRun), &paths, Privilege::Root).unwrap_err();

    assert!(matches!(error, Error::Compare(_)));
}
