//! Parsing and describing the command line.

/// Whether the run is allowed to touch the filesystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Report the decision without creating or removing the flag file.
    DryRun,
    /// Record the decision in the flag file.
    Commit,
}

/// Whether an existing flag file short-circuits the check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recompute {
    /// Always recompute, discarding any existing flag file.
    Always,
    /// Trust an existing flag file and report its contents instead.
    SkipIfFlagged,
}

/// How much the run logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verbosity {
    /// Bare messages with no log decoration.
    Quiet,
    /// Structured logging at `INFO`.
    Info,
    /// Structured logging at `DEBUG`.
    Debug,
}

/// A parsed run configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Whether the flag file may be written.
    pub mode: Mode,
    /// Whether an existing flag file short-circuits the check.
    pub recompute: Recompute,
}

/// What the user asked the binary to do.
///
/// `Help` and `Version` are separate variants rather than flags on [`Options`]
/// so that they cannot reach the privilege check: both must work for an
/// unprivileged user, and that is a property of the type, not of the order of
/// statements in `main`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invocation {
    /// Print usage and exit successfully.
    Help,
    /// Print the version and exit successfully.
    Version,
    /// Perform a reboot check.
    Check(Options),
}

/// Parse `args` (including `argv[0]`) into an [`Invocation`] and a
/// [`Verbosity`].
///
/// Unrecognised arguments are ignored, matching the behaviour this tool has
/// always had.
#[must_use]
pub fn parse(args: &[String]) -> (Invocation, Verbosity) {
    let has = |flag: &str| args.iter().any(|arg| arg == flag);

    let verbosity = if has("--debug") {
        Verbosity::Debug
    } else if has("--logging-test") {
        Verbosity::Info
    } else {
        Verbosity::Quiet
    };

    let invocation = if has("--help") {
        Invocation::Help
    } else if has("--version") {
        Invocation::Version
    } else {
        Invocation::Check(Options {
            mode: if has("--dry-run") {
                Mode::DryRun
            } else {
                Mode::Commit
            },
            recompute: if has("--no-force-recompute") {
                Recompute::SkipIfFlagged
            } else {
                Recompute::Always
            },
        })
    };

    (invocation, verbosity)
}

/// The usage text printed for `--help`.
pub const HELP: &str = "\
nixos-needsreboot - Determine if a NixOS system reboot is required

USAGE:
  nixos-needsreboot [--dry-run] [--no-force-recompute] [--help] [--version] [--logging-test] [--debug]

OPTIONS:
  --dry-run               Print the reasons for needing a reboot without creating the reboot file
  --no-force-recompute    Do not recompute the reboot requirement if the reboot file already exists
  --help                  Print this help message
  --version               Print version information
  --logging-test          Enable logging for testing purposes
  --debug                 Enable debug logging

EXIT STATUS:
  0    No reboot is required
  1    The check could not be completed
  2    A reboot is required";

#[cfg(test)]
mod tests {
    use super::{parse, Invocation, Mode, Options, Recompute, Verbosity};

    fn args(rest: &[&str]) -> Vec<String> {
        std::iter::once("nixos-needsreboot")
            .chain(rest.iter().copied())
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn a_bare_invocation_commits_and_always_recomputes() {
        assert_eq!(
            parse(&args(&[])),
            (
                Invocation::Check(Options {
                    mode: Mode::Commit,
                    recompute: Recompute::Always,
                }),
                Verbosity::Quiet,
            )
        );
    }

    #[test]
    fn each_flag_selects_its_variant() {
        let (invocation, _) = parse(&args(&["--dry-run", "--no-force-recompute"]));
        assert_eq!(
            invocation,
            Invocation::Check(Options {
                mode: Mode::DryRun,
                recompute: Recompute::SkipIfFlagged,
            })
        );
    }

    #[test]
    fn help_outranks_version_and_the_check() {
        assert_eq!(parse(&args(&["--help"])).0, Invocation::Help);
        assert_eq!(
            parse(&args(&["--help", "--version", "--dry-run"])).0,
            Invocation::Help
        );
    }

    #[test]
    fn version_outranks_the_check() {
        assert_eq!(
            parse(&args(&["--version", "--dry-run"])).0,
            Invocation::Version
        );
    }

    #[test]
    fn debug_outranks_logging_test() {
        assert_eq!(parse(&args(&["--debug"])).1, Verbosity::Debug);
        assert_eq!(parse(&args(&["--logging-test"])).1, Verbosity::Info);
        assert_eq!(
            parse(&args(&["--logging-test", "--debug"])).1,
            Verbosity::Debug
        );
    }

    #[test]
    fn an_unknown_flag_is_ignored() {
        let (invocation, verbosity) = parse(&args(&["--nonsense"]));
        assert_eq!(
            invocation,
            Invocation::Check(Options {
                mode: Mode::Commit,
                recompute: Recompute::Always,
            })
        );
        assert_eq!(verbosity, Verbosity::Quiet);
    }

    /// A flag must be matched exactly; a substring must not trigger it.
    #[test]
    fn a_flag_is_matched_exactly() {
        assert_ne!(parse(&args(&["--dry-run-please"])).0, Invocation::Help);
        assert_eq!(
            parse(&args(&["--dry-run-please"])).0,
            Invocation::Check(Options {
                mode: Mode::Commit,
                recompute: Recompute::Always,
            })
        );
    }

    /// The documented exit statuses are part of the tool's contract, so the
    /// help text must keep describing them.
    #[test]
    fn help_documents_every_flag_and_exit_status() {
        for flag in [
            "--dry-run",
            "--no-force-recompute",
            "--help",
            "--version",
            "--logging-test",
            "--debug",
        ] {
            assert!(super::HELP.contains(flag), "help omits {flag}");
        }
        assert!(super::HELP.contains("EXIT STATUS"));
    }
}
