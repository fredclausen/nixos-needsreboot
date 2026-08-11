//! The lifecycle of `/run/reboot-required`.
//!
//! The flag file is what other tooling reads, so when it exists, what it
//! contains, and when it is cleared are all part of the contract.

#![allow(clippy::unwrap_used)]

mod common;

use common::FakeNixos;
use nixos_needsreboot::cli::{Mode, Options, Recompute};
use nixos_needsreboot::{run, Error, Outcome, Privilege};
use std::fs;

const fn options(mode: Mode, recompute: Recompute) -> Options {
    Options { mode, recompute }
}

#[test]
fn a_required_reboot_writes_its_reasons_to_the_flag_file() {
    let nixos = FakeNixos::new();
    let paths = nixos.generations(("6.18.41", "260.2"), ("6.19.1", "261.1"));

    run(
        &options(Mode::Commit, Recompute::Always),
        &paths,
        Privilege::Root,
    )
    .unwrap();

    assert_eq!(
        fs::read_to_string(&paths.flag_file).unwrap(),
        "Linux Kernel (6.18.41 -> 6.19.1)\nSystemd (260.2 -> 261.1)\n"
    );
}

#[test]
fn no_required_reboot_leaves_no_flag_file() {
    let nixos = FakeNixos::new();
    let paths = nixos.generations(("6.18.41", "260.2"), ("6.18.41", "260.2"));

    run(
        &options(Mode::Commit, Recompute::Always),
        &paths,
        Privilege::Root,
    )
    .unwrap();

    assert!(!paths.flag_file.exists());
}

/// A flag file written for a previous generation must not outlive the reason
/// it was written for, or the machine claims a reboot is needed forever.
#[test]
fn a_stale_flag_file_is_cleared_when_the_reboot_is_no_longer_needed() {
    let nixos = FakeNixos::new();
    let paths = nixos.generations(("6.18.41", "260.2"), ("6.18.41", "260.2"));
    fs::write(&paths.flag_file, "Linux Kernel (6.1.0 -> 6.2.0)\n").unwrap();

    let outcome = run(
        &options(Mode::Commit, Recompute::Always),
        &paths,
        Privilege::Root,
    )
    .unwrap();

    assert_eq!(outcome, Outcome::NoRebootRequired);
    assert!(!paths.flag_file.exists());
}

#[test]
fn a_stale_flag_file_is_replaced_by_the_current_reasons() {
    let nixos = FakeNixos::new();
    let paths = nixos.generations(("6.18.41", "260.2"), ("6.19.1", "260.2"));
    fs::write(&paths.flag_file, "something from last week\n").unwrap();

    run(
        &options(Mode::Commit, Recompute::Always),
        &paths,
        Privilege::Root,
    )
    .unwrap();

    assert_eq!(
        fs::read_to_string(&paths.flag_file).unwrap(),
        "Linux Kernel (6.18.41 -> 6.19.1)\n"
    );
}

/// `--no-force-recompute` trusts an existing flag file: it reports a required
/// reboot without recomputing, and leaves the file exactly as it found it.
#[test]
fn no_force_recompute_trusts_an_existing_flag_file() {
    let nixos = FakeNixos::new();
    // The generations agree, so a recomputation would say "no reboot".
    let paths = nixos.generations(("6.18.41", "260.2"), ("6.18.41", "260.2"));
    fs::write(&paths.flag_file, "Linux Kernel (6.1.0 -> 6.2.0)\n").unwrap();

    let outcome = run(
        &options(Mode::Commit, Recompute::SkipIfFlagged),
        &paths,
        Privilege::Root,
    )
    .unwrap();

    assert_eq!(outcome, Outcome::RebootRequired);
    assert_eq!(
        fs::read_to_string(&paths.flag_file).unwrap(),
        "Linux Kernel (6.1.0 -> 6.2.0)\n"
    );
}

#[test]
fn no_force_recompute_still_computes_when_there_is_no_flag_file() {
    let nixos = FakeNixos::new();
    let paths = nixos.generations(("6.18.41", "260.2"), ("6.19.1", "260.2"));

    let outcome = run(
        &options(Mode::Commit, Recompute::SkipIfFlagged),
        &paths,
        Privilege::Root,
    )
    .unwrap();

    assert_eq!(outcome, Outcome::RebootRequired);
    assert_eq!(
        fs::read_to_string(&paths.flag_file).unwrap(),
        "Linux Kernel (6.18.41 -> 6.19.1)\n"
    );
}

/// A dry run must be observationally free of side effects, whichever way the
/// decision goes and whatever is already on disk.
#[test]
fn a_dry_run_never_touches_the_flag_file() {
    // Reboot required, no existing file: none may be created.
    let creating = FakeNixos::new();
    let paths = creating.generations(("6.18.41", "260.2"), ("6.19.1", "260.2"));
    run(
        &options(Mode::DryRun, Recompute::Always),
        &paths,
        Privilege::Root,
    )
    .unwrap();
    assert!(!paths.flag_file.exists());

    // No reboot required, existing file: it may not be removed.
    let clearing = FakeNixos::new();
    let paths = clearing.generations(("6.18.41", "260.2"), ("6.18.41", "260.2"));
    fs::write(&paths.flag_file, "left alone\n").unwrap();
    run(
        &options(Mode::DryRun, Recompute::Always),
        &paths,
        Privilege::Root,
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(&paths.flag_file).unwrap(),
        "left alone\n"
    );
}

/// An unreadable flag file must fail loudly rather than being silently ignored
/// and recomputed.
#[test]
fn an_unreadable_flag_file_is_an_error() {
    let nixos = FakeNixos::new();
    let paths = nixos.generations(("6.18.41", "260.2"), ("6.19.1", "260.2"));
    // A directory is readable as an entry but not as a string.
    fs::create_dir(&paths.flag_file).unwrap();

    let error = run(
        &options(Mode::Commit, Recompute::SkipIfFlagged),
        &paths,
        Privilege::Root,
    )
    .unwrap_err();

    assert!(matches!(error, Error::FlagFileRead { .. }));
}

/// If the flag file cannot be written, the run must fail rather than exit
/// claiming a reboot was recorded.
#[test]
fn an_unwritable_flag_file_is_an_error() {
    let nixos = FakeNixos::new();
    let mut paths = nixos.generations(("6.18.41", "260.2"), ("6.19.1", "260.2"));
    paths.flag_file = nixos.root().join("no/such/directory/reboot-required");

    let error = run(
        &options(Mode::Commit, Recompute::Always),
        &paths,
        Privilege::Root,
    )
    .unwrap_err();

    assert!(matches!(error, Error::FlagFileWrite { .. }));
}
