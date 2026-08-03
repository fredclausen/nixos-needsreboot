// Original code from https://github.com/thefossguy/nixos-needsreboot
// Forked and updated by Fred Clausen https://github.com/fredclausen/nixos-needsreboot/

#![deny(
    clippy::pedantic,
    //clippy::cargo,
    clippy::nursery,
    clippy::style,
    clippy::correctness,
    clippy::all,
    clippy::unwrap_used,
    clippy::expect_used
)]

#[macro_use]
extern crate log;

use env_logger::Builder;
use log::LevelFilter;
use sdre_rust_logging::SetupLogging;
use std::env;
use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

mod compare_nixos_modules;
#[cfg(test)]
mod test_support;

pub static OLD_SYSTEM_PATH: &str = "/run/booted-system";
pub static NEW_SYSTEM_PATH: &str = "/nix/var/nix/profiles/system";
pub static NIXOS_NEEDS_REBOOT: &str = "/run/reboot-required";

fn enable_logging(env_args: &[String]) {
    if env_args.contains(&"--help".to_string()) {
        print_help();
        std::process::exit(0);
    }

    if env_args.contains(&"--logging-test".to_string()) || env_args.contains(&"--debug".to_string())
    {
        if env_args.contains(&"--debug".to_string()) {
            "DEBUG".enable_logging();
        } else {
            "INFO".enable_logging();
        }
    } else {
        let _ = Builder::new()
            .format(|buf, record| writeln!(buf, "{}", record.args()))
            .filter(None, LevelFilter::Info)
            .try_init();
    }
}

fn print_help() {
    println!("nixos-needsreboot - Determine if a NixOS system reboot is required");
    println!();
    println!("USAGE:");
    println!("  nixos-needsreboot [--dry-run] [--no-force-recompute] [--help] [--version] [--logging-test] [--debug]");
    println!();
    println!("OPTIONS:");
    println!("  --dry-run               Print the reasons for needing a reboot without creating the reboot file");
    println!("  --no-force-recompute    Do not recompute the reboot requirement if the reboot file already exists");
    println!("  --help                  Print this help message");
    println!("  --version               Print version information");
    println!("  --logging-test          Enable logging for testing purposes");
    println!("  --debug                 Enable debug logging");
}

