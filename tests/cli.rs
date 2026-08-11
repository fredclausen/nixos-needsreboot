//! End-to-end checks against the real binary.
//!
//! These run the compiled executable as a subprocess, so they cover the wiring
//! in `main` — argument parsing, logger setup, and the mapping from an outcome
//! to a process exit status — that in-process tests cannot reach.

#![allow(clippy::unwrap_used)]

use std::process::{Command, Output};

fn invoke(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nixos-needsreboot"))
        .args(args)
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

/// `--help` is handled before the privilege check, so it must work for an
/// unprivileged user. The test suite never runs as root, so simply passing is
/// the assertion.
#[test]
fn help_succeeds_without_root() {
    let output = invoke(&["--help"]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("USAGE:"));
    assert!(stdout(&output).contains("--no-force-recompute"));
}

/// The documented exit statuses must reach the user, since they are the only
/// thing a calling script can branch on.
#[test]
fn help_documents_the_exit_statuses() {
    let help = stdout(&invoke(&["--help"]));

    assert!(help.contains("EXIT STATUS"));
    assert!(help.contains("2    A reboot is required"));
}

#[test]
fn version_succeeds_without_root_and_prints_the_crate_version() {
    let output = invoke(&["--version"]);
    let printed = stdout(&output);

    assert_eq!(output.status.code(), Some(0));
    assert!(
        printed.contains(env!("CARGO_PKG_VERSION")),
        "version output was {printed:?}"
    );
    assert!(printed.contains("nixos-needsreboot"));
}

/// Both are informational and must not be blocked by the root check, in any
/// combination with the operational flags.
#[test]
fn help_and_version_outrank_the_operational_flags() {
    for args in [
        vec!["--help", "--dry-run"],
        vec!["--dry-run", "--help"],
        vec!["--version", "--no-force-recompute"],
    ] {
        let output = invoke(&args);
        assert_eq!(output.status.code(), Some(0), "for {args:?}");
    }
}

/// `--debug` must not change the answer, only the amount of logging. Combined
/// with `--help` it still exits successfully.
#[test]
fn debug_logging_does_not_change_the_exit_status() {
    assert_eq!(invoke(&["--help", "--debug"]).status.code(), Some(0));
    assert_eq!(invoke(&["--version", "--debug"]).status.code(), Some(0));
}

/// `--help` and `--version` are the two informational modes and must agree on
/// where their output goes; `--version` used to be logged to stderr while
/// `--help` was printed to stdout.
#[test]
fn the_informational_modes_both_write_to_stdout() {
    for flag in ["--help", "--version"] {
        let output = invoke(&[flag]);
        assert!(
            !stdout(&output).is_empty(),
            "{flag} wrote nothing to stdout"
        );
        assert!(
            stderr(&output).is_empty(),
            "{flag} wrote {:?} to stderr",
            stderr(&output)
        );
    }
}

/// Every real run must terminate with one of the three documented statuses.
/// This asserts the binary never panics or dies on a signal, whatever the host
/// looks like: on a NixOS box it yields 0 or 2, elsewhere 1.
#[test]
fn a_real_run_always_exits_with_a_documented_status() {
    let status = invoke(&["--dry-run"]).status;

    // 0 no reboot, 1 check failed, 2 reboot required.
    assert!(
        matches!(status.code(), Some(0..=2)),
        "undocumented exit status {status:?}"
    );
}
