// Original code from https://github.com/thefossguy/nixos-needsreboot
// Forked and updated by Fred Clausen https://github.com/fredclausen/nixos-needsreboot/

#![deny(
    clippy::pedantic,
    clippy::nursery,
    clippy::style,
    clippy::correctness,
    clippy::all,
    clippy::unwrap_used,
    clippy::expect_used
)]

use env_logger::Builder;
use log::{error, LevelFilter};
use nixos_needsreboot::cli::{self, Invocation, Verbosity};
use nixos_needsreboot::{run, Privilege, SystemPaths};
use sdre_rust_logging::SetupLogging;
use std::io::Write;
use std::process::ExitCode;

/// Exit status for a check that could not be completed.
const FAILED: u8 = 1;

fn enable_logging(verbosity: Verbosity) {
    match verbosity {
        Verbosity::Debug => "DEBUG".enable_logging(),
        Verbosity::Info => "INFO".enable_logging(),
        // The default output is consumed by humans and by scripts, so it
        // carries no timestamp or level decoration.
        Verbosity::Quiet => {
            let _ = Builder::new()
                .format(|buf, record| writeln!(buf, "{}", record.args()))
                .filter(None, LevelFilter::Info)
                .try_init();
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let (invocation, verbosity) = cli::parse(&args);

    enable_logging(verbosity);

    // `Help` and `Version` never reach the privilege check, because they are
    // not `Check` invocations. That is a property of the type rather than of
    // the order of statements here.
    let options = match invocation {
        Invocation::Help => {
            println!("{}", cli::HELP);
            return ExitCode::SUCCESS;
        }
        // Printed rather than logged, so that it lands on stdout next to
        // `--help` instead of on the logger's stderr.
        Invocation::Version => {
            println!("{}: v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Invocation::Check(options) => options,
    };

    let result =
        Privilege::from_env().and_then(|privilege| run(&options, &SystemPaths::nixos(), privilege));

    match result {
        Ok(outcome) => ExitCode::from(outcome.code()),
        Err(e) => {
            error!("{e}");
            ExitCode::from(FAILED)
        }
    }
}