fn version(env_args: &[String]) {
    if env_args.contains(&"--version".to_string()) {
        info!("{}: v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }
}

fn get_user(dry_run: bool) {
    let user_result = env::var_os("USER");

    let user = user_result.map_or_else(
        || {
            error!("Cannot determine current user");
            std::process::exit(1);
        },
        |val| {
            val.into_string().unwrap_or_else(|_| {
                error!("Cannot convert USER to String");
                std::process::exit(1);
            })
        },
    );

    if user != "root" && !dry_run {
        error!("Please run this as root");
        error!("HINT: use the '--dry-run' option");
        std::process::exit(1);
    }
}

/// Resolve a system profile symlink to the concrete `/nix/store` closure it
/// points at.
fn resolve_system(path: &str) -> PathBuf {
    match fs::canonicalize(path) {
        Ok(resolved) => {
            debug!("{path} resolves to {}", resolved.display());
            resolved
        }
        Err(e) => {
            error!("Cannot resolve {path}: {e}");
            std::process::exit(1);
        }
    }
}

/// Identify the booted and the activated generation by their store closure
/// paths.
///
/// The store path is the only reliable generation identity. `nixos-version`
/// holds `config.system.nixos.label`, which on a flake that does not embed a
/// revision in the label (e.g. a bare `25.11pre-git`) is byte-identical for
/// every generation ever built — using it as an identity makes every reboot
/// check a no-op.
fn verify_nixos_and_paths() -> (PathBuf, PathBuf) {
    if !Path::new(NEW_SYSTEM_PATH).exists() {
        error!("This binary is intended to run only on NixOS.");
        std::process::exit(1);
    }

    (
        resolve_system(OLD_SYSTEM_PATH),
        resolve_system(NEW_SYSTEM_PATH),
    )
}

fn maybe_skip_checks(no_force_recompute: bool) {
    if Path::new(NIXOS_NEEDS_REBOOT).exists() && no_force_recompute {
        let contents = match fs::read_to_string(NIXOS_NEEDS_REBOOT) {
            Ok(c) => c,
            Err(e) => {
                error!("Could not read existing {NIXOS_NEEDS_REBOOT}: {e}");
                std::process::exit(1);
            }
        };

        info!("Reboot needed: {}", contents.trim());

        std::process::exit(0);
    }
}

fn maybe_delete_old_reboot_file(dry_run: bool) {
    if Path::new(NIXOS_NEEDS_REBOOT).exists() && !dry_run {
        match fs::remove_file(NIXOS_NEEDS_REBOOT) {
            Ok(()) => {}
            Err(e) => {
                error!("Could not remove existing {NIXOS_NEEDS_REBOOT}: {e}");
                std::process::exit(1);
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum RebootDecision {
    /// The booted closure *is* the activated closure.
    SameGeneration,
    /// A new generation is activated, but neither the kernel nor systemd moved.
    NoRebootWorthyChange,
    /// A reboot is required; one reason per module that got newer.
    Needed(Vec<String>),
}

/// Decide whether the activated generation requires a reboot.
///
/// Generations are identified by their store closure path. `nixos-version`
/// must not be used for this: it holds `config.system.nixos.label`, which on a
/// flake that does not embed a revision in the label (e.g. a bare
/// `25.11pre-git`) is byte-identical for every generation ever built, so an
/// identity check based on it always reports `SameGeneration` and the kernel is
/// never compared.
fn decide(old_system: &Path, new_system: &Path) -> Result<RebootDecision, Box<dyn Error>> {
    if old_system == new_system {
        return Ok(RebootDecision::SameGeneration);
    }

    debug!(
        "Booted generation {} differs from activated generation {}",
        old_system.display(),
        new_system.display()
    );

    let reason = compare_nixos_modules::upgrades_available(old_system, new_system)?;

    if reason.is_empty() {
        return Ok(RebootDecision::NoRebootWorthyChange);
    }

    Ok(RebootDecision::Needed(reason))
}

fn generate_reason_for_reboot(old_system: &Path, new_system: &Path) -> Vec<String> {
    match decide(old_system, new_system) {
        Ok(RebootDecision::SameGeneration) => {
            info!("You are using the latest NixOS generation, no need to reboot");
            std::process::exit(0);
        }
        Ok(RebootDecision::NoRebootWorthyChange) => {
            info!("No updates available, moar uptime!!!");
            std::process::exit(0);
        }
        Ok(RebootDecision::Needed(reason)) => reason,
        Err(e) => {
            error!("Failed to compute upgrades:\n{e}");
            std::process::exit(1);
        }
    }
}

fn print_reasons(reason: &[String]) {
    for r in reason {
        info!("Upgrade available: {}", r.trim());
    }
}

fn write_reason_file(reason: &[String]) {
    let mut reason_out = String::new();

    for r in reason {
        reason_out.push_str(r);
    }

    match fs::write(NIXOS_NEEDS_REBOOT, &reason_out) {
        Ok(()) => {}
        Err(e) => {
            error!("Could not write {NIXOS_NEEDS_REBOOT}: {e}");
            std::process::exit(1);
        }
    }
    std::process::exit(2);
}

fn main() {
    let env_args: Vec<String> = env::args().collect();

    enable_logging(&env_args);
    version(&env_args);

    let dry_run = env_args.contains(&"--dry-run".to_string());
    let no_force_recompute = env_args.contains(&"--no-force-recompute".to_string());

    get_user(dry_run);

    let (old_system, new_system) = verify_nixos_and_paths();

    maybe_skip_checks(no_force_recompute);

    maybe_delete_old_reboot_file(dry_run);

    let reason = generate_reason_for_reboot(&old_system, &new_system);

    if dry_run {
        print_reasons(&reason);
    } else {
        write_reason_file(&reason);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
// Panicking on a failed setup step is the correct failure mode for a test; the
// crate-level `deny(clippy::unwrap_used)` is aimed at production code.
mod tests {
    use super::{decide, RebootDecision};
    use crate::test_support::fake_system;
    use std::fs;
    use tempfile::TempDir;

    /// Regression test for the bug that made reboot detection a permanent
    /// no-op: generation identity was read from `nixos-version`, which is just
    /// `config.system.nixos.label`. On a flake that does not embed a revision
    /// in the label every generation carries the same string, so the check
    /// always concluded "latest generation" and never compared the kernel.
    #[test]
    fn kernel_upgrade_is_detected_when_nixos_version_labels_match() {
        let tmp = TempDir::new().unwrap();
        let label = "26.05pre-git";

        let old = fake_system(tmp.path(), "old", label, "6.18.33", "260.2");
        let new = fake_system(tmp.path(), "new", label, "6.18.41", "260.2");

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
        let tmp = TempDir::new().unwrap();
        let system = fake_system(tmp.path(), "only", "26.05pre-git", "6.18.41", "260.2");

        assert_eq!(
            decide(&system, &system).unwrap(),
            RebootDecision::SameGeneration
        );
    }

    #[test]
    fn new_generation_without_kernel_or_systemd_change_needs_no_reboot() {
        let tmp = TempDir::new().unwrap();

        let old = fake_system(tmp.path(), "old", "26.05pre-git", "6.18.41", "260.2");
        let new = fake_system(tmp.path(), "new", "26.05pre-git", "6.18.41", "260.2");

        assert_ne!(old, new);
        assert_eq!(
            decide(&old, &new).unwrap(),
            RebootDecision::NoRebootWorthyChange
        );
    }

    #[test]
    fn unreadable_system_closure_is_an_error_not_a_silent_pass() {
        let tmp = TempDir::new().unwrap();
        let old = fake_system(tmp.path(), "old", "26.05pre-git", "6.18.41", "260.2");

        assert!(decide(&old, &tmp.path().join("nope")).is_err());
    }
}
